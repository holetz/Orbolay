//! Transcribes a bundled clip with a real model. Built only with `--features test-model`, so the
//! default test run never needs a model; skipped when no model file is found.
//!
//! The model is `$ORBOLAY_TEST_MODEL`, else `ggml-base.bin` in the Orbolay models dir.
//! Fixture: `fixtures/jfk.wav`, from the 1961 inaugural address (public domain), 16 kHz mono.

use std::path::PathBuf;
use std::time::Duration;

use orbolay_rtt::stt::{AudioSegment, SttEngine, WhisperConfig, WhisperEngine};

fn model_path() -> Option<PathBuf> {
  let path = match std::env::var_os("ORBOLAY_TEST_MODEL") {
    Some(path) => PathBuf::from(path),
    None => orbolay_core::config::config_dir()?
      .join("models")
      .join("ggml-base.bin"),
  };
  path.is_file().then_some(path)
}

/// Reads a 16-bit PCM mono WAV as `f32` samples in `[-1, 1)`.
fn read_wav_mono_16k(bytes: &[u8]) -> Vec<f32> {
  assert_eq!(&bytes[0..4], b"RIFF");
  assert_eq!(&bytes[8..12], b"WAVE");
  let mut pos = 12;
  let mut format = None;
  while pos + 8 <= bytes.len() {
    let id = &bytes[pos..pos + 4];
    let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
    let body = &bytes[pos + 8..(pos + 8 + len).min(bytes.len())];
    match id {
      b"fmt " => {
        let channels = u16::from_le_bytes([body[2], body[3]]);
        let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
        let bits = u16::from_le_bytes([body[14], body[15]]);
        format = Some((channels, rate, bits));
      }
      b"data" => {
        assert_eq!(
          format,
          Some((1, 16_000, 16)),
          "fixture must be 16 kHz mono s16"
        );
        let (pairs, _) = body.as_chunks::<2>();
        return pairs
          .iter()
          .map(|&b| f32::from(i16::from_le_bytes(b)) / 32768.)
          .collect();
      }
      _ => {}
    }
    pos += 8 + len + (len & 1);
  }
  panic!("no data chunk");
}

#[test]
fn transcribes_fixture_key_words() {
  let Some(model_path) = model_path() else {
    eprintln!("skipped: no model (set ORBOLAY_TEST_MODEL)");
    return;
  };

  let mut engine = WhisperEngine::new(&WhisperConfig {
    model_path,
    language: "en".into(),
    threads: None,
  })
  .expect("model loads");

  let segment = AudioSegment {
    utterance: 1,
    start: Duration::from_secs(10),
    samples: read_wav_mono_16k(include_bytes!("fixtures/jfk.wav")),
    is_final: true,
  };
  let text = engine.transcribe(&segment).expect("transcribes");
  assert!(!text.is_empty());
  assert!(
    text
      .iter()
      .all(|t| t.start >= segment.start && t.end <= segment.end() + Duration::from_secs(1))
  );

  let joined = text
    .iter()
    .map(|t| t.text.as_str())
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase();
  for word in ["fellow americans", "country"] {
    assert!(joined.contains(word), "{word:?} missing from {joined:?}");
  }
}
