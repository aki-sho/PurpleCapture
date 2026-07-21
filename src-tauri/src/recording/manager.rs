use super::{
    audio::AudioPipeline,
    capture::{CaptureFlags, CaptureHandler},
};
use crate::{
    history::HistoryStore,
    models::{HistoryEntry, RecordingRequest, RecordingStatus},
    portable_paths::PortablePaths,
    sources::ResolvedSource,
};
use anyhow::{Context, Result, bail};
use chrono::Local;
use parking_lot::Mutex;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use windows_capture::{
    capture::{CaptureControl, GraphicsCaptureApiHandler},
    encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    },
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
};

type Control = CaptureControl<CaptureHandler, anyhow::Error>;

struct ActiveRecording {
    control: Option<Control>,
    audio: Option<AudioPipeline>,
    paused: Arc<AtomicBool>,
    target_closed: Arc<AtomicBool>,
    captured_frames: Arc<AtomicU64>,
    started: Instant,
    pause_started: Option<Instant>,
    paused_duration: Duration,
    source_id: String,
    target_name: String,
    working_path: PathBuf,
    final_path: PathBuf,
    created_at: String,
}

struct RecordingInner {
    active: Option<ActiveRecording>,
    last_error: Option<String>,
}

pub struct RecordingManager {
    paths: Arc<PortablePaths>,
    history: Arc<HistoryStore>,
    inner: Mutex<RecordingInner>,
}

impl RecordingManager {
    pub fn new(paths: Arc<PortablePaths>, history: Arc<HistoryStore>) -> Self {
        Self {
            paths,
            history,
            inner: Mutex::new(RecordingInner {
                active: None,
                last_error: None,
            }),
        }
    }

    pub fn start(
        &self,
        request: RecordingRequest,
        source: ResolvedSource,
        save_directory: PathBuf,
    ) -> Result<RecordingStatus> {
        self.validate_request(&request)?;
        if self.inner.lock().active.is_some() {
            bail!("すでに録画中です。");
        }
        if fs2::available_space(&save_directory).unwrap_or(u64::MAX) < 256 * 1024 * 1024 {
            bail!("保存先の空き容量が不足しています（最低256 MB必要です）。");
        }
        let timestamp = Local::now();
        let stem = format!("PurpleCapture_{}", timestamp.format("%Y-%m-%d_%H-%M-%S"));
        let working_path = self.paths.working.join(format!("{stem}.mp4.part"));
        let final_path = unique_path(save_directory.join(format!("{stem}.mp4")));
        let (width, height) = match &source {
            ResolvedSource::Monitor { width, height, .. }
            | ResolvedSource::Window { width, height, .. } => (*width, *height),
        };
        let target_width = make_even(width);
        let target_height = make_even(height);
        if target_width > 16_384 || target_height > 16_384 {
            bail!("録画対象のサイズが大きすぎます。");
        }
        let bitrate = match request.quality.as_str() {
            "high" => {
                if request.fps == 60 {
                    30_000_000
                } else {
                    20_000_000
                }
            }
            _ => {
                if request.fps == 60 {
                    16_000_000
                } else {
                    10_000_000
                }
            }
        };
        let audio_enabled = request.system_audio || request.microphone;
        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(target_width, target_height)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(bitrate)
                .frame_rate(request.fps),
            AudioSettingsBuilder::new()
                .channel_count(2)
                .sample_rate(48_000)
                .bit_per_sample(16)
                .disabled(!audio_enabled),
            ContainerSettingsBuilder::new(),
            &working_path,
        )
        .context("Media Foundation H.264/MP4エンコーダーを開始できません。")?;
        let encoder = Arc::new(Mutex::new(Some(encoder)));
        let paused = Arc::new(AtomicBool::new(false));
        let target_closed = Arc::new(AtomicBool::new(false));
        let captured_frames = Arc::new(AtomicU64::new(0));
        let flags = CaptureFlags {
            encoder: encoder.clone(),
            paused: paused.clone(),
            target_closed: target_closed.clone(),
            captured_frames: captured_frames.clone(),
            target_width,
            target_height,
            frame_ticks: 10_000_000_i64 / request.fps as i64,
            frame_period: Duration::from_secs_f64(1.0 / request.fps as f64),
        };
        // Windows 10の一部ビルドはMinimumUpdateIntervalを実装していないため、
        // WGC側はDefaultとし、CaptureHandlerで30/60 FPSへ間引く。
        let interval = MinimumUpdateIntervalSettings::Default;
        let control = match source {
            ResolvedSource::Monitor { monitor, .. } => {
                CaptureHandler::start_free_threaded(Settings::new(
                    monitor,
                    CursorCaptureSettings::Default,
                    DrawBorderSettings::Default,
                    SecondaryWindowSettings::Default,
                    interval,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flags,
                ))
            }
            ResolvedSource::Window { window, .. } => {
                CaptureHandler::start_free_threaded(Settings::new(
                    window,
                    CursorCaptureSettings::Default,
                    DrawBorderSettings::Default,
                    SecondaryWindowSettings::Default,
                    interval,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    flags,
                ))
            }
        }
        .context("Windows.Graphics.Captureを開始できません。")?;

