//! Wires capture to captions: sources → resample → VAD → one engine worker → `AppState.transcript`.
//!
//! Each source gets its own segmenter thread; both share one engine worker behind a bounded
//! queue, so CPU use is bounded by a single transcription at a time. No STT work ever runs on an
//! audio thread.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use orbolay_core::app_state::AppHandle;
use orbolay_core::config::{Config, config_dir};
use orbolay_core::transcript::{TranscriptLine, TranscriptSource};

use crate::attribution::attribute_loopback;
#[cfg(target_os = "linux")]
use crate::audio::LoopbackSource;
use crate::audio::{AudioSource, Epoch, MicSource, MonoResampler};
use crate::stt::{AudioSegment, SttEngine, VadConfig, VadSegmenter, WhisperConfig, WhisperEngine};

/// Segments waiting for the engine before the oldest provisional ones are dropped.
const QUEUE_CAP: usize = 8;
const POLL: Duration = Duration::from_millis(100);

/// Resolves `rttModel` to `<config dir>/models/<name>.bin`. Only [`KNOWN_MODELS`] names resolve:
/// a path, an existing file, an unknown name or anything that is not a bare file name is `None`,
/// so the config can never point the engine at an arbitrary file.
///
/// [`KNOWN_MODELS`]: crate::stt::models::KNOWN_MODELS
pub fn model_path(model: &str) -> Option<PathBuf> {
  if !is_bare_name(model) || !crate::stt::models::is_known(model) {
    return None;
  }
  let file = if model.ends_with(".bin") {
    model.to_string()
  } else {
    format!("{model}.bin")
  };
  Some(config_dir()?.join("models").join(file))
}

/// A single normal path component: no separators, no `.`/`..`, not empty.
pub fn is_bare_name(name: &str) -> bool {
  let mut parts = std::path::Path::new(name).components();
  matches!(parts.next(), Some(std::path::Component::Normal(_)))
    && parts.next().is_none()
    && !name.contains(['/', '\\'])
}

/// Everything the pipeline is started from; a change to any of it restarts the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineConfig {
  pub whisper: WhisperConfig,
  pub mic_device: Option<String>,
  pub loopback_device: Option<String>,
}

impl PipelineConfig {
  pub fn from_config(config: &Config) -> Self {
    Self {
      whisper: WhisperConfig {
        model_path: model_path(&config.rtt_model).unwrap_or_default(),
        language: config.rtt_language.clone(),
        threads: config.rtt_threads,
      },
      mic_device: config.rtt_mic_device.clone(),
      loopback_device: config.rtt_loopback_device.clone(),
    }
  }
}

struct Job {
  source: TranscriptSource,
  segment: AudioSegment,
}

/// Bounded FIFO that never drops a final segment. A newer provisional replaces a queued one of
/// the same utterance, a final supersedes them, and overflow evicts the oldest provisional.
#[derive(Default)]
struct JobQueue {
  jobs: Mutex<VecDeque<Job>>,
  ready: Condvar,
}

impl JobQueue {
  fn push(&self, job: Job) {
    let mut jobs = self.jobs.lock().unwrap();
    let same = |j: &Job| j.source == job.source && j.segment.utterance == job.segment.utterance;
    jobs.retain(|j| j.segment.is_final || !same(j));
    jobs.push_back(job);
    while jobs.len() > QUEUE_CAP {
      match jobs.iter().position(|j| !j.segment.is_final) {
        Some(oldest) => jobs.remove(oldest),
        None => break,
      };
    }
    self.ready.notify_one();
  }

  fn pop(&self, timeout: Duration) -> Option<Job> {
    let jobs = self.jobs.lock().unwrap();
    let (mut jobs, _) = self
      .ready
      .wait_timeout_while(jobs, timeout, |j| j.is_empty())
      .unwrap();
    jobs.pop_front()
  }
}

pub struct Pipeline {
  stop: Arc<AtomicBool>,
  sources: Vec<Box<dyn AudioSource>>,
  threads: Vec<JoinHandle<()>>,
}

impl Pipeline {
  /// Loads the engine and opens every source it can. Fails when the engine cannot load or no
  /// source opens; a source that fails alone is returned as a warning.
  pub fn start(config: &PipelineConfig, app: AppHandle) -> Result<(Self, Vec<String>), String> {
    let engine = WhisperEngine::new(&config.whisper).map_err(|e| e.to_string())?;
    let epoch = Epoch::now();
    let origin = Instant::now();
    let wall_origin = SystemTime::now();

    let mut candidates: Vec<(TranscriptSource, Box<dyn AudioSource>)> = vec![(
      TranscriptSource::Mic,
      Box::new(MicSource::new(config.mic_device.clone())),
    )];
    #[cfg(target_os = "linux")]
    candidates.push((
      TranscriptSource::Loopback,
      Box::new(LoopbackSource::new(config.loopback_device.clone())),
    ));

    let stop = Arc::new(AtomicBool::new(false));
    let queue = Arc::new(JobQueue::default());
    let mut pipeline = Self {
      stop: stop.clone(),
      sources: Vec::new(),
      threads: Vec::new(),
    };
    let mut warnings = Vec::new();

    for (kind, mut source) in candidates {
      match source.start(epoch) {
        Ok(frames) => {
          let (stop, queue) = (stop.clone(), queue.clone());
          pipeline.threads.push(std::thread::spawn(move || {
            segment_source(kind, &frames, &queue, &stop)
          }));
          pipeline.sources.push(source);
        }
        Err(err) => warnings.push(format!("{kind:?} capture unavailable: {err}")),
      }
    }

    if pipeline.sources.is_empty() {
      return Err(warnings.join("; "));
    }

    pipeline.threads.push(std::thread::spawn(move || {
      run_engine(engine, &queue, &stop, &app, wall_origin, origin)
    }));
    Ok((pipeline, warnings))
  }
}

