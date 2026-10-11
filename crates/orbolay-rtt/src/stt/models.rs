//! Speech models on disk and the explicit, user-triggered download of the known ones.
//!
//! Nothing here runs on its own: [`download`] is only called from a button in the settings.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

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
  let target = local_path(model).ok_or("no config directory")?;
  let dir = target.parent().ok_or("invalid model path")?;
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let partial = target.with_extension("bin.part");
  let result = fetch(&download_url(model), &partial);
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
  let response = ureq::get(url).call().map_err(|e| e.to_string())?;
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
  fn missing_model_is_not_installed() {
    assert!(!is_installed("definitely-not-a-model-xyz"));
  }
}
