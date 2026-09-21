use super::clock::RecordingClock;
use crate::models::AudioDevice;
use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat, initialize_mta};
use windows_capture::encoder::VideoEncoder;

const SAMPLE_RATE: usize = 48_000;
const CHANNELS: usize = 2;
const BYTES_PER_SAMPLE: usize = 2;
const CHUNK_FRAMES: usize = 960;
const CHUNK_BYTES: usize = CHUNK_FRAMES * CHANNELS * BYTES_PER_SAMPLE;

pub fn list_microphones() -> Result<Vec<AudioDevice>> {
    let _ = initialize_mta().ok();
    let enumerator = DeviceEnumerator::new().context("音声デバイス列挙を開始できません。")?;
    let collection = enumerator
        .get_device_collection(&Direction::Capture)
        .context("マイク一覧を取得できません。")?;
    let mut devices = Vec::new();
    for device in &collection {
        let Ok(device) = device else { continue };
        let Ok(id) = device.get_id() else { continue };
        let name = device
            .get_friendlyname()
            .unwrap_or_else(|_| "マイク".to_string());
        devices.push(AudioDevice { id, name });
    }
    Ok(devices)
}

#[derive(Clone)]
enum InputKind {
    System,
    Microphone(Option<String>),
}

pub struct AudioPipeline {
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}

impl AudioPipeline {
    pub fn start(
        system_audio: bool,
        microphone: bool,
        microphone_id: Option<String>,
        clock: Arc<RecordingClock>,
        encoder: Arc<Mutex<Option<VideoEncoder>>>,
    ) -> Result<Option<Self>> {
        if !system_audio && !microphone {
            return Ok(None);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let mut handles = Vec::new();
        let mut receivers = Vec::new();

        if system_audio {
            let (sender, receiver) = mpsc::sync_channel(8);
            receivers.push(receiver);
            handles.push(spawn_input(
                InputKind::System,
                sender,
                stop.clone(),
                error.clone(),
            )?);
        }
        if microphone {
            let (sender, receiver) = mpsc::sync_channel(8);
            receivers.push(receiver);
            handles.push(spawn_input(
                InputKind::Microphone(microphone_id),
                sender,
                stop.clone(),
                error.clone(),
            )?);
        }

        let mixer_stop = stop.clone();
        let mixer_error = error.clone();
        handles.push(
            thread::Builder::new()
                .name("purplecapture-audio-mixer".into())
                .spawn(move || {
                    mix_loop(receivers, encoder, clock, mixer_stop, mixer_error);
                })
                .context("音声ミキサースレッドを開始できません。")?,
        );
        Ok(Some(Self {
            stop,
            handles,
            error,
        }))
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn spawn_input(
    kind: InputKind,
    sender: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(match kind {
            InputKind::System => "purplecapture-system-audio".into(),
            InputKind::Microphone(_) => "purplecapture-microphone".into(),
        })
        .spawn(move || {
            if let Err(cause) = capture_loop(kind, sender, stop) {
                *error.lock() = Some(format!("音声録音に失敗しました: {cause:#}"));
            }
        })
        .context("WASAPI録音スレッドを開始できません。")
}

fn capture_loop(kind: InputKind, sender: SyncSender<Vec<u8>>, stop: Arc<AtomicBool>) -> Result<()> {
    initialize_mta()
        .ok()
        .context("WASAPIスレッドを初期化できません。")?;
    let enumerator = DeviceEnumerator::new()?;
    let device = match kind {
        InputKind::System => enumerator.get_default_device(&Direction::Render)?,
        InputKind::Microphone(Some(id)) => enumerator.get_device(&id)?,
        InputKind::Microphone(None) => enumerator.get_default_device(&Direction::Capture)?,
    };
    let mut client = device.get_iaudioclient()?;
    let format = WaveFormat::new(16, 16, &SampleType::Int, SAMPLE_RATE, CHANNELS, None);
    let (_, minimum_period) = client.get_device_period()?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: minimum_period,
    };
    // Render endpoint + Capture direction enables WASAPI loopback.
    client.initialize_client(&format, &Direction::Capture, &mode)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    let mut queue = VecDeque::with_capacity(CHUNK_BYTES * 8);
    client.start_stream()?;

    while !stop.load(Ordering::Acquire) {
        capture.read_from_device_to_deque(&mut queue)?;
        while queue.len() >= CHUNK_BYTES {
            let chunk: Vec<u8> = queue.drain(..CHUNK_BYTES).collect();
            match sender.try_send(chunk) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    let _ = client.stop_stream();
                    return Ok(());
                }
            }
        }
        let _ = event.wait_for_event(200);
    }
    let _ = client.stop_stream();
    Ok(())
}

