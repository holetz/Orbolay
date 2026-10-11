use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranscriptSource {
  #[default]
  Mic,
  Loopback,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptLine {
  pub id: String,
  pub speaker_id: Option<String>,
  pub speaker_name: String,
  pub text: String,
  /// Milliseconds since the Unix epoch.
  pub started_at: u64,
  pub ended_at: u64,
  pub is_partial: bool,
  pub source: TranscriptSource,
}

/// Appends a line, dropping the oldest ones so at most `cap` remain.
/// A line whose `id` already exists replaces that line in place.
pub fn push_capped(buffer: &mut VecDeque<TranscriptLine>, line: TranscriptLine, cap: usize) {
  if let Some(existing) = buffer.iter_mut().find(|l| l.id == line.id) {
    *existing = line;
  } else {
    buffer.push_back(line);
  }

  while buffer.len() > cap {
    buffer.pop_front();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn line(id: &str, text: &str, is_partial: bool) -> TranscriptLine {
    TranscriptLine {
      id: id.into(),
      speaker_id: Some("1".into()),
      speaker_name: "Alice".into(),
      text: text.into(),
      started_at: 0,
      ended_at: 0,
      is_partial,
      source: TranscriptSource::Mic,
    }
  }

  #[test]
  fn drops_oldest_beyond_cap() {
    let mut buf = VecDeque::new();
    for i in 0..5 {
      push_capped(&mut buf, line(&i.to_string(), "x", false), 3);
    }
    let ids: Vec<_> = buf.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["2", "3", "4"]);
  }

  #[test]
  fn same_id_replaces_in_place() {
    let mut buf = VecDeque::new();
    push_capped(&mut buf, line("a", "hel", true), 3);
    push_capped(&mut buf, line("b", "other", false), 3);
    push_capped(&mut buf, line("a", "hello", false), 3);
    assert_eq!(buf.len(), 2);
    assert_eq!(buf[0].text, "hello");
    assert!(!buf[0].is_partial);
  }

  #[test]
  fn zero_cap_keeps_nothing() {
    let mut buf = VecDeque::new();
    push_capped(&mut buf, line("a", "x", false), 0);
    assert!(buf.is_empty());
  }
}
