//! Audio sources and the shared pipeline clock.

#[cfg(target_os = "linux")]
mod loopback_linux;
mod mic;

#[cfg(target_os = "linux")]
pub use loopback_linux::LoopbackSource;
pub use mic::MicSource;

use std::fmt;
use std::time::{Duration, Instant};

/// Monotonic origin shared by every audio source and the speaking timeline, so that audio frames
/// and speaking events can be lined up by plain interval overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Epoch(Instant);

impl Epoch {
  pub fn now() -> Self {
    Self(Instant::now())
  }

  /// Offset of `instant` from the epoch; instants before it saturate to zero.
  pub fn offset(&self, instant: Instant) -> Duration {
    instant.saturating_duration_since(self.0)
  }

  pub fn elapsed(&self) -> Duration {
    self.offset(Instant::now())
  }
}

/// A block of interleaved `f32` samples, stamped with the epoch offset of its first sample.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
  pub timestamp: Duration,
  pub sample_rate: u32,
  pub channels: u16,
  pub samples: Vec<f32>,
}

impl AudioFrame {
  pub fn duration(&self) -> Duration {
    let per_channel = self.samples.len() / usize::from(self.channels.max(1));
    Duration::from_secs_f64(per_channel as f64 / f64::from(self.sample_rate.max(1)))
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
  DeviceNotFound(String),
  Backend(String),
}

impl fmt::Display for CaptureError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::DeviceNotFound(name) => write!(f, "audio device not found: {name}"),
      Self::Backend(msg) => write!(f, "audio backend error: {msg}"),
    }
  }
}

impl std::error::Error for CaptureError {}

/// A capture stream. `start` returns the receiving end of a bounded channel; the capture callback
/// never blocks, so when the consumer falls behind the oldest frames are dropped.
pub trait AudioSource {
  fn start(&mut self, epoch: Epoch) -> Result<flume::Receiver<AudioFrame>, CaptureError>;
  fn stop(&mut self);
}

/// Frames buffered per source before the oldest are dropped.
pub const FRAME_QUEUE: usize = 256;

/// Non-blocking producer side of a source's bounded channel, safe to call from an audio callback.
#[derive(Clone)]
pub struct FrameSender {
  tx: flume::Sender<AudioFrame>,
  rx: flume::Receiver<AudioFrame>,
}

impl FrameSender {
  pub fn bounded(capacity: usize) -> (Self, flume::Receiver<AudioFrame>) {
    let (tx, rx) = flume::bounded(capacity);
    (Self { tx, rx: rx.clone() }, rx)
  }

  /// Sends `frame`, evicting the oldest queued frame when the channel is full.
  pub fn push(&self, mut frame: AudioFrame) {
    loop {
      match self.tx.try_send(frame) {
        Ok(()) | Err(flume::TrySendError::Disconnected(_)) => return,
        Err(flume::TrySendError::Full(rejected)) => {
          let _ = self.rx.try_recv();
          frame = rejected;
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn frame(ms: u64) -> AudioFrame {
    AudioFrame {
      timestamp: Duration::from_millis(ms),
      sample_rate: 48_000,
      channels: 2,
      samples: vec![0.; 960],
    }
  }

  #[test]
  fn full_queue_drops_oldest() {
    let (tx, rx) = FrameSender::bounded(2);
    for ms in [0, 10, 20] {
      tx.push(frame(ms));
    }
    let kept: Vec<_> = rx.try_iter().map(|f| f.timestamp.as_millis()).collect();
    assert_eq!(kept, [10, 20]);
  }

  #[test]
  fn frame_duration_counts_per_channel_samples() {
    assert_eq!(frame(0).duration(), Duration::from_millis(10));
  }

  #[test]
  fn epoch_offset_saturates_before_origin() {
    let before = Instant::now();
    let epoch = Epoch::now();
    assert_eq!(epoch.offset(before), Duration::ZERO);
  }
}