        let audio = match AudioPipeline::start(
            request.system_audio,
            request.microphone,
            request.microphone_id,
            paused.clone(),
            encoder,
        ) {
            Ok(audio) => audio,
            Err(cause) => {
                let _ = control.stop();
                let _ = fs::remove_file(&working_path);
                return Err(cause);
            }
        };
        self.paths.log(
            "INFO",
            format!("Recording started: {}", request.source.name),
        );
        self.inner.lock().active = Some(ActiveRecording {
            control: Some(control),
            audio,
            paused,
            target_closed,
            captured_frames,
            started: Instant::now(),
            pause_started: None,
            paused_duration: Duration::ZERO,
            source_id: request.source.id,
            target_name: request.source.name,
            working_path,
            final_path,
            created_at: timestamp.to_rfc3339(),
        });
        Ok(self.status())
    }

    pub fn pause(&self) -> Result<RecordingStatus> {
        let mut inner = self.inner.lock();
        let active = inner.active.as_mut().context("録画中ではありません。")?;
        if active.pause_started.is_none() {
            active.paused.store(true, Ordering::Release);
            active.pause_started = Some(Instant::now());
        }
        drop(inner);
        Ok(self.status())
    }

    pub fn resume(&self) -> Result<RecordingStatus> {
        let mut inner = self.inner.lock();
        let active = inner.active.as_mut().context("録画中ではありません。")?;
        if let Some(started) = active.pause_started.take() {
            active.paused_duration += started.elapsed();
            active.paused.store(false, Ordering::Release);
        }
        drop(inner);
        Ok(self.status())
    }

    pub fn stop(&self) -> Result<RecordingStatus> {
        let active = self
            .inner
            .lock()
            .active
            .take()
            .context("録画中ではありません。")?;
        match self.finalize(active, false) {
            Ok(()) => {
                self.inner.lock().last_error = None;
                Ok(self.status())
            }
            Err(cause) => {
                let message = format!("{cause:#}");
                self.paths.log("ERROR", &message);
                self.inner.lock().last_error = Some(message.clone());
                Err(anyhow::anyhow!(message))
            }
        }
    }

    pub fn stop_for_shutdown(&self) {
        if let Some(active) = self.inner.lock().active.take() {
            if let Err(cause) = self.finalize(active, false) {
                self.paths.log(
                    "ERROR",
                    format!("Shutdown recording finalization failed: {cause:#}"),
                );
            }
        }
    }

    pub fn stop_if_source(
        &self,
        source_id: &str,
        message: &str,
    ) -> Result<Option<RecordingStatus>> {
        let active = {
            let mut inner = self.inner.lock();
            if !inner
                .active
                .as_ref()
                .is_some_and(|active| active.source_id == source_id)
            {
                return Ok(None);
            }
            inner.active.take().context("録画状態を取得できません。")?
        };
        match self.finalize(active, false) {
            Ok(()) => {
                self.inner.lock().last_error = Some(message.into());
                Ok(Some(self.status()))
            }
            Err(cause) => {
                let detail = format!("{message} 終了処理エラー: {cause:#}");
                self.paths.log("ERROR", &detail);
                self.inner.lock().last_error = Some(detail.clone());
                Err(anyhow::anyhow!(detail))
            }
        }
    }

    pub fn status(&self) -> RecordingStatus {
        self.reap_finished();
        let inner = self.inner.lock();
        let Some(active) = inner.active.as_ref() else {
            return RecordingStatus {
                error: inner.last_error.clone(),
                ..RecordingStatus::default()
            };
        };
        let current_pause = active
            .pause_started
            .map(|value| value.elapsed())
            .unwrap_or_default();
        let elapsed = active
            .started
            .elapsed()
            .saturating_sub(active.paused_duration + current_pause);
        let elapsed_seconds = elapsed.as_secs();
        let audio_error = active.audio.as_ref().and_then(AudioPipeline::error);
        RecordingStatus {
            state: if active.pause_started.is_some() {
                "paused".into()
            } else {
                "recording".into()
            },
            elapsed_seconds,
            target_name: active.target_name.clone(),
            error: audio_error,
        }
    }

    fn reap_finished(&self) {
        let should_reap = self.inner.lock().active.as_ref().is_some_and(|active| {
            active.target_closed.load(Ordering::Acquire)
                || active
                    .control
                    .as_ref()
                    .is_some_and(CaptureControl::is_finished)
        });
        if !should_reap {
            return;
        }
        let Some(active) = self.inner.lock().active.take() else {
            return;
        };
        let result = self.finalize(active, true);
        let mut inner = self.inner.lock();
        inner.last_error = Some(match result {
            Ok(()) => "録画対象が閉じられたため、安全に録画を停止しました。".into(),
            Err(cause) => format!("録画対象の終了処理に失敗しました: {cause:#}"),
        });
    }

    fn finalize(&self, mut active: ActiveRecording, already_finished: bool) -> Result<()> {
        active.paused.store(false, Ordering::Release);
        if let Some(audio) = active.audio.take() {
            audio.stop();
        }
        let control = active.control.take().context("録画制御がありません。")?;
        let callback = control.callback();
        if already_finished {
            control
                .wait()
                .context("録画スレッドの終了を確認できません。")?;
        } else {
            control.stop().context("録画スレッドを停止できません。")?;
        }
        callback.lock().finish_encoder()?;
        let captured_frames = active.captured_frames.load(Ordering::Acquire);
        let current_pause = active
            .pause_started
            .map(|value| value.elapsed())
            .unwrap_or_default();
        let elapsed = active
            .started
            .elapsed()
            .saturating_sub(active.paused_duration + current_pause);
        if !active.working_path.is_file() || fs::metadata(&active.working_path)?.len() == 0 {
            bail!("録画データが生成されませんでした。");
        }
        move_completed_file(&active.working_path, &active.final_path)?;
        let size = fs::metadata(&active.final_path)?.len();
        self.history.add(HistoryEntry {
            path: active.final_path.to_string_lossy().into_owned(),
            file_name: active
                .final_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            target_name: active.target_name.clone(),
            created_at: active.created_at,
            duration_seconds: elapsed.as_secs(),
            size_bytes: size,
        })?;
        self.paths.log(
            "INFO",
            format!(
                "Recording saved: {} ({} bytes, {} frames)",
                active.final_path.display(),
                size,
                captured_frames
            ),
        );
        Ok(())
    }

    fn validate_request(&self, request: &RecordingRequest) -> Result<()> {
        if !matches!(request.fps, 30 | 60) {
            bail!("FPSは30または60を指定してください。");
        }
        if !matches!(request.quality.as_str(), "standard" | "high") {
            bail!("画質の指定が不正です。");
        }
        if request.source.name.trim().is_empty() || request.source.name.len() > 512 {
            bail!("録画対象名が不正です。");
        }
        Ok(())
    }
}

