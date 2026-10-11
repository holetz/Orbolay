//! Microphone capture through `cpal`.

use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use super::{AudioFrame, AudioSource, CaptureError, Epoch, FRAME_QUEUE, FrameSender};

/// Captures the local microphone: the default input device, or the one named by `rttMicDevice`
/// (matched against the device id or its display name).
pub struct MicSource {
  device: Option<String>,
  stop_tx: Option<flume::Sender<()>>,
  worker: Option<JoinHandle<()>>,
}

impl MicSource {
  pub fn new(device: Option<String>) -> Self {
    Self {
      device,
      stop_tx: None,
      worker: None,
    }
  }
}

fn backend(err: impl std::fmt::Display) -> CaptureError {
  CaptureError::Backend(err.to_string())
}

fn find_device(wanted: Option<&str>) -> Result<cpal::Device, CaptureError> {
  let host = cpal::default_host();
  let Some(wanted) = wanted else {
    return host
      .default_input_device()
      .ok_or_else(|| CaptureError::DeviceNotFound("default input".into()));
  };
  host
    .input_devices()
    .map_err(backend)?
    .find(|device| {
      device.id().is_ok_and(|id| id.to_string() == wanted)
        || device.description().is_ok_and(|desc| desc.name() == wanted)
    })
    .ok_or_else(|| CaptureError::DeviceNotFound(wanted.to_owned()))
}

fn build_stream<T>(
  device: &cpal::Device,
  config: &cpal::StreamConfig,
  epoch: Epoch,
  tx: FrameSender,
) -> Result<cpal::Stream, CaptureError>
where
  T: SizedSample,
  f32: FromSample<T>,
{
  let sample_rate = config.sample_rate;
  let channels = config.channels;
  device
    .build_input_stream(
      config,
      move |data: &[T], _: &cpal::InputCallbackInfo| {
        let frames = data.len() / usize::from(channels.max(1));
        let span = Duration::from_secs_f64(frames as f64 / f64::from(sample_rate));
        let first_sample = Instant::now()
          .checked_sub(span)
          .unwrap_or_else(Instant::now);
        tx.push(AudioFrame {
          timestamp: epoch.offset(first_sample),
          sample_rate,
          channels,
          samples: data.iter().map(|&s| s.to_sample::<f32>()).collect(),
        });
      },
      |err| eprintln!("[rtt] microphone stream error: {err}"),
      None,
    )
    .map_err(backend)
}

fn open(wanted: Option<&str>, epoch: Epoch, tx: FrameSender) -> Result<cpal::Stream, CaptureError> {
  let device = find_device(wanted)?;
  let supported = device.default_input_config().map_err(backend)?;
  let config = supported.config();
  let stream = match supported.sample_format() {
    SampleFormat::F32 => build_stream::<f32>(&device, &config, epoch, tx),
    SampleFormat::I16 => build_stream::<i16>(&device, &config, epoch, tx),
    SampleFormat::I32 => build_stream::<i32>(&device, &config, epoch, tx),
    SampleFormat::U16 => build_stream::<u16>(&device, &config, epoch, tx),
    other => Err(CaptureError::Backend(format!(
      "unsupported sample format {other}"
    ))),
  }?;
  stream.play().map_err(backend)?;
  Ok(stream)
}

impl AudioSource for MicSource {
  fn start(&mut self, epoch: Epoch) -> Result<flume::Receiver<AudioFrame>, CaptureError> {
    self.stop();
    let wanted = self.device.clone();
    let (tx, rx) = FrameSender::bounded(FRAME_QUEUE);
    let (ready_tx, ready_rx) = flume::bounded(1);
    let (stop_tx, stop_rx) = flume::bounded::<()>(1);

    // `cpal::Stream` is not `Send` on every platform, so a dedicated thread owns it until stop.
    self.worker = Some(std::thread::spawn(move || {
      match open(wanted.as_deref(), epoch, tx) {
        Ok(stream) => {
          ready_tx.send(Ok(())).ok();
          stop_rx.recv().ok();
          drop(stream);
        }
        Err(err) => {
          ready_tx.send(Err(err)).ok();
        }
      }
    }));
    self.stop_tx = Some(stop_tx);

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
    if let Some(stop_tx) = self.stop_tx.take() {
      stop_tx.send(()).ok();
    }
    if let Some(worker) = self.worker.take() {
      worker.join().ok();
    }
  }
}

impl Drop for MicSource {
  fn drop(&mut self) {
    self.stop();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn unknown_device_is_reported_by_name() {
    match find_device(Some("orbolay-no-such-device")) {
      Err(CaptureError::DeviceNotFound(name)) => assert_eq!(name, "orbolay-no-such-device"),
      Err(CaptureError::Backend(_)) => {}
      Ok(_) => panic!("matched a device that does not exist"),
    }
  }
}
