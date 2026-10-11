//! Scripted two-speaker caption feed for screenshots and UI iteration (`--rtt-demo`).

use std::{
  thread,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use orbolay_core::{
  app_state::AppHandle,
  transcript::{TranscriptLine, TranscriptSource},
};

const SCRIPT: &[(&str, &str, &str)] = &[
  ("1", "Alice", "Hey, can you hear me okay?"),
  ("2", "Bob", "Loud and clear, the captions are working."),
  ("1", "Alice", "Nice, that was quick to set up."),
  (
    "2",
    "Bob",
    "Let's try a longer sentence so the partial updates show up while I keep talking.",
  ),
];

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or_default()
}

fn line(index: usize, speaker: &str, name: &str, text: &str, is_partial: bool) -> TranscriptLine {
  let now = now_ms();
  TranscriptLine {
    id: format!("demo-{index}"),
    speaker_id: Some(speaker.into()),
    speaker_name: name.into(),
    text: text.into(),
    started_at: now,
    ended_at: now,
    is_partial,
    source: if speaker == "1" {
      TranscriptSource::Mic
    } else {
      TranscriptSource::Loopback
    },
  }
}

/// Words of `text` accumulated one at a time, the way streaming STT refines a partial.
fn partial_steps(text: &str) -> Vec<String> {
  let words: Vec<_> = text.split_whitespace().collect();
  (1..=words.len()).map(|n| words[..n].join(" ")).collect()
}

pub fn spawn(app: AppHandle) {
  app.update(|state| state.config.rtt_enabled = true);

  thread::spawn(move || {
    let mut index = 0;
    loop {
      for (speaker, name, text) in SCRIPT {
        index += 1;
        let steps = partial_steps(text);
        for (i, step) in steps.iter().enumerate() {
          let last = i + 1 == steps.len();
          let mut update = line(index, speaker, name, step, !last);
          update.ended_at = now_ms();
          app.update(|state| state.push_transcript(update));
          thread::sleep(Duration::from_millis(250));
        }
        thread::sleep(Duration::from_millis(1500));
      }
      thread::sleep(Duration::from_secs(6));
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn partial_steps_grow_word_by_word() {
    assert_eq!(partial_steps("a b c"), ["a", "a b", "a b c"]);
  }

  #[test]
  fn script_has_two_speakers() {
    let mut ids: Vec<_> = SCRIPT.iter().map(|(id, _, _)| *id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 2);
  }
}
