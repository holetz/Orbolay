//! Labels a loopback segment with the Discord speaker(s) whose speaking events overlap it.

use std::time::{Duration, Instant};

use orbolay_core::speaking_timeline::SpeakingTimeline;
use orbolay_core::user::User;

/// A user covering at least this share of the segment is named in a joint label.
const JOINT_SHARE: f64 = 0.35;
const UNKNOWN: &str = "Unknown";

/// Moves `at` by `offset_ms`, leaving it as is when the shift would leave the clock's range.
fn shift(at: Instant, offset_ms: i32) -> Instant {
  let by = Duration::from_millis(u64::from(offset_ms.unsigned_abs()));
  let moved = if offset_ms >= 0 {
    at.checked_add(by)
  } else {
    at.checked_sub(by)
  };
  moved.unwrap_or(at)
}

/// Speaker id (only when a single user is named) and display label for a loopback segment
/// spanning `[from, to)`. Events are shifted by `offset_ms` to align with the audio; the local
/// user is never a candidate, as their own voice is not in the loopback.
pub fn attribute_loopback(
  timeline: &SpeakingTimeline,
  users: &[User],
  local_user_id: &str,
  from: Instant,
  to: Instant,
  offset_ms: i32,
) -> (Option<String>, String) {
  // Events arrive `offset_ms` off the audio, so the audio window moves the opposite way.
  let (from, to) = (shift(from, -offset_ms), shift(to, -offset_ms));
  let mut overlaps: Vec<_> = timeline
    .overlaps(from, to)
    .into_iter()
    .filter(|(id, _)| id != local_user_id)
    .collect();
  overlaps.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

  let name = |id: &str| {
    users
      .iter()
      .find(|u| u.id == id)
      .map_or_else(|| id.to_string(), |u| u.name.clone())
  };
  let len = to.saturating_duration_since(from).as_secs_f64();
  let share = |d: Duration| if len > 0. { d.as_secs_f64() / len } else { 0. };

  match overlaps.as_slice() {
    [] => (None, UNKNOWN.to_string()),
    [(top, _)] => (Some(top.clone()), name(top)),
    [(a, da), (b, db), ..] if share(*da) >= JOINT_SHARE && share(*db) >= JOINT_SHARE => {
      (None, format!("{} + {}", name(a), name(b)))
    }
    [(top, _), ..] => (Some(top.clone()), name(top)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use orbolay_core::user::UserVoiceState;

  fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
  }

  fn user(id: &str, name: &str) -> User {
    User {
      id: id.into(),
      name: name.into(),
      avatar: String::new(),
      voice_state: UserVoiceState::NotSpeaking,
      streaming: false,
      camera: false,
      volume: 100.,
      speaking_epoch: 0,
    }
  }

  fn users() -> Vec<User> {
    vec![user("1", "Me"), user("2", "Ann"), user("3", "Bob")]
  }

  #[test]
  fn majority_overlap_wins() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    tl.record_start("2", t0);
    tl.record_stop("2", t0 + ms(800));
    tl.record_start("3", t0 + ms(800));
    tl.record_stop("3", t0 + ms(1000));
    let got = attribute_loopback(&tl, &users(), "1", t0, t0 + ms(1000), 0);
    assert_eq!(got, (Some("2".into()), "Ann".into()));
  }

  #[test]
  fn two_substantial_speakers_get_a_joint_label() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    tl.record_start("2", t0);
    tl.record_stop("2", t0 + ms(600));
    tl.record_start("3", t0 + ms(400));
    tl.record_stop("3", t0 + ms(1000));
    let got = attribute_loopback(&tl, &users(), "1", t0, t0 + ms(1000), 0);
    assert_eq!(got, (None, "Ann + Bob".into()));
  }

  #[test]
  fn nobody_speaking_is_unknown_and_local_user_is_excluded() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    assert_eq!(
      attribute_loopback(&tl, &users(), "1", t0, t0 + ms(1000), 0),
      (None, "Unknown".into())
    );
    tl.record_start("1", t0);
    tl.record_stop("1", t0 + ms(1000));
    assert_eq!(
      attribute_loopback(&tl, &users(), "1", t0, t0 + ms(1000), 0),
      (None, "Unknown".into())
    );
  }

  #[test]
  fn offset_aligns_events_with_audio() {
    let t0 = Instant::now() + ms(5000);
    let mut tl = SpeakingTimeline::new();
    // Events arrive 200 ms before the audio they describe.
    tl.record_start("2", t0 - ms(200));
    tl.record_stop("2", t0 + ms(300));
    tl.record_start("3", t0 + ms(300));
    tl.record_stop("3", t0 + ms(900));
    // Window [t0+200, t0+900) holds 100 ms of Ann and 600 ms of Bob without a shift.
    let plain = attribute_loopback(&tl, &users(), "1", t0 + ms(200), t0 + ms(900), 0);
    assert_eq!(plain.0.as_deref(), Some("3"));
    // With +200 ms the window moves back to [t0, t0+700): Ann 300 ms, Bob 400 ms.
    let shifted = attribute_loopback(&tl, &users(), "1", t0 + ms(200), t0 + ms(900), 200);
    assert_eq!(shifted, (None, "Bob + Ann".into()));
  }

  #[test]
  fn unknown_user_id_falls_back_to_the_id() {
    let t0 = Instant::now();
    let mut tl = SpeakingTimeline::new();
    tl.record_start("9", t0);
    let got = attribute_loopback(&tl, &users(), "1", t0, t0 + ms(500), 0);
    assert_eq!(got, (Some("9".into()), "9".into()));
  }
}
