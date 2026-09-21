use super::clock::{RecordingClock, ticks};
use anyhow::{Context as _, Result};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{
    thread::{self, JoinHandle},
    time::Duration,
};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    encoder::VideoEncoder,
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
};

#[derive(Clone)]
pub struct CaptureFlags {
    pub encoder: Arc<Mutex<Option<VideoEncoder>>>,
    pub clock: Arc<RecordingClock>,
    pub target_closed: Arc<AtomicBool>,
    pub captured_frames: Arc<AtomicU64>,
    pub target_width: u32,
    pub target_height: u32,
    pub frame_ticks: i64,
    pub frame_period: Duration,
}

pub struct CaptureHandler {
    encoder: Arc<Mutex<Option<VideoEncoder>>>,
    clock: Arc<RecordingClock>,
    target_closed: Arc<AtomicBool>,
    captured_frames: Arc<AtomicU64>,
    target_width: u32,
    target_height: u32,
    latest: Arc<Mutex<Option<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
    staging: Vec<u8>,
    flipped: Vec<u8>,
}

impl CaptureHandler {
    pub fn finish_encoder(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        let worker_result = self
            .worker
            .take()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("映像エンコードスレッドが終了しました。"))?
            })
            .unwrap_or(Ok(()));
        if let Some(encoder) = self.encoder.lock().take() {
            encoder.finish().context("MP4ファイルを確定できません。")?;
        }
        worker_result
    }
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = CaptureFlags;
    type Error = anyhow::Error;

    fn new(context: Context<Self::Flags>) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_latest = latest.clone();
        let worker_stop = stop.clone();
        let flags = context.flags.clone();
        let worker = thread::Builder::new()
            .name("purplecapture-video-clock".into())
            .spawn(move || {
                let result = encode_video(&flags, worker_latest, worker_stop);
                if result.is_err() {
                    flags.target_closed.store(true, Ordering::Release);
                }
                result
            })?;
        Ok(Self {
            encoder: context.flags.encoder,
            clock: context.flags.clock,
            target_closed: context.flags.target_closed,
            captured_frames: context.flags.captured_frames,
            target_width: context.flags.target_width,
            target_height: context.flags.target_height,
            latest,
            stop,
            worker: Some(worker),
            staging: Vec::new(),
            flipped: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<()> {
        if self.clock.is_paused() {
            return Ok(());
        }
        let source_width = frame.width() as usize;
        let source_height = frame.height() as usize;
        let target_width = self.target_width as usize;
        let target_height = self.target_height as usize;
        let frame_buffer = frame.buffer().context("録画フレームを読み出せません。")?;
        let pixels = frame_buffer.as_nopadding_buffer(&mut self.staging);
        self.captured_frames.fetch_add(1, Ordering::Relaxed);
        self.flipped.resize(target_width * target_height * 4, 0);
        self.flipped.fill(0);
        let copy_width = source_width.min(target_width);
        let copy_height = source_height.min(target_height);
        let source_stride = source_width * 4;
        let target_stride = target_width * 4;
        for source_y in 0..copy_height {
            let source_start = source_y * source_stride;
            let target_y = target_height - 1 - source_y;
            let target_start = target_y * target_stride;
            self.flipped[target_start..target_start + copy_width * 4]
                .copy_from_slice(&pixels[source_start..source_start + copy_width * 4]);
        }
        // A separate clock repeats this image when WGC produces no new frame.
        let mut latest = self.latest.lock();
        let previous = latest.replace(std::mem::take(&mut self.flipped));
        self.flipped = previous.unwrap_or_default();
        Ok(())
    }

    fn on_closed(&mut self) -> Result<()> {
        self.target_closed.store(true, Ordering::Release);
        Ok(())
    }
}

impl Drop for CaptureHandler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn encode_video(
    flags: &CaptureFlags,
    latest: Arc<Mutex<Option<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let mut last_timestamp = None;
    loop {
        let stopping = stop.load(Ordering::Acquire);
        let elapsed = flags.clock.elapsed();
        let timestamp = if stopping {
            ticks(elapsed.saturating_sub(flags.frame_period))
        } else {
            ticks(elapsed) / flags.frame_ticks * flags.frame_ticks
        };
        if (stopping || !flags.clock.is_paused())
            && last_timestamp.is_none_or(|last| timestamp > last)
        {
            let pixels = latest.lock();
            if let Some(pixels) = pixels.as_ref() {
                let mut encoder = flags.encoder.lock();
                let encoder = encoder
                    .as_mut()
                    .context("映像エンコーダーが終了しています。")?;
                // Anchor at zero even when the first WGC frame arrives late.
                if last_timestamp.is_none() {
                    encoder.send_frame_buffer(pixels, 0)?;
                    last_timestamp = Some(0);
                }
                if timestamp > last_timestamp.unwrap_or(0) {
                    encoder
                        .send_frame_buffer(pixels, timestamp)
                        .context("映像フレームをエンコードできません。")?;
                    last_timestamp = Some(timestamp);
                }
            }
        }
        if stopping {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_capture::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoSettingsBuilder,
    };

    // Generated pixels and silence only; never records the user's desktop.
    #[test]
    #[ignore = "Windows Media Foundationによる実MP4生成テスト"]
    fn media_clock_preserves_static_video_and_audio_duration() {
        for fps in [30, 60] {
            let directory = tempfile::tempdir().unwrap();
            let output = directory.path().join("clock.mp4");
            let encoder = VideoEncoder::new(
                VideoSettingsBuilder::new(640, 360)
                    .frame_rate(fps)
                    .sub_type(windows_capture::encoder::VideoSettingsSubType::H264)
                    .bitrate(2_000_000),
                AudioSettingsBuilder::new()
                    .channel_count(2)
                    .sample_rate(48_000)
                    .bit_per_sample(16),
                ContainerSettingsBuilder::new(),
                &output,
            )
            .unwrap();
            let clock = Arc::new(RecordingClock::new());
            let encoder = Arc::new(Mutex::new(Some(encoder)));
            let flags = CaptureFlags {
                encoder: encoder.clone(),
                clock: clock.clone(),
                target_closed: Arc::new(AtomicBool::new(false)),
                captured_frames: Arc::new(AtomicU64::new(0)),
                target_width: 640,
                target_height: 360,
                frame_ticks: 10_000_000 / fps as i64,
                frame_period: Duration::from_secs_f64(1.0 / fps as f64),
            };
            let latest = Arc::new(Mutex::new(None));
            let stop = Arc::new(AtomicBool::new(false));
            let video_stop = stop.clone();
            let video_latest = latest.clone();
            let video = thread::spawn(move || encode_video(&flags, video_latest, video_stop));
            let audio_stop = stop.clone();
            let audio_clock = clock.clone();
            let audio_encoder = encoder.clone();
            let audio_error = Arc::new(Mutex::new(None));
            let error = audio_error.clone();
            let audio = thread::spawn(move || {
                super::super::audio::mix_loop(
                    Vec::new(),
                    audio_encoder,
                    audio_clock,
                    audio_stop,
                    error,
                )
            });
            thread::sleep(Duration::from_millis(100));
            *latest.lock() = Some(vec![128; 640 * 360 * 4]);
            thread::sleep(Duration::from_millis(900));
            clock.pause();
            thread::sleep(Duration::from_millis(500));
            clock.resume();
            thread::sleep(Duration::from_millis(1000));
            clock.stop();
            let expected = clock.elapsed().as_secs_f64();
            stop.store(true, Ordering::Release);
            audio.join().unwrap();
            video.join().unwrap().unwrap();
            assert!(audio_error.lock().is_none());
            encoder.lock().take().unwrap().finish().unwrap();
            let data = std::fs::read(&output).unwrap();
            let mut durations = Vec::new();
            media_durations(&data, &mut durations);
            assert_eq!(durations.len(), 2, "expected audio and video tracks");
            for duration in &durations {
                assert!(
                    (duration - expected).abs() < 0.15,
                    "{fps} FPS: track {duration:.3}s vs clock {expected:.3}s"
                );
            }
            assert!(
                (durations[0] - durations[1]).abs() < 0.1,
                "A/V durations: {durations:?}"
            );
            println!("{fps} FPS, clock {expected:.3}s, MP4 tracks {durations:?}");
        }
    }

    fn media_durations(mut bytes: &[u8], output: &mut Vec<f64>) {
        while bytes.len() >= 8 {
            let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
            if length < 8 || length > bytes.len() {
                break;
            }
            let kind = &bytes[4..8];
            let body = &bytes[8..length];
            if kind == b"mdhd" {
                let (scale, duration) = if body[0] == 1 {
                    (
                        u32::from_be_bytes(body[20..24].try_into().unwrap()),
                        u64::from_be_bytes(body[24..32].try_into().unwrap()),
                    )
                } else {
                    (
                        u32::from_be_bytes(body[12..16].try_into().unwrap()),
                        u32::from_be_bytes(body[16..20].try_into().unwrap()) as u64,
                    )
                };
                output.push(duration as f64 / scale as f64);
            } else if kind == b"moov" || kind == b"trak" || kind == b"mdia" {
                media_durations(body, output);
            }
            bytes = &bytes[length..];
        }
    }
}
