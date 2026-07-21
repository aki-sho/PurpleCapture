use anyhow::{Context as _, Result};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    encoder::VideoEncoder,
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
};

#[derive(Clone)]
pub struct CaptureFlags {
    pub encoder: Arc<Mutex<Option<VideoEncoder>>>,
    pub paused: Arc<AtomicBool>,
    pub target_closed: Arc<AtomicBool>,
    pub captured_frames: Arc<AtomicU64>,
    pub target_width: u32,
    pub target_height: u32,
    pub frame_ticks: i64,
    pub frame_period: Duration,
}

pub struct CaptureHandler {
    encoder: Arc<Mutex<Option<VideoEncoder>>>,
    paused: Arc<AtomicBool>,
    target_closed: Arc<AtomicBool>,
    captured_frames: Arc<AtomicU64>,
    target_width: u32,
    target_height: u32,
    frame_ticks: i64,
    frame_period: Duration,
    last_frame: Option<Instant>,
    next_timestamp: i64,
    staging: Vec<u8>,
    flipped: Vec<u8>,
}

impl CaptureHandler {
    pub fn finish_encoder(&mut self) -> Result<()> {
        if let Some(encoder) = self.encoder.lock().take() {
            encoder.finish().context("MP4ファイルを確定できません。")?;
        }
        Ok(())
    }
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = CaptureFlags;
    type Error = anyhow::Error;

    fn new(context: Context<Self::Flags>) -> Result<Self> {
        Ok(Self {
            encoder: context.flags.encoder,
            paused: context.flags.paused,
            target_closed: context.flags.target_closed,
            captured_frames: context.flags.captured_frames,
            target_width: context.flags.target_width,
            target_height: context.flags.target_height,
            frame_ticks: context.flags.frame_ticks,
            frame_period: context.flags.frame_period,
            last_frame: None,
            next_timestamp: 0,
            staging: Vec::new(),
            flipped: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<()> {
        if self.paused.load(Ordering::Acquire) {
            return Ok(());
        }
        let now = Instant::now();
        if self
            .last_frame
            .is_some_and(|last| now.duration_since(last) < self.frame_period)
        {
            return Ok(());
        }
        self.last_frame = Some(now);
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
        let mut encoder = self.encoder.lock();
        let encoder = encoder
            .as_mut()
            .context("映像エンコーダーが終了しています。")?;
        encoder
            .send_frame_buffer(&self.flipped, self.next_timestamp)
            .context("映像フレームをエンコードできません。")?;
        self.next_timestamp = self.next_timestamp.saturating_add(self.frame_ticks);
        Ok(())
    }

    fn on_closed(&mut self) -> Result<()> {
        self.target_closed.store(true, Ordering::Release);
        Ok(())
    }
}
