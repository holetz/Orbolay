//! Conversion of any source format to the single format the STT stage consumes: 16 kHz mono.

use std::time::Duration;

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

use super::{AudioFrame, CaptureError};

pub const TARGET_RATE: u32 = 16_000;
/// Input frames per FFT chunk.
const CHUNK: usize = 1024;
/// Input timestamp jump, beyond the previous frame's end, that marks a discontinuity.
const GAP_TOLERANCE: Duration = Duration::from_millis(50);

/// Streaming downmix + resample to [`TARGET_RATE`] mono, run on the consumer side of a source.
///
/// Input is buffered into fixed chunks, so output lags the input by up to one chunk; output
/// timestamps are derived from the first input frame plus the samples emitted since. A forward
/// jump in input timestamps (dropped frames) discards the buffered tail and re-anchors the origin.
#[derive(Default)]
pub struct MonoResampler {
  in_rate: u32,
  fft: Option<Fft<f32>>,
  pending: Vec<f32>,
  scratch: Vec<f32>,
  delay_left: usize,
  origin: Duration,
  /// Where the previous input frame ended; a later start means frames were dropped.
  expected_start: Option<Duration>,
  consumed: u64,
  emitted: u64,
}

impl MonoResampler {
  fn reset(&mut self, frame: &AudioFrame) -> Result<(), CaptureError> {
    self.fft = if frame.sample_rate == TARGET_RATE {
      None
    } else {
      let fft = Fft::new(
        frame.sample_rate as usize,
        TARGET_RATE as usize,
        CHUNK,
        1,
        1,
        FixedSync::Input,
      )
      .map_err(|err| CaptureError::Backend(err.to_string()))?;
      self.scratch = vec![0.; fft.output_frames_max()];
      self.delay_left = fft.output_delay();
      Some(fft)
    };
    self.in_rate = frame.sample_rate;
    self.pending.clear();
    self.origin = frame.timestamp;
    self.consumed = 0;
    self.emitted = 0;
    Ok(())
  }

  /// Feeds one source frame and returns the 16 kHz mono samples that became available.
  pub fn process(&mut self, frame: &AudioFrame) -> Result<Option<AudioFrame>, CaptureError> {
    let gap = self
      .expected_start
      .is_some_and(|end| frame.timestamp > end + GAP_TOLERANCE);
    if frame.sample_rate != self.in_rate || gap {
      self.reset(frame)?;
    }
    self.expected_start = Some(frame.timestamp + frame.duration());
    let channels = usize::from(frame.channels.max(1));
    let mono = frame
      .samples
      .chunks_exact(channels)
      .map(|c| c.iter().sum::<f32>() / channels as f32);
    self.consumed += (frame.samples.len() / channels) as u64;

    if self.fft.is_none() {
      return Ok(self.emit(mono.collect()));
    }
    self.pending.extend(mono);
    let mut out = Vec::new();
    while self.pending.len() >= CHUNK {
      self.run_chunk(CHUNK, &mut out)?;
    }
    Ok(self.emit(out))
  }

  /// Pads the buffered tail with silence and returns everything still owed, so that the total
  /// output length matches the total input duration.
  pub fn flush(&mut self) -> Result<Option<AudioFrame>, CaptureError> {
    let owed = (self.consumed * u64::from(TARGET_RATE)).div_ceil(u64::from(self.in_rate.max(1)));
    let mut out = Vec::new();
    while self.fft.is_some() && self.emitted + (out.len() as u64) < owed {
      let len = self.pending.len().min(CHUNK);
      self.run_chunk(len, &mut out)?;
    }
    out.truncate(owed.saturating_sub(self.emitted) as usize);
    Ok(self.emit(out))
  }

  fn run_chunk(&mut self, len: usize, out: &mut Vec<f32>) -> Result<(), CaptureError> {
    let Some(fft) = self.fft.as_mut() else {
      return Ok(());
    };
    let backend = |err: &dyn std::fmt::Display| CaptureError::Backend(err.to_string());
    let input = InterleavedSlice::new(&self.pending[..len], 1, len).map_err(|e| backend(&e))?;
    let frames = self.scratch.len();
    let mut output =
      InterleavedSlice::new_mut(&mut self.scratch, 1, frames).map_err(|e| backend(&e))?;
    let indexing = Indexing {
      input_offset: 0,
      output_offset: 0,
      partial_len: (len < CHUNK).then_some(len),
      active_channels_mask: None,
    };
    let (_, written) = fft
      .process_into_buffer(&input, &mut output, Some(&indexing))
      .map_err(|e| backend(&e))?;
    self.pending.drain(..len);
    self
      .pending
      .truncate(self.pending.len().saturating_sub(CHUNK - len));
    let skip = self.delay_left.min(written);
    self.delay_left -= skip;
    out.extend_from_slice(&self.scratch[skip..written]);
    Ok(())
  }

