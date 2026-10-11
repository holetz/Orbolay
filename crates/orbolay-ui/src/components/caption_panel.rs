use std::str::FromStr;

use freya::prelude::*;

use orbolay_core::transcript::TranscriptLine;

use crate::{
  config::CornerAlignment,
  util::{scale::UiScale, theme::Theme},
};

const PARTIAL_OPACITY: f32 = 0.6;
/// How long the last stretch of a line's life is spent fading out, in milliseconds.
const FADE_WINDOW_MS: f32 = 1500.;

const SPEAKER_PALETTE: [(u8, u8, u8); 8] = [
  (240, 120, 120),
  (240, 170, 90),
  (230, 210, 100),
  (130, 210, 120),
  (100, 210, 200),
  (110, 160, 240),
  (170, 130, 240),
  (230, 130, 200),
];

/// Stable colour for a speaker, derived only from its id (or name when there is no id).
pub fn speaker_color(speaker_id: Option<&str>, speaker_name: &str) -> (u8, u8, u8) {
  let key = speaker_id.unwrap_or(speaker_name);
  // FNV-1a, so the choice does not depend on a randomized hasher
  let hash = key.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
    (hash ^ byte as u64).wrapping_mul(0x100000001b3)
  });
  SPEAKER_PALETTE[(hash % SPEAKER_PALETTE.len() as u64) as usize]
}

/// Opacity of a finished line `age_ms` after it ended; `None` once it is gone.
fn fade_opacity(age_ms: u64, fade_seconds: f32) -> Option<f32> {
  let total = fade_seconds.max(0.) * 1000.;
  let age = age_ms as f32;
  if age >= total {
    return None;
  }
  let fade_window = FADE_WINDOW_MS.min(total);
  Some(((total - age) / fade_window).clamp(0., 1.))
}

/// Opacity a line is drawn with, or `None` when it should not be shown.
pub fn line_opacity(
  line: &TranscriptLine,
  now_ms: u64,
  is_open: bool,
  fade_seconds: f32,
) -> Option<f32> {
  let base = if line.is_partial { PARTIAL_OPACITY } else { 1. };

  // Partial lines are still being spoken, and an open overlay keeps everything on screen
  if line.is_partial || is_open {
    return Some(base);
  }

  fade_opacity(now_ms.saturating_sub(line.ended_at), fade_seconds).map(|fade| fade * base)
}

#[derive(PartialEq)]
pub struct CaptionPanel {
  pub lines: Vec<TranscriptLine>,
  pub is_open: bool,
  pub alignment: String,
  pub offset_x: i32,
  pub offset_y: i32,
  pub max_lines: usize,
  pub fade_seconds: f32,
  pub font_scale: f32,
  pub show_avatars: bool,
  /// Milliseconds since the Unix epoch, used to age lines out.
  pub now_ms: u64,
  pub theme: Theme,
  pub ui_scale: f32,
}

impl Component for CaptionPanel {
  fn render(&self) -> impl IntoElement {
    // unwrap: this does not fail
    let alignment = CornerAlignment::from_str(&self.alignment).unwrap();
    let scale = UiScale::new(self.ui_scale);
    let gaps = alignment.to_gaps(scale.int(self.offset_x), scale.int(self.offset_y));
    let font_scale = if self.font_scale.is_finite() && self.font_scale > 0. {
      self.font_scale
    } else {
      1.
    };
    let font_size = scale.px(16. * font_scale);

    let visible: Vec<_> = self
      .lines
      .iter()
      .filter_map(|line| {
        line_opacity(line, self.now_ms, self.is_open, self.fade_seconds).map(|o| (line, o))
      })
      .collect();
    let skip = visible.len().saturating_sub(self.max_lines);

    visible.into_iter().skip(skip).fold(
      rect()
        .direction(Direction::Vertical)
        .cross_align(alignment.x.to_freya())
        .main_align(alignment.y.to_freya())
        .position(Position::new_absolute().top(0.).left(0.))
        .background(Color::TRANSPARENT)
        .height(Size::fill())
        .width(Size::fill())
        .padding(gaps),
      |el, (line, opacity)| {
        let (r, g, b) = speaker_color(line.speaker_id.as_deref(), &line.speaker_name);
        let color = Color::from_rgb(r, g, b);
        let initial = line
          .speaker_name
          .chars()
          .next()
          .map(|c| c.to_uppercase().to_string())
          .unwrap_or_default();

        el.child(
          rect()
            .key(line.id.clone())
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(scale.px(8.))
            .margin(Gaps::new(scale.px(2.), 0., scale.px(2.), 0.))
            .padding(Gaps::new_all(scale.px(6.)))
            .background(self.theme.transparent_gray)
            .corner_radius(self.theme.border_radius)
            .opacity(opacity)
            .maybe(self.show_avatars, |el| {
              el.child(
                rect()
                  .width(Size::px(font_size * 1.6))
                  .height(Size::px(font_size * 1.6))
                  .main_align(Alignment::Center)
                  .cross_align(Alignment::Center)
                  .background(color)
                  .corner_radius(font_size * 0.8)
                  .child(
                    label()
                      .text(initial)
                      .color(self.theme.gray)
                      .font_size(font_size),
                  ),
              )
            })
            .child(
              label()
                .text(format!("{}:", line.speaker_name))
                .color(color)
                .font_weight(FontWeight::BOLD)
                .font_size(font_size),
            )
            .child(
              label()
                .text(line.text.clone())
                .color(self.theme.text_color)
                .font_size(font_size),
            ),
        )
      },
    )
  }
}

#[cfg(test)]
mod tests {
  use orbolay_core::transcript::TranscriptSource;

  use super::*;

  fn line(is_partial: bool, ended_at: u64) -> TranscriptLine {
    TranscriptLine {
      id: "a".into(),
      speaker_id: Some("42".into()),
      speaker_name: "Alice".into(),
      text: "hi".into(),
      started_at: 0,
      ended_at,
      is_partial,
      source: TranscriptSource::Mic,
    }
  }

  #[test]
  fn speaker_color_is_stable_and_id_based() {
    assert_eq!(
      speaker_color(Some("42"), "Alice"),
      speaker_color(Some("42"), "Renamed")
    );
  }

  #[test]
  fn partial_lines_are_dimmed_and_never_fade() {
    assert_eq!(
      line_opacity(&line(true, 0), 1_000_000, false, 8.),
      Some(0.6)
    );
  }

  #[test]
  fn finished_lines_fade_out_when_closed() {
    let l = line(false, 0);
    assert_eq!(line_opacity(&l, 1_000, false, 8.), Some(1.));
    let mid = line_opacity(&l, 7_250, false, 8.).unwrap();
    assert!(mid > 0. && mid < 1.);
    assert_eq!(line_opacity(&l, 8_000, false, 8.), None);
    assert_eq!(line_opacity(&l, 8_000, true, 8.), Some(1.));
  }
}
