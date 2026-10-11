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

/// whisper's encoder covers 30 s of audio in 1500 positions (50 per second).
const AUDIO_CTX_PER_SECOND: usize = 50;
const FULL_AUDIO_CTX: usize = 1500;
/// Positions added past the clip so its last words are not cut off.
const AUDIO_CTX_MARGIN: usize = 64;
/// Below this whisper's output degrades sharply, so short clips still get this much context.
const MIN_AUDIO_CTX: usize = 256;

/// The encoder context for a clip of `samples` at 16 kHz: proportional to its length instead of
/// whisper's fixed 30 s window, which makes short clips several times cheaper to encode.
pub fn audio_ctx_for(samples: usize) -> usize {
  let positions = (samples * AUDIO_CTX_PER_SECOND).div_ceil(16_000);
  (positions + AUDIO_CTX_MARGIN).clamp(MIN_AUDIO_CTX, FULL_AUDIO_CTX)
}

/// Segments whose no-speech probability exceeds this are dropped outright.
const NO_SPEECH_THRESHOLD: f32 = 0.6;
/// Below this mean token probability a segment counts as low-confidence.
const LOW_CONFIDENCE: f32 = 0.5;
/// Above this no-speech probability a segment counts as low-confidence.
const DOUBTFUL_NO_SPEECH: f32 = 0.2;

/// Phrases Whisper invents on noise and music (subtitle credits, video outros), compared after
/// lowercasing and stripping punctuation. Dropped only when the segment is low-confidence, so a
/// real "thank you" survives.
const HALLUCINATIONS: &[&str] = &[
  "thank you",
  "thanks for watching",
  "thank you for watching",
  "thank you so much for watching",
  "please subscribe",
  "subscribe to my channel",
  "like and subscribe",
  "see you next time",
  "bye",
  "you",
  "subtitles by the amaraorg community",
  "obrigado",
  "obrigado por assistir",
  "legendas pela comunidade amaraorg",
  "gracias por ver el video",
  "untertitel der amaraorg community",
  "soustitres réalisés par la communauté damaraorg",
];

fn normalise(text: &str) -> String {
  text
    .chars()
    .filter(|c| c.is_alphanumeric() || c.is_whitespace())
    .collect::<String>()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase()
}

/// Whether a recognised segment is real speech rather than a hallucination on noise.
fn is_plausible(text: &str, no_speech_prob: f32, mean_token_prob: f32) -> bool {
  if no_speech_prob > NO_SPEECH_THRESHOLD {
    return false;
  }
  let norm = normalise(text);
  // Pure annotations such as "[BLANK_AUDIO]", "(music)" or "♪♪" carry no words.
  if norm.is_empty() || is_annotation(text) {
    return false;
  }
  let doubtful = mean_token_prob < LOW_CONFIDENCE || no_speech_prob > DOUBTFUL_NO_SPEECH;
  !(doubtful && HALLUCINATIONS.contains(&norm.as_str()))
}

fn is_annotation(text: &str) -> bool {
  let t = text.trim();
  (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'))
}

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
    params.set_audio_ctx(i32::try_from(audio_ctx_for(segment.samples.len())).unwrap_or(0));
    params.set_single_segment(!segment.is_final);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_no_speech_thold(NO_SPEECH_THRESHOLD);
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
      let no_speech_prob = s.no_speech_probability();
      let tokens = s.n_tokens().max(0);
      let mean_token_prob = if tokens == 0 {
        0.
      } else {
        (0..tokens)
          .filter_map(|i| s.get_token(i))
          .map(|t| t.token_probability())
          .sum::<f32>()
          / tokens as f32
      };
      if !is_plausible(&text, no_speech_prob, mean_token_prob) {
        continue;
      }
      out.push(TextSegment {
        start: segment.start + centis(s.start_timestamp()),
        end: segment.start + centis(s.end_timestamp()),
        text,
        no_speech_prob,
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
  fn audio_ctx_scales_with_the_clip_and_stays_in_bounds() {
    assert_eq!(audio_ctx_for(0), MIN_AUDIO_CTX);
    assert_eq!(audio_ctx_for(16_000 * 8), 8 * 50 + AUDIO_CTX_MARGIN);
    assert_eq!(audio_ctx_for(16_000 * 30), FULL_AUDIO_CTX);
    assert_eq!(audio_ctx_for(16_000 * 60), FULL_AUDIO_CTX);
  }

  #[test]
  fn explicit_threads_win_and_default_is_capped() {
    assert_eq!(config(Some(7)).thread_count(), 7);
    let auto = config(None).thread_count();
    assert!((1..=MAX_DEFAULT_THREADS).contains(&auto));
    assert_eq!(config(Some(0)).thread_count(), auto);
  }

  #[test]
  fn guard_drops_silence_annotations_and_doubtful_stock_phrases() {
    assert!(
      !is_plausible("Hello there", 0.9, 0.9),
      "no-speech probability too high"
    );
    assert!(!is_plausible("[BLANK_AUDIO]", 0.0, 0.9));
    assert!(!is_plausible("(music)", 0.0, 0.9));
    assert!(!is_plausible(" ♪ ♪ ", 0.0, 0.9));
    assert!(!is_plausible("Thanks for watching!", 0.1, 0.3));
    assert!(!is_plausible("Obrigado.", 0.3, 0.9));
  }

  #[test]
  fn guard_keeps_confident_speech_even_when_it_matches_a_stock_phrase() {
    assert!(is_plausible("Thank you.", 0.05, 0.9));
    assert!(is_plausible(
      "Ask not what your country can do for you",
      0.1,
      0.3
    ));
  }

  #[test]
  fn centiseconds_convert_and_clamp() {
    assert_eq!(centis(150), Duration::from_millis(1500));
    assert_eq!(centis(-3), Duration::ZERO);
  }
}
