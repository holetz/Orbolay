//! Speech-to-text: the engine interface and the segments that flow through it.

pub mod models;
pub mod vad;
pub mod whisper;

pub use vad::{VadConfig, VadSegmenter};
pub use whisper::{WhisperConfig, WhisperEngine};

use std::fmt;
use std::time::Duration;

use crate::audio::TARGET_RATE;

/// A slice of one utterance, as 16 kHz mono `f32` samples.
///
/// A provisional segment (`is_final == false`) carries the utterance so far and is followed by
/// longer ones with the same `utterance` id, until the final segment closes it.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSegment {
  pub utterance: u64,
  /// Pipeline-clock offset of the first sample.
  pub start: Duration,
  pub samples: Vec<f32>,
  pub is_final: bool,
}

impl AudioSegment {
  pub fn duration(&self) -> Duration {
    Duration::from_secs_f64(self.samples.len() as f64 / f64::from(TARGET_RATE))
  }

  pub fn end(&self) -> Duration {
    self.start + self.duration()
  }
}

/// Recognised text with its span on the pipeline clock.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSegment {
  pub start: Duration,
  pub end: Duration,
  pub text: String,
  /// The engine's probability that the span holds no speech, `0.0` when it does not report one.
  pub no_speech_prob: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttError {
  ModelNotFound(String),
  Engine(String),
}

impl fmt::Display for SttError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::ModelNotFound(path) => write!(f, "speech model not found: {path}"),
      Self::Engine(msg) => write!(f, "speech engine error: {msg}"),
    }
  }
}

impl std::error::Error for SttError {}

/// A speech recogniser. Called from the engine worker only, never from an audio thread.
pub trait SttEngine: Send {
  fn transcribe(&mut self, segment: &AudioSegment) -> Result<Vec<TextSegment>, SttError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn segment_end_follows_sample_count() {
    let segment = AudioSegment {
      utterance: 1,
      start: Duration::from_secs(2),
      samples: vec![0.; TARGET_RATE as usize / 2],
      is_final: true,
    };
    assert_eq!(segment.end(), Duration::from_millis(2500));
  }
}
