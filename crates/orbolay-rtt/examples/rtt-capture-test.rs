//! Records the microphone and the loopback side by side, resampled to 16 kHz mono, into
//! `rtt-mic.wav` and `rtt-loopback.wav` for inspection.
//!
//! `cargo run -p orbolay-rtt --example rtt-capture-test -- [seconds] [output dir]`
//!
//! Devices come from `rttMicDevice` / `rttLoopbackDevice` in the Orbolay config, when set. Both
//! files start at the same pipeline epoch, so the leading silence of each lines them up.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use orbolay_rtt::audio::{
  AudioSource, Epoch, LoopbackSource, MicSource, MonoResampler, TARGET_RATE,
};

fn record(mut source: impl AudioSource, epoch: Epoch, length: Duration) -> Vec<f32> {
  let rx = match source.start(epoch) {
    Ok(rx) => rx,
    Err(err) => {
      eprintln!("capture failed: {err}");
      return Vec::new();
    }
  };
  let mut resampler = MonoResampler::default();
  let mut samples = Vec::new();
  while epoch.elapsed() < length {
    let Ok(frame) = rx.recv_timeout(Duration::from_millis(100)) else {
      continue;
    };
    if samples.is_empty() {
      // Pad with silence up to the first frame so both files share the epoch as time zero.
      samples.resize(
        (frame.timestamp.as_secs_f64() * f64::from(TARGET_RATE)) as usize,
        0.,
      );
    }
    if let Ok(Some(out)) = resampler.process(&frame) {
      samples.extend(out.samples);
    }
  }
  source.stop();
  if let Ok(Some(out)) = resampler.flush() {
    samples.extend(out.samples);
  }
  samples
}

fn write_wav(path: &Path, samples: &[f32]) -> io::Result<()> {
  let data_len = u32::try_from(samples.len() * 2).map_err(io::Error::other)?;
  let mut out = BufWriter::new(File::create(path)?);
  out.write_all(b"RIFF")?;
  out.write_all(&(36 + data_len).to_le_bytes())?;
  out.write_all(b"WAVEfmt ")?;
  out.write_all(&16u32.to_le_bytes())?;
  out.write_all(&1u16.to_le_bytes())?; // PCM
  out.write_all(&1u16.to_le_bytes())?; // mono
  out.write_all(&TARGET_RATE.to_le_bytes())?;
  out.write_all(&(TARGET_RATE * 2).to_le_bytes())?;
  out.write_all(&2u16.to_le_bytes())?;
  out.write_all(&16u16.to_le_bytes())?;
  out.write_all(b"data")?;
  out.write_all(&data_len.to_le_bytes())?;
  for sample in samples {
    out.write_all(&((sample.clamp(-1., 1.) * f32::from(i16::MAX)) as i16).to_le_bytes())?;
  }
  out.flush()
}

fn main() -> io::Result<()> {
  let mut args = std::env::args().skip(1);
  let seconds = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
  let dir = args
    .next()
    .map_or_else(|| PathBuf::from("."), PathBuf::from);
  let length = Duration::from_secs(seconds);
  let config = orbolay_core::config::load_config().unwrap_or_default();

  println!("Recording {seconds} s of microphone and loopback...");
  let epoch = Epoch::now();
  let mic = MicSource::new(config.rtt_mic_device);
  let loopback = LoopbackSource::new(config.rtt_loopback_device);
  let mic = std::thread::spawn(move || record(mic, epoch, length));
  let loopback = record(loopback, epoch, length);
  let mic = mic.join().unwrap_or_default();

  for (name, samples) in [("rtt-mic.wav", mic), ("rtt-loopback.wav", loopback)] {
    let path = dir.join(name);
    write_wav(&path, &samples)?;
    let secs = samples.len() as f64 / f64::from(TARGET_RATE);
    println!("{}: {secs:.2} s, {TARGET_RATE} Hz mono", path.display());
  }
  Ok(())
}