pub(super) fn mix_loop(
    receivers: Vec<Receiver<Vec<u8>>>,
    encoder: Arc<Mutex<Option<VideoEncoder>>>,
    clock: Arc<RecordingClock>,
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    let mut sent_frames = 0_u64;
    loop {
        let stopping = stop.load(Ordering::Acquire);
        let target_frames = (clock.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
        if clock.is_paused() && !stopping {
            for receiver in &receivers {
                while receiver.try_recv().is_ok() {}
            }
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let remaining = target_frames.saturating_sub(sent_frames);
        if remaining < CHUNK_FRAMES as u64 && !stopping {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        if remaining == 0 && stopping {
            break;
        }
        let output = mix_available(&receivers);
        let frames = remaining.min(CHUNK_FRAMES as u64) as usize;
        let output = &output[..frames * CHANNELS * BYTES_PER_SAMPLE];
        let mut guard = encoder.lock();
        let Some(encoder) = guard.as_mut() else { break };
        if let Err(cause) = encoder.send_audio_buffer(output, 0) {
            *error.lock() = Some(format!("音声エンコードに失敗しました: {cause}"));
            break;
        }
        sent_frames += frames as u64;
    }
}

fn mix_available(receivers: &[Receiver<Vec<u8>>]) -> Vec<u8> {
    let chunks: Vec<Vec<u8>> = receivers.iter().filter_map(|r| r.try_recv().ok()).collect();
    if chunks.is_empty() {
        vec![0; CHUNK_BYTES]
    } else {
        mix_pcm_i16(&chunks)
    }
}

fn mix_pcm_i16(chunks: &[Vec<u8>]) -> Vec<u8> {
    let length = chunks.iter().map(Vec::len).min().unwrap_or_default() & !1;
    let mut result = vec![0_u8; length];
    for offset in (0..length).step_by(2) {
        let sum: i32 = chunks
            .iter()
            .map(|chunk| i16::from_le_bytes([chunk[offset], chunk[offset + 1]]) as i32)
            .sum();
        let mixed = sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        result[offset..offset + 2].copy_from_slice(&mixed.to_le_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_first_input_preserves_every_healthy_chunk() {
        let (_stalled, first) = mpsc::sync_channel(8);
        let (healthy, second) = mpsc::sync_channel(8);
        let inputs = [first, second];
        for value in 1_u8..=100 {
            let chunk = vec![value; CHUNK_BYTES];
            healthy.send(chunk.clone()).unwrap();
            assert_eq!(mix_available(&inputs), chunk);
        }
        drop(healthy);
        assert_eq!(mix_available(&inputs), vec![0; CHUNK_BYTES]);
    }

    #[test]
    fn mixing_clamps_instead_of_wrapping() {
        assert_eq!(
            mix_pcm_i16(&[
                30000_i16.to_le_bytes().to_vec(),
                10000_i16.to_le_bytes().to_vec()
            ]),
            i16::MAX.to_le_bytes()
        );
        assert_eq!(
            mix_pcm_i16(&[
                (-30000_i16).to_le_bytes().to_vec(),
                (-10000_i16).to_le_bytes().to_vec()
            ]),
            i16::MIN.to_le_bytes()
        );
    }
}
