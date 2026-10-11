//! Who was speaking when, fed by the transports from Discord's speaking events.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Intervals older than this are dropped on the next write.
pub const RETENTION: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy)]
struct Interval {
  start: Instant,
  /// `None` while the user is still speaking.
  end: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
pub struct SpeakingTimeline {
  users: HashMap<String, Vec<Interval>>,
}

impl SpeakingTimeline {
  pub fn new() -> Self {
    Self::default()
  }

  /// Opens an interval; a repeated start while one is open is ignored.
  pub fn record_start(&mut self, user_id: &str, at: Instant) {
    self.prune(at);
    let intervals = self.users.entry(user_id.to_string()).or_default();
    if intervals.last().is_none_or(|i| i.end.is_some()) {
      intervals.push(Interval {
        start: at,
        end: None,
      });
    }
  }

  /// Closes the open interval, if any.
  pub fn record_stop(&mut self, user_id: &str, at: Instant) {
    self.prune(at);
    if let Some(open) = self
      .users
      .get_mut(user_id)
      .and_then(|v| v.last_mut())
      .filter(|i| i.end.is_none())
    {
      open.end = Some(at.max(open.start));
    }
  }

  /// Drops intervals that ended more than [`RETENTION`] before `now`, and users left empty.
  pub fn prune(&mut self, now: Instant) {
    let Some(cutoff) = now.checked_sub(RETENTION) else {
      return;
    };
    self.users.retain(|_, intervals| {
      intervals.retain(|i| i.end.is_none_or(|end| end >= cutoff));
      !intervals.is_empty()
    });
  }

  /// How long each user spoke within `[from, to)`; an open interval counts up to `to`.
  /// Users with no overlap are left out; the order is by user id.
  pub fn overlaps(&self, from: Instant, to: Instant) -> Vec<(String, Duration)> {
    let mut out: Vec<(String, Duration)> = self
      .users
      .iter()
      .filter_map(|(user, intervals)| {
        let total: Duration = intervals
          .iter()
          .map(|i| {
            let start = i.start.max(from);
            let end = i.end.unwrap_or(to).min(to);
            end.saturating_duration_since(start)
          })
          .sum();
        (!total.is_zero()).then(|| (user.clone(), total))
      })
      .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
  }

  #[test]
  fn closed_and_open_intervals_overlap_the_window() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    tl.record_start("a", t0 + ms(100));
    tl.record_stop("a", t0 + ms(400));
    tl.record_start("b", t0 + ms(800));
    let got = tl.overlaps(t0 + ms(200), t0 + ms(1000));
    assert_eq!(got, [("a".into(), ms(200)), ("b".into(), ms(200))]);
  }

  #[test]
  fn no_overlap_is_omitted_and_duplicate_start_ignored() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    tl.record_start("a", t0 + ms(100));
    tl.record_start("a", t0 + ms(300));
    tl.record_stop("a", t0 + ms(500));
    assert_eq!(
      tl.overlaps(t0 + ms(100), t0 + ms(500)),
      [("a".into(), ms(400))]
    );
    assert!(tl.overlaps(t0 + ms(600), t0 + ms(900)).is_empty());
    tl.record_stop("a", t0 + ms(700));
    assert_eq!(tl.overlaps(t0, t0 + ms(900)), [("a".into(), ms(400))]);
  }

  #[test]
  fn old_intervals_are_pruned() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    tl.record_start("a", t0);
    tl.record_stop("a", t0 + ms(100));
    tl.record_start("b", t0 + RETENTION + ms(500));
    assert!(tl.overlaps(t0, t0 + ms(200)).is_empty());
    assert_eq!(tl.users.len(), 1);
  }
}
