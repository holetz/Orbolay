//! Speech models on disk and the explicit, user-triggered download of the known ones.
//!
//! Nothing here runs on its own: [`download`] is only called from a button in the settings.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::pipeline::model_path;

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Models offered for download, as names accepted by `rttModel`.
pub const KNOWN_MODELS: &[&str] = &[
  "ggml-tiny",
  "ggml-base",
  "ggml-small",
  "ggml-tiny.en",
  "ggml-base.en",
  "ggml-small.en",
];

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// ureq has no per-read timeout, so this bounds the whole body (the largest model is ~490 MB).
const BODY_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// SHA-256 of each known model, from the `lfs.oid` of
/// `https://huggingface.co/api/models/ggerganov/whisper.cpp/tree/main`.
fn expected_sha256(model: &str) -> Option<&'static str> {
  match model.strip_suffix(".bin").unwrap_or(model) {
    "ggml-tiny" => Some("be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"),
    "ggml-base" => Some("60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"),
    "ggml-small" => Some("1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"),
    "ggml-tiny.en" => Some("921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f"),
    "ggml-base.en" => Some("a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"),
    "ggml-small.en" => Some("c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d"),
    _ => None,
  }
}

/// Streams `path` through SHA-256 and compares it with `expected` (hex).
fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
  let mut file = File::open(path).map_err(|e| e.to_string())?;
  let mut hasher = Sha256::new();
  let mut buf = [0u8; 64 * 1024];
  loop {
    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
    if n == 0 {
      break;
    }
    hasher.update(&buf[..n]);
  }
  let actual: String = hasher
    .finalize()
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
  if actual.eq_ignore_ascii_case(expected) {
    Ok(())
  } else {
    Err(format!(
      "checksum mismatch: expected {expected}, got {actual}"
    ))
  }
}

/// Where `model` lives (or will live) on disk.
pub fn local_path(model: &str) -> Option<PathBuf> {
  model_path(model)
}

pub fn is_installed(model: &str) -> bool {
  local_path(model).is_some_and(|p| p.is_file())
}

pub fn download_url(model: &str) -> String {
  let file = model.strip_suffix(".bin").unwrap_or(model);
  format!("{BASE_URL}/{file}.bin")
}

/// Known models first, then any other `.bin` found in the models directory, without duplicates.
pub fn available_models() -> Vec<String> {
  let mut names: Vec<String> = KNOWN_MODELS.iter().map(|s| s.to_string()).collect();
  if let Some(dir) = local_path("x").and_then(|p| p.parent().map(Path::to_path_buf))
    && let Ok(entries) = fs::read_dir(dir)
  {
    let mut extra: Vec<String> = entries
      .filter_map(|e| e.ok())
      .filter_map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.strip_suffix(".bin").map(str::to_string)
      })
      .filter(|n| !names.contains(n))
      .collect();
    extra.sort();
    names.extend(extra);
  }
  names
}

/// Downloads a known model into the models directory, through a temporary file so a failed
/// transfer never leaves a truncated model behind. Blocking; returns the final path.
pub fn download(model: &str) -> Result<PathBuf, String> {
  let expected = expected_sha256(model).ok_or_else(|| format!("no pinned checksum for {model}"))?;
  let target = local_path(model).ok_or("no config directory")?;
  let dir = target.parent().ok_or("invalid model path")?;
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let partial = target.with_extension("bin.part");
  let result = fetch(&download_url(model), &partial)
    .and_then(|()| verify_sha256(&partial, expected).map_err(|e| format!("{model}: {e}")));
  match result {
    Ok(()) => {
      fs::rename(&partial, &target).map_err(|e| e.to_string())?;
      Ok(target)
    }
    Err(e) => {
      let _ = fs::remove_file(&partial);
      Err(e)
    }
  }
}

fn fetch(url: &str, dest: &Path) -> Result<(), String> {
  let agent: ureq::Agent = ureq::Agent::config_builder()
    .timeout_connect(Some(CONNECT_TIMEOUT))
    .timeout_recv_response(Some(RESPONSE_TIMEOUT))
    .timeout_recv_body(Some(BODY_TIMEOUT))
    .build()
    .into();
  let response = agent.get(url).call().map_err(|e| e.to_string())?;
  let mut reader = response.into_body().into_reader();
  let mut file = File::create(dest).map_err(|e| e.to_string())?;
  io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn url_names_the_bin_file() {
    assert_eq!(
      download_url("ggml-base"),
      format!("{BASE_URL}/ggml-base.bin")
    );
    assert_eq!(
      download_url("ggml-base.bin"),
      format!("{BASE_URL}/ggml-base.bin")
    );
  }

  #[test]
  fn known_models_are_listed_without_duplicates() {
    let names = available_models();
    for known in KNOWN_MODELS {
      assert_eq!(names.iter().filter(|n| n == known).count(), 1);
    }
  }

  #[test]
  fn every_known_model_has_a_pinned_checksum() {
    for known in KNOWN_MODELS {
      let sum = expected_sha256(known).unwrap_or_else(|| panic!("no pin for {known}"));
      assert_eq!(sum.len(), 64);
    }
    assert!(expected_sha256("not-a-model").is_none());
  }

  #[test]
  fn corrupt_download_is_rejected() {
    let path = std::env::temp_dir().join(format!("orbolay-model-test-{}.bin", std::process::id()));
    fs::write(&path, b"<html>404 but 200</html>").unwrap();
    let pinned = expected_sha256("ggml-tiny").unwrap();
    let err = verify_sha256(&path, pinned).unwrap_err();
    assert!(err.contains("checksum mismatch"), "{err}");
    // The digest of the exact bytes is accepted (SHA-256 of "abc").
    fs::write(&path, b"abc").unwrap();
    verify_sha256(
      &path,
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
  }

  #[test]
  fn missing_model_is_not_installed() {
    assert!(!is_installed("definitely-not-a-model-xyz"));
  }
}