impl Drop for Pipeline {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    for source in &mut self.sources {
      source.stop();
    }
    for thread in self.threads.drain(..) {
      thread.join().ok();
    }
  }
}

fn segment_source(
  source: TranscriptSource,
  frames: &flume::Receiver<crate::audio::AudioFrame>,
  queue: &JobQueue,
  stop: &AtomicBool,
) {
  let mut resampler = MonoResampler::default();
  let mut vad = VadSegmenter::new(VadConfig::default());

  while !stop.load(Ordering::Relaxed) {
    let frame = match frames.recv_timeout(POLL) {
      Ok(frame) => frame,
      Err(flume::RecvTimeoutError::Timeout) => continue,
      Err(flume::RecvTimeoutError::Disconnected) => break,
    };
    let Ok(Some(mono)) = resampler.process(&frame) else {
      continue;
    };
    for segment in vad.push(&mono) {
      queue.push(Job { source, segment });
    }
  }

  if let Some(segment) = vad.flush() {
    queue.push(Job { source, segment });
  }
}

fn run_engine(
  mut engine: WhisperEngine,
  queue: &JobQueue,
  stop: &AtomicBool,
  app: &AppHandle,
  wall_origin: SystemTime,
  origin: Instant,
) {
  while !stop.load(Ordering::Relaxed) {
    let Some(job) = queue.pop(POLL) else {
      continue;
    };
    if is_discard(&job.segment) {
      app.update(|state| {
        let id = line_id(job.source, job.segment.utterance);
        state.transcript.retain(|l| l.id != id);
      });
      continue;
    }
    let Ok(text) = engine.transcribe(&job.segment) else {
      continue;
    };
    let text = text
      .iter()
      .map(|t| t.text.as_str())
      .collect::<Vec<_>>()
      .join(" ");
    app.update(|state| {
      let (speaker_id, speaker_name) = match job.source {
        TranscriptSource::Mic => speaker(job.source, &state.user_id, &state.voice_users),
        TranscriptSource::Loopback => attribute_loopback(
          &state.speaking,
          &state.voice_users,
          &state.user_id,
          origin + job.segment.start,
          origin + job.segment.end(),
          state.config.rtt_event_offset_ms,
        ),
      };
      let line = transcript_line(&job, text, speaker_id, speaker_name, wall_origin);
      if line.text.is_empty() {
        // Nothing recognised: drop the provisional line this utterance may have left behind.
        if job.segment.is_final {
          state.transcript.retain(|l| l.id != line.id);
        }
      } else {
        state.push_transcript(line);
      }
    });
  }
}

/// A final with no audio closes an utterance the VAD dropped: its provisional line must go.
fn is_discard(segment: &AudioSegment) -> bool {
  segment.is_final && segment.samples.is_empty()
}

fn line_id(source: TranscriptSource, utterance: u64) -> String {
  let source = match source {
    TranscriptSource::Mic => "mic",
    TranscriptSource::Loopback => "loopback",
  };
  format!("{source}-{utterance}")
}

/// The mic is always the local user; loopback is attributed from the speaking timeline.
fn speaker(
  source: TranscriptSource,
  user_id: &str,
  voice_users: &[orbolay_core::user::User],
) -> (Option<String>, String) {
  match source {
    TranscriptSource::Mic => {
      let name = voice_users
        .iter()
        .find(|u| u.id == user_id)
        .map_or_else(|| "You".to_string(), |u| u.name.clone());
      ((!user_id.is_empty()).then(|| user_id.to_string()), name)
    }
    TranscriptSource::Loopback => (None, "Unknown".to_string()),
  }
}

fn unix_ms(origin: SystemTime, offset: Duration) -> u64 {
  (origin + offset)
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_millis() as u64)
}

