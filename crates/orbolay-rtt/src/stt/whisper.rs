//! Local recognition through whisper.cpp.

use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

use whisper_rs::{
  FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use super::{AudioSegment, SttEngine, SttError, TextSegment};

/// Upper bound for the default thread count, so a game keeps most of the CPU.
const MAX_DEFAULT_THREADS: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct WhisperConfig {
  pub model_path: PathBuf,
  /// ISO 639-1 code, or `auto` to detect the language per segment.
  pub language: String,
  /// `None` uses up to [`MAX_DEFAULT_THREADS`] of the available cores.
  pub threads: Option<usize>,
}

impl WhisperConfig {
  fn thread_count(&self) -> usize {
    self.threads.filter(|&n| n > 0).unwrap_or_else(|| {
      std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_DEFAULT_THREADS)
    })
  }
}

pub struct WhisperEngine {
  state: WhisperState,
  language: String,
  threads: usize,
}

impl WhisperEngine {
  pub fn new(config: &WhisperConfig) -> Result<Self, SttError> {
    if !config.model_path.is_file() {
      return Err(SttError::ModelNotFound(
        config.model_path.display().to_string(),
      ));
    }

    // Routes whisper.cpp's own logging away from stderr.
    static HOOKS: Once = Once::new();
    HOOKS.call_once(whisper_rs::install_logging_hooks);

    let ctx =
      WhisperContext::new_with_params(&config.model_path, WhisperContextParameters::default())
        .map_err(engine_error)?;
    let state = ctx.create_state().map_err(engine_error)?;

    Ok(Self {
      state,
      language: config.language.clone(),
      threads: config.thread_count(),
    })
  }
}

fn engine_error(err: whisper_rs::WhisperError) -> SttError {
  SttError::Engine(err.to_string())
}

/// whisper.cpp timestamps are in centiseconds from the start of the input.
fn centis(t: i64) -> Duration {
  Duration::from_millis(u64::try_from(t).unwrap_or(0) * 10)
}

impl SttEngine for WhisperEngine {
  fn transcribe(&mut self, segment: &AudioSegment) -> Result<Vec<TextSegment>, SttError> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(i32::try_from(self.threads).unwrap_or(1));
    params.set_language(Some(&self.language));
    params.set_no_context(true);
    params.set_single_segment(!segment.is_final);
    params.set_suppress_blank(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    self
      .state
      .full(params, &segment.samples)
      .map_err(engine_error)?;

    let mut out = Vec::new();
    for s in self.state.as_iter() {
      let text = s.to_str_lossy().map_err(engine_error)?.trim().to_string();
      if text.is_empty() {
        continue;
      }
      out.push(TextSegment {
        start: segment.start + centis(s.start_timestamp()),
        end: segment.start + centis(s.end_timestamp()),
        text,
        no_speech_prob: s.no_speech_probability(),
      });
    }
    Ok(out)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn config(threads: Option<usize>) -> WhisperConfig {
    WhisperConfig {
      model_path: PathBuf::from("/nonexistent/ggml-base.bin"),
      language: "auto".into(),
      threads,
    }
  }

  #[test]
  fn missing_model_is_reported() {
    let err = WhisperEngine::new(&config(None)).err().unwrap();
    assert!(matches!(err, SttError::ModelNotFound(_)));
  }

  #[test]
  fn explicit_threads_win_and_default_is_capped() {
    assert_eq!(config(Some(7)).thread_count(), 7);
    let auto = config(None).thread_count();
    assert!((1..=MAX_DEFAULT_THREADS).contains(&auto));
    assert_eq!(config(Some(0)).thread_count(), auto);
  }

  #[test]
  fn centiseconds_convert_and_clamp() {
    assert_eq!(centis(150), Duration::from_millis(1500));
    assert_eq!(centis(-3), Duration::ZERO);
  }
}