  fn emit(&mut self, samples: Vec<f32>) -> Option<AudioFrame> {
    if samples.is_empty() {
      return None;
    }
    let offset = Duration::from_secs_f64(self.emitted as f64 / f64::from(TARGET_RATE));
    self.emitted += samples.len() as u64;
    Some(AudioFrame {
      timestamp: self.origin + offset,
      sample_rate: TARGET_RATE,
      channels: 1,
      samples,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::f32::consts::TAU;

  fn stereo_sine(rate: u32, freq: f32, seconds: f32) -> Vec<f32> {
    let frames = (rate as f32 * seconds) as usize;
    (0..frames)
      .flat_map(|i| {
        let s = 0.5 * (TAU * freq * i as f32 / rate as f32).sin();
        [s, s]
      })
      .collect()
  }

  #[test]
  fn maps_48k_stereo_sine_to_16k_mono_with_same_frequency_and_duration() {
    let input = stereo_sine(48_000, 1_000., 2.);
    let mut resampler = MonoResampler::default();
    let mut output = Vec::new();
    // Feed 10 ms blocks the way a capture source does.
    for (i, block) in input.chunks(960).enumerate() {
      let frame = AudioFrame {
        timestamp: Duration::from_millis(i as u64 * 10),
        sample_rate: 48_000,
        channels: 2,
        samples: block.to_vec(),
      };
      if let Some(out) = resampler.process(&frame).unwrap() {
        assert_eq!((out.sample_rate, out.channels), (TARGET_RATE, 1));
        output.extend(out.samples);
      }
    }
    output.extend(
      resampler
        .flush()
        .unwrap()
        .map(|f| f.samples)
        .unwrap_or_default(),
    );

    assert_eq!(output.len(), 32_000);

    // Skip the filter edges, then count rising zero crossings over one second.
    let window = &output[8_000..24_000];
    let crossings = window
      .windows(2)
      .filter(|w| w[0] < 0. && w[1] >= 0.)
      .count();
    assert!(
      (998..=1_002).contains(&crossings),
      "measured {crossings} Hz"
    );
    let peak = window.iter().fold(0f32, |m, s| m.max(s.abs()));
    assert!((0.45..=0.55).contains(&peak), "peak {peak}");
  }

  #[test]
  fn output_timestamps_follow_the_first_input_frame() {
    let mut resampler = MonoResampler::default();
    let frame = AudioFrame {
      timestamp: Duration::from_millis(250),
      sample_rate: TARGET_RATE,
      channels: 2,
      samples: vec![0.25, 0.75, 1., 0.],
    };
    let first = resampler.process(&frame).unwrap().unwrap();
    assert_eq!(first.samples, [0.5, 0.5]);
    assert_eq!(first.timestamp, Duration::from_millis(250));
    let second = resampler.process(&frame).unwrap().unwrap();
    assert_eq!(
      second.timestamp,
      Duration::from_millis(250) + Duration::from_micros(125)
    );
  }

  #[test]
  fn output_timestamps_re_anchor_after_an_input_gap() {
    for rate in [TARGET_RATE, 48_000] {
      let mut resampler = MonoResampler::default();
      let block = |ms: u64| AudioFrame {
        timestamp: Duration::from_millis(ms),
        sample_rate: rate,
        channels: 1,
        samples: vec![0.; (rate as usize) / 100 * 2],
      };
      // 20 ms blocks at 0 ms, then a 5 s jump as if frames were dropped.
      let mut stamps = Vec::new();
      for ms in [0, 20, 40, 5_000, 5_020, 5_040, 5_060] {
        if let Some(out) = resampler.process(&block(ms)).unwrap() {
          stamps.push(out.timestamp);
        }
      }
      let after_gap = stamps.last().copied().unwrap();
      assert!(
        after_gap >= Duration::from_millis(5_000),
        "rate {rate}: output stamped {after_gap:?}, expected >= 5s"
      );
    }
  }
}