fn make_even(value: u32) -> u32 {
    value.max(2).saturating_add(value % 2)
}

fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    for index in 2..10_000 {
        let candidate = parent.join(format!("{stem}_{index}.mp4"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}

fn move_completed_file(source: &PathBuf, destination: &PathBuf) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, destination).with_context(|| {
                format!("録画を保存先へコピーできません: {}", destination.display())
            })?;
            fs::remove_file(source)?;
            Ok(())
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use crate::{history::HistoryStore, models::CaptureSource, portable_paths::PortablePaths};
    use std::thread;
    use windows_capture::monitor::Monitor;

    #[test]
    #[ignore = "実モニターとMedia Foundationを使う手動スモークテスト"]
    fn manual_monitor_recording_smoke() {
        let paths = PortablePaths::initialize().expect("portable paths");
        let history = Arc::new(HistoryStore::load(paths.clone()).expect("history"));
        let manager = RecordingManager::new(paths.clone(), history.clone());
        let monitor = Monitor::primary().expect("primary monitor");
        let width = monitor.width().expect("monitor width");
        let height = monitor.height().expect("monitor height");
        let request = RecordingRequest {
            source: CaptureSource {
                id: "monitor:1".into(),
                kind: "monitor".into(),
                name: "自動テスト用モニター".into(),
                detail: format!("{width} x {height}"),
            },
            quality: "standard".into(),
            fps: 30,
            system_audio: true,
            microphone: false,
            microphone_id: None,
        };
        manager
            .start(
                request,
                ResolvedSource::Monitor {
                    monitor,
                    width,
                    height,
                },
                paths.recordings.clone(),
            )
            .expect("start recording");
        thread::sleep(Duration::from_secs(2));
        manager.pause().expect("pause recording");
        thread::sleep(Duration::from_secs(1));
        manager.resume().expect("resume recording");
        thread::sleep(Duration::from_secs(2));
        manager.stop().expect("stop recording");
        let newest = history.list().into_iter().next().expect("history entry");
        assert!(PathBuf::from(&newest.path).is_file());
        assert!(newest.size_bytes > 0);
        assert!((3..=6).contains(&newest.duration_seconds));
    }
}
