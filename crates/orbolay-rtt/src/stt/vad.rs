//! Voice-activity segmentation: cuts a 16 kHz mono stream into utterances, so silence never
//! reaches the engine.
//!
//! Detection is energy-based against an adaptive noise floor. While an utterance grows, a
//! provisional segment carrying everything so far is emitted every `partial_every`; the final
//! segment, with the same utterance id, follows once `hangover` of silence closes it or it reaches
//! `max_utterance`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::AudioSegment;
use crate::audio::{AudioFrame, TARGET_RATE};

#[derive(Debug, Clone, PartialEq)]
pub struct VadConfig {
  /// Analysis window.
  pub frame: Duration,
  /// RMS below this is never speech, however quiet the room.
  pub min_rms: f32,
  /// A window is speech when its RMS exceeds the noise floor by this factor.
  pub floor_ratio: f32,
  /// Silence kept before the first voiced window, so word onsets are not clipped.
  pub pre_roll: Duration,
  /// Silence that closes an utterance.
  pub hangover: Duration,
  /// Utterances with less voiced audio than this are dropped as clicks.
  pub min_speech: Duration,
  pub max_utterance: Duration,
  pub partial_every: Duration,
}

impl Default for VadConfig {
  fn default() -> Self {
    Self {
      frame: Duration::from_millis(30),
      min_rms: 0.01,
      floor_ratio: 3.,
      pre_roll: Duration::from_millis(200),
      hangover: Duration::from_millis(600),
      min_speech: Duration::from_millis(250),
      max_utterance: Duration::from_secs(8),
      partial_every: Duration::from_millis(1500),
    }
  }
}

fn samples_in(d: Duration) -> usize {
  (d.as_secs_f64() * f64::from(TARGET_RATE)).round() as usize
}

fn duration_of(samples: usize) -> Duration {
  Duration::from_secs_f64(samples as f64 / f64::from(TARGET_RATE))
}

fn rms(window: &[f32]) -> f32 {
  (window.iter().map(|s| s * s).sum::<f32>() / window.len().max(1) as f32).sqrt()
}

/// Utterance ids are unique for the life of the process: a rebuilt pipeline must never reuse an id
/// whose caption line is still in the transcript.
static NEXT_UTTERANCE: AtomicU64 = AtomicU64::new(1);

struct Utterance {
  id: u64,
  start: Duration,
  samples: Vec<f32>,
  voiced: usize,
  silence_run: usize,
  next_partial: usize,
}

pub struct VadSegmenter {
  config: VadConfig,
  window: usize,
  noise_floor: f32,
  /// Samples not yet forming a whole window, and the clock offset of the first of them.
  pending: Vec<f32>,
  pending_start: Duration,
  pre_roll: VecDeque<f32>,
  current: Option<Utterance>,
}

impl VadSegmenter {
  pub fn new(config: VadConfig) -> Self {
    Self {
      window: samples_in(config.frame).max(1),
      noise_floor: config.min_rms,
      pending: Vec::new(),
      pending_start: Duration::ZERO,
      pre_roll: VecDeque::new(),
      current: None,
      config,
    }
  }

  /// Feeds a 16 kHz mono frame; returns the provisional and final segments it completed.
  pub fn push(&mut self, frame: &AudioFrame) -> Vec<AudioSegment> {
    debug_assert_eq!((frame.sample_rate, frame.channels), (TARGET_RATE, 1));
    if self.pending.is_empty() {
      self.pending_start = frame.timestamp;
    }
    self.pending.extend_from_slice(&frame.samples);

    let mut out = Vec::new();
    let whole = self.pending.len() / self.window * self.window;
    let ready: Vec<f32> = self.pending.drain(..whole).collect();
    for window in ready.chunks_exact(self.window) {
      let start = self.pending_start;
      self.pending_start += duration_of(self.window);
      out.extend(self.window_in(window, start));
    }
    out
  }

  /// Closes the open utterance, if any, as a final segment.
  pub fn flush(&mut self) -> Option<AudioSegment> {
    self.pending.clear();
    self.close()
  }

  fn window_in(&mut self, window: &[f32], start: Duration) -> Option<AudioSegment> {
    let level = rms(window);
    let voiced = level
      > self
        .config
        .min_rms
        .max(self.noise_floor * self.config.floor_ratio);

    let Some(utt) = self.current.as_mut() else {
      if voiced {
        let mut samples: Vec<f32> = self.pre_roll.drain(..).collect();
        let utt_start = start.saturating_sub(duration_of(samples.len()));
        samples.extend_from_slice(window);
        self.current = Some(Utterance {
          id: NEXT_UTTERANCE.fetch_add(1, Ordering::Relaxed),
          start: utt_start,
          samples,
          voiced: window.len(),
          silence_run: 0,
          next_partial: samples_in(self.config.partial_every),
        });
      } else {
        self.noise_floor = 0.95 * self.noise_floor + 0.05 * level;
        self.pre_roll.extend(window);
        let keep = samples_in(self.config.pre_roll);
        while self.pre_roll.len() > keep {
          self.pre_roll.pop_front();
        }
      }
      return None;
    };

    utt.samples.extend_from_slice(window);
    if voiced {
      utt.voiced += window.len();
      utt.silence_run = 0;
    } else {
      utt.silence_run += window.len();
    }

    if utt.silence_run >= samples_in(self.config.hangover)
      || utt.samples.len() + self.window > samples_in(self.config.max_utterance)
    {
      return self.close();
    }

    if utt.samples.len() >= utt.next_partial {
      utt.next_partial += samples_in(self.config.partial_every);
      return Some(AudioSegment {
        utterance: utt.id,
        start: utt.start,
        samples: utt.samples.clone(),
        is_final: false,
      });
    }
    None
  }