fn transcript_line(
  job: &Job,
  text: String,
  speaker_id: Option<String>,
  speaker_name: String,
  wall_origin: SystemTime,
) -> TranscriptLine {
  TranscriptLine {
    id: line_id(job.source, job.segment.utterance),
    speaker_id,
    speaker_name,
    text,
    started_at: unix_ms(wall_origin, job.segment.start),
    ended_at: unix_ms(wall_origin, job.segment.end()),
    is_partial: !job.segment.is_final,
    source: job.source,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn job(source: TranscriptSource, utterance: u64, is_final: bool) -> Job {
    Job {
      source,
      segment: AudioSegment {
        utterance,
        start: Duration::from_secs(1),
        samples: vec![0.; 1600],
        is_final,
      },
    }
  }

  fn queued(queue: &JobQueue) -> Vec<(u64, bool)> {
    let jobs = queue.jobs.lock().unwrap();
    jobs
      .iter()
      .map(|j| (j.segment.utterance, j.segment.is_final))
      .collect()
  }

  #[test]
  fn newer_provisional_and_final_supersede_queued_provisionals() {
    let queue = JobQueue::default();
    queue.push(job(TranscriptSource::Mic, 1, false));
    queue.push(job(TranscriptSource::Loopback, 1, false));
    queue.push(job(TranscriptSource::Mic, 1, false));
    assert_eq!(queue.jobs.lock().unwrap().len(), 2);
    queue.push(job(TranscriptSource::Mic, 1, true));
    let left: Vec<_> = queue
      .jobs
      .lock()
      .unwrap()
      .iter()
      .map(|j| (j.source, j.segment.is_final))
      .collect();
    assert_eq!(
      left,
      [
        (TranscriptSource::Loopback, false),
        (TranscriptSource::Mic, true)
      ]
    );
  }

  #[test]
  fn overflow_drops_oldest_provisional_never_finals() {
    let queue = JobQueue::default();
    queue.push(job(TranscriptSource::Loopback, 100, false));
    queue.push(job(TranscriptSource::Loopback, 101, false));
    for u in 0..QUEUE_CAP as u64 - 1 {
      queue.push(job(TranscriptSource::Mic, u, true));
    }
    let q = queued(&queue);
    assert_eq!(q.len(), QUEUE_CAP);
    assert_eq!(q[0], (101, false), "the oldest provisional went first");

    queue.push(job(TranscriptSource::Mic, 50, true));
    queue.push(job(TranscriptSource::Mic, 51, true));
    let q = queued(&queue);
    assert_eq!(q.len(), QUEUE_CAP + 1, "finals are kept past the cap");
    assert!(q.iter().all(|(_, is_final)| *is_final));
  }

  #[test]
  fn pop_waits_then_times_out() {
    let queue = JobQueue::default();
    assert!(queue.pop(Duration::from_millis(10)).is_none());
    queue.push(job(TranscriptSource::Mic, 7, true));
    assert_eq!(
      queue
        .pop(Duration::from_millis(10))
        .map(|j| j.segment.utterance),
      Some(7)
    );
  }

  #[test]
  fn empty_final_is_a_discard_and_nothing_else_is() {
    let mut j = job(TranscriptSource::Mic, 4, true);
    assert!(!is_discard(&j.segment));
    j.segment.samples.clear();
    assert!(is_discard(&j.segment));
    j.segment.is_final = false;
    assert!(!is_discard(&j.segment));
    assert_eq!(line_id(TranscriptSource::Loopback, 4), "loopback-4");
  }

  #[test]
  fn mic_is_local_user_and_loopback_is_unknown() {
    let (id, name) = speaker(TranscriptSource::Mic, "42", &[]);
    assert_eq!((id.as_deref(), name.as_str()), (Some("42"), "You"));
    let (id, name) = speaker(TranscriptSource::Loopback, "42", &[]);
    assert_eq!((id, name.as_str()), (None, "Unknown"));
  }

  #[test]
  fn line_carries_wall_clock_span_and_partial_flag() {
    let origin = UNIX_EPOCH + Duration::from_secs(1000);
    let line = transcript_line(
      &job(TranscriptSource::Loopback, 3, false),
      "hi".into(),
      None,
      "Unknown".into(),
      origin,
    );
    assert_eq!(line.id, "loopback-3");
    assert_eq!((line.started_at, line.ended_at), (1_001_000, 1_001_100));
    assert!(line.is_partial);
  }

  #[test]
  fn model_name_resolves_into_models_dir() {
    let path = model_path("ggml-base").unwrap();
    assert!(path.ends_with("models/ggml-base.bin"));
    assert_eq!(
      model_path("ggml-base.bin").unwrap().file_name().unwrap(),
      "ggml-base.bin"
    );
  }

  #[test]
  fn existing_absolute_file_is_not_a_model() {
    let file = std::env::temp_dir().join(format!("orbolay-notmodel-{}.bin", std::process::id()));
    std::fs::write(&file, b"x").unwrap();
    let got = model_path(file.to_str().unwrap());
    std::fs::remove_file(&file).unwrap();
    assert_eq!(got, None);
    assert_eq!(model_path("/etc/passwd"), None);
  }

  #[test]
  fn unknown_bare_name_is_not_a_model() {
    assert_eq!(model_path("x.bin"), None);
    assert_eq!(model_path("ggml-unknown"), None);
  }
}
