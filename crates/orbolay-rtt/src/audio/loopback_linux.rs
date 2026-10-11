//! Linux loopback: records the PulseAudio monitor of a sink through the ALSA `pulse` plugin.
//!
//! PipeWire's `pipewire-pulse` serves the same protocol, so one path covers both servers. The
//! `alsa` crate is already linked by `cpal`, which keeps the build free of `libpulse`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};

use super::{AudioFrame, AudioSource, CaptureError, Epoch, FRAME_QUEUE, FrameSender};

/// PulseAudio alias for the monitor of the current default sink.
const DEFAULT_MONITOR: &str = "@DEFAULT_MONITOR@";
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// 10 ms per read at 48 kHz.
const PERIOD_FRAMES: usize = 480;

/// Captures what the system plays: the default sink's monitor, or a named PulseAudio source
/// (`rttLoopbackDevice`, e.g. `<sink>.monitor`).
pub struct LoopbackSource {
  source: Option<String>,
  running: Arc<AtomicBool>,
  worker: Option<JoinHandle<()>>,
}

impl LoopbackSource {
  pub fn new(source: Option<String>) -> Self {
    Self {
      source,
      running: Arc::new(AtomicBool::new(false)),
      worker: None,
    }
  }

  fn pcm_name(&self) -> String {
    let source = self.source.as_deref().unwrap_or(DEFAULT_MONITOR);
    format!("pulse:DEVICE=\"{source}\"")
  }
}

fn backend(err: alsa::Error) -> CaptureError {
  CaptureError::Backend(err.to_string())
}

fn open(name: &str) -> Result<(PCM, u32), CaptureError> {
  let pcm = PCM::new(name, Direction::Capture, false)
    .map_err(|_| CaptureError::DeviceNotFound(name.to_owned()))?;
  {
    let hw = HwParams::any(&pcm).map_err(backend)?;
    hw.set_access(Access::RWInterleaved).map_err(backend)?;
    hw.set_format(Format::float()).map_err(backend)?;
    hw.set_channels(u32::from(CHANNELS)).map_err(backend)?;
    hw.set_rate_near(RATE, ValueOr::Nearest).map_err(backend)?;
    hw.set_period_size_near(PERIOD_FRAMES as alsa::pcm::Frames, ValueOr::Nearest)
      .map_err(backend)?;
    pcm.hw_params(&hw).map_err(backend)?;
  }
  let rate = pcm
    .hw_params_current()
    .and_then(|hw| hw.get_rate())
    .map_err(backend)?;
  Ok((pcm, rate))
}

fn capture(pcm: PCM, rate: u32, epoch: Epoch, tx: FrameSender, running: Arc<AtomicBool>) {
  let Ok(io) = pcm.io_f32() else { return };
  let mut buf = vec![0f32; PERIOD_FRAMES * usize::from(CHANNELS)];
  while running.load(Ordering::Relaxed) {
    match io.readi(&mut buf) {
      Ok(frames) => {
        let span = Duration::from_secs_f64(frames as f64 / f64::from(rate));
        let first_sample = Instant::now()
          .checked_sub(span)
          .unwrap_or_else(Instant::now);
        tx.push(AudioFrame {
          timestamp: epoch.offset(first_sample),
          sample_rate: rate,
          channels: CHANNELS,
          samples: buf[..frames * usize::from(CHANNELS)].to_vec(),
        });
      }
      Err(err) => {
        if pcm.try_recover(err, true).is_err() {
          break;
        }
      }
    }
  }
  pcm.drop().ok();
}

impl AudioSource for LoopbackSource {
  fn start(&mut self, epoch: Epoch) -> Result<flume::Receiver<AudioFrame>, CaptureError> {
    self.stop();
    let name = self.pcm_name();
    let (tx, rx) = FrameSender::bounded(FRAME_QUEUE);
    let (ready_tx, ready_rx) = flume::bounded(1);
    let running = self.running.clone();
    running.store(true, Ordering::Relaxed);

    // The PCM is opened on the capture thread, which owns it for its whole life.
    self.worker = Some(std::thread::spawn(move || match open(&name) {
      Ok((pcm, rate)) => {
        ready_tx.send(Ok(())).ok();
        capture(pcm, rate, epoch, tx, running);
      }
      Err(err) => {
        ready_tx.send(Err(err)).ok();
      }
    }));

    let opened = ready_rx
      .recv()
      .unwrap_or_else(|_| Err(CaptureError::Backend("capture thread exited".into())));
    if let Err(err) = opened {
      self.stop();
      return Err(err);
    }
    Ok(rx)
  }

  fn stop(&mut self) {
    self.running.store(false, Ordering::Relaxed);
    if let Some(worker) = self.worker.take() {
      worker.join().ok();
    }
  }
}

impl Drop for LoopbackSource {
  fn drop(&mut self) {
    self.stop();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_to_the_default_sink_monitor() {
    assert_eq!(
      LoopbackSource::new(None).pcm_name(),
      "pulse:DEVICE=\"@DEFAULT_MONITOR@\""
    );
  }

  #[test]
  fn configured_source_is_quoted() {
    let source = LoopbackSource::new(Some("alsa_output.usb-x.analog-stereo.monitor".into()));
    assert_eq!(
      source.pcm_name(),
      "pulse:DEVICE=\"alsa_output.usb-x.analog-stereo.monitor\""
    );
  }
}