  fn close(&mut self) -> Option<AudioSegment> {
    let utt = self.current.take()?;
    (utt.voiced >= samples_in(self.config.min_speech)).then_some(AudioSegment {
      utterance: utt.id,
      start: utt.start,
      samples: utt.samples,
      is_final: true,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const RATE: usize = TARGET_RATE as usize;

  /// Feeds `secs` of audio in 20 ms frames from `t0`, `tone` loud or near-silent.
  fn feed(vad: &mut VadSegmenter, t0: f64, secs: f64, tone: bool) -> Vec<AudioSegment> {
    let frame_len = RATE / 50;
    let total = (secs * RATE as f64) as usize;
    let mut out = Vec::new();
    for f in 0..total / frame_len {
      let first = f * frame_len;
      let samples = (first..first + frame_len)
        .map(|i| {
          let t = i as f32 / RATE as f32;
          if tone {
            0.3 * (t * 440. * std::f32::consts::TAU).sin()
          } else {
            0.001 * (t * 50. * std::f32::consts::TAU).sin()
          }
        })
        .collect();
      out.extend(vad.push(&AudioFrame {
        timestamp: Duration::from_secs_f64(t0 + first as f64 / RATE as f64),
        sample_rate: TARGET_RATE,
        channels: 1,
        samples,
      }));
    }
    out
  }

  #[test]
  fn utterance_ids_are_unique_across_segmenters() {
    let first_id = |vad: &mut VadSegmenter| {
      let mut out = feed(vad, 0., 3.5, true);
      out.extend(feed(vad, 3.5, 1., false));
      out
        .iter()
        .find(|s| s.is_final)
        .expect("a final segment")
        .utterance
    };
    let a = first_id(&mut VadSegmenter::new(VadConfig::default()));
    let b = first_id(&mut VadSegmenter::new(VadConfig::default()));
    assert_ne!(a, b, "a restarted pipeline must not reuse utterance ids");
  }

  #[test]
  fn sixty_seconds_of_silence_emit_nothing() {
    let mut vad = VadSegmenter::new(VadConfig::default());
    assert!(feed(&mut vad, 0., 60., false).is_empty());
    assert!(vad.flush().is_none());
  }

  #[test]
  fn utterance_emits_partials_then_one_final() {
    let mut vad = VadSegmenter::new(VadConfig::default());
    let mut out = feed(&mut vad, 0., 1., false);
    out.extend(feed(&mut vad, 1., 3.5, true));
    out.extend(feed(&mut vad, 4.5, 1., false));

    let (partials, finals): (Vec<_>, Vec<_>) = out.iter().partition(|s| !s.is_final);
    assert_eq!(partials.len(), 2, "partials at ~1.5 s and ~3 s");
    assert_eq!(finals.len(), 1);
    assert!(out.iter().all(|s| s.utterance == finals[0].utterance));
    assert!(partials[0].samples.len() < partials[1].samples.len());

    let fin = finals[0];
    assert!(fin.start >= Duration::from_millis(790) && fin.start <= Duration::from_secs(1));
    assert!(fin.end() >= Duration::from_millis(4500) && fin.end() <= Duration::from_millis(5200));
  }

  #[test]
  fn long_speech_is_cut_at_max_utterance() {
    let mut vad = VadSegmenter::new(VadConfig::default());
    let finals: Vec<_> = feed(&mut vad, 0., 20., true)
      .into_iter()
      .filter(|s| s.is_final)
      .collect();
    assert_eq!(finals.len(), 2);
    assert!(
      finals
        .iter()
        .all(|s| s.duration() <= Duration::from_secs(8))
    );
    assert_ne!(finals[0].utterance, finals[1].utterance);
    assert!(vad.flush().is_some_and(|s| s.is_final));
  }

  #[test]
  fn click_shorter_than_min_speech_is_dropped() {
    let mut vad = VadSegmenter::new(VadConfig::default());
    let mut out = feed(&mut vad, 0., 0.1, true);
    out.extend(feed(&mut vad, 0.1, 2., false));
    assert!(out.is_empty());
  }
}
