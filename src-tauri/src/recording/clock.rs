use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub struct RecordingClock {
    state: Mutex<ClockState>,
}

struct ClockState {
    started: Instant,
    paused_at: Option<Instant>,
    paused_total: Duration,
    stopped_at: Option<Instant>,
}

impl RecordingClock {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ClockState {
                started: Instant::now(),
                paused_at: None,
                paused_total: Duration::ZERO,
                stopped_at: None,
            }),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.state.lock().elapsed_at(Instant::now())
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().paused_at.is_some()
    }

    pub fn pause(&self) {
        self.state.lock().pause_at(Instant::now());
    }

    pub fn resume(&self) {
        self.state.lock().resume_at(Instant::now());
    }

    pub fn stop(&self) {
        self.state.lock().stopped_at.get_or_insert(Instant::now());
    }
}

impl ClockState {
    fn elapsed_at(&self, now: Instant) -> Duration {
        self.paused_at
            .or(self.stopped_at)
            .unwrap_or(now)
            .saturating_duration_since(self.started)
            .saturating_sub(self.paused_total)
    }
    fn pause_at(&mut self, now: Instant) {
        if self.stopped_at.is_none() {
            self.paused_at.get_or_insert(now);
        }
    }
    fn resume_at(&mut self, now: Instant) {
        if self.stopped_at.is_none() {
            if let Some(paused) = self.paused_at.take() {
                self.paused_total += now.saturating_duration_since(paused);
            }
        }
    }
}

pub fn ticks(time: Duration) -> i64 {
    (time.as_nanos() / 100).min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_frames_do_not_shorten_time_and_pause_is_excluded() {
        let start = Instant::now();
        let mut clock = ClockState {
            started: start,
            paused_at: None,
            paused_total: Duration::ZERO,
            stopped_at: None,
        };
        assert_eq!(
            ticks(clock.elapsed_at(start + Duration::from_secs(10))),
            100_000_000
        );
        clock.pause_at(start + Duration::from_secs(10));
        clock.pause_at(start + Duration::from_secs(11));
        assert_eq!(
            clock.elapsed_at(start + Duration::from_secs(20)),
            Duration::from_secs(10)
        );
        clock.resume_at(start + Duration::from_secs(20));
        clock.resume_at(start + Duration::from_secs(21));
        clock.stopped_at = Some(start + Duration::from_secs(25));
        assert_eq!(
            clock.elapsed_at(start + Duration::from_secs(100)),
            Duration::from_secs(15)
        );
    }
}
