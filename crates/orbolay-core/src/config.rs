use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::dirs::ensure_config_dirs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TransportMode {
  #[default]
  Ipc,
  Websocket,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DisplayVoiceMembers {
  Always,
  #[default]
  AlwaysSemiTransparent,
  WhenSpeaking,
}

impl Display for TransportMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      TransportMode::Ipc => write!(f, "ipc"),
      TransportMode::Websocket => write!(f, "websocket"),
    }
  }
}

impl From<String> for TransportMode {
  fn from(value: String) -> Self {
    match value.as_ref() {
      "ipc" => TransportMode::Ipc,
      "websocket" => TransportMode::Websocket,
      _ => TransportMode::Ipc,
    }
  }
}

impl Display for DisplayVoiceMembers {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DisplayVoiceMembers::Always => write!(f, "always"),
      DisplayVoiceMembers::AlwaysSemiTransparent => write!(f, "always (semi-transparent)"),
      DisplayVoiceMembers::WhenSpeaking => write!(f, "only when speaking"),
    }
  }
}

impl From<String> for DisplayVoiceMembers {
  fn from(value: String) -> Self {
    match value.as_ref() {
      "always" => DisplayVoiceMembers::Always,
      "always (semi-transparent)" => DisplayVoiceMembers::AlwaysSemiTransparent,
      "only when speaking" => DisplayVoiceMembers::WhenSpeaking,
      _ => DisplayVoiceMembers::Always,
    }
  }
}

fn default_overlay_keybind() -> Option<Vec<String>> {
  Some(vec!["ControlLeft".into(), "BackQuote".into()])
}

fn default_accent() -> (u8, u8, u8) {
  (30, 31, 35)
}

fn default_text() -> (u8, u8, u8) {
  (255, 255, 255)
}

fn default_border_radius() -> f32 {
  10.
}

fn default_ui_scale() -> f32 {
  1.0
}

fn default_user_width() -> f32 {
  320.
}

fn default_user_avatar_size() -> f32 {
  48.
}

fn default_user_font_size() -> f32 {
  14.
}

fn default_user_avatar_gap() -> f32 {
  8.
}

fn default_user_name_padding() -> f32 {
  6.
}

fn default_user_spacing() -> f32 {
  14.
}

fn default_rtt_max_lines() -> usize {
  6
}

fn default_rtt_fade_seconds() -> f32 {
  8.
}

fn default_rtt_font_scale() -> f32 {
  1.0
}

pub const DEFAULT_RTT_MODEL: &str = "ggml-base";

fn default_rtt_model() -> String {
  DEFAULT_RTT_MODEL.into()
}

fn default_rtt_language() -> String {
  "auto".into()
}

fn default_true() -> bool {
  true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
  #[serde(default = "default_overlay_keybind")]
  #[cfg(not(target_os = "macos"))]
  pub overlay_keybind: Option<Vec<String>>,
  #[serde(default)]
  pub display_idx: Option<usize>,
  #[serde(default)]
  pub port: Option<u16>,
  #[serde(default = "default_true")]
  pub enable_message_notifications: bool,
  #[serde(default)]
  pub message_alignment: Option<String>,
  #[serde(default)]
  pub user_alignment: Option<String>,
  #[serde(default)]
  pub message_offset_x: i32,
  #[serde(default)]
  pub message_offset_y: i32,
  #[serde(default)]
  pub user_offset_x: i32,
  #[serde(default)]
  pub user_offset_y: i32,
  /// Width (px) of each voice user row.
  #[serde(default = "default_user_width")]
  pub user_width: f32,
  /// Avatar diameter (px) of voice users; `0` hides the avatar.
  #[serde(default = "default_user_avatar_size")]
  pub user_avatar_size: f32,
  /// Font size (px) of voice user names; status icons follow it.
  #[serde(default = "default_user_font_size")]
  pub user_font_size: f32,
  /// Horizontal gap (px) between a voice user's avatar and name.
  #[serde(default = "default_user_avatar_gap")]
  pub user_avatar_gap: f32,
  /// Padding (px) inside a voice user's name box.
  #[serde(default = "default_user_name_padding")]
  pub user_name_padding: f32,
  /// Vertical space (px) between voice users.
  #[serde(default = "default_user_spacing")]
  pub user_spacing: f32,
  #[serde(default)]
  pub display_voice_members: Option<DisplayVoiceMembers>,
  #[serde(default)]
  pub messages_semitransparent: bool,
  #[serde(default)]
  pub is_keybind_enabled: Option<bool>,
  #[serde(default)]
  pub transport_mode: TransportMode,
  #[serde(default)]
  pub software_rendering: Option<bool>,
  #[serde(default = "default_true")]
  pub xwayland: bool,
  #[serde(default = "default_accent")]
  pub accent: (u8, u8, u8),
  #[serde(default = "default_text")]
  pub text_color: (u8, u8, u8),
  #[serde(default = "default_border_radius")]
  pub border_radius: f32,
  #[serde(default = "default_ui_scale")]
  pub ui_scale: f32,
  #[serde(default)]
  pub user_template: Option<String>,
  #[serde(default)]
  pub notification_template: Option<String>,
  #[serde(default)]
  pub rtt_enabled: bool,
  #[serde(default)]
  pub rtt_alignment: Option<String>,
  #[serde(default)]
  pub rtt_offset_x: i32,
  #[serde(default)]
  pub rtt_offset_y: i32,
  #[serde(default = "default_rtt_max_lines")]
  pub rtt_max_lines: usize,
  #[serde(default = "default_rtt_fade_seconds")]
  pub rtt_fade_seconds: f32,
  #[serde(default = "default_rtt_font_scale")]
  pub rtt_font_scale: f32,
  #[serde(default = "default_true")]
  pub rtt_show_avatars: bool,
  /// Input device for captions of what you say; `None` uses the system default.
  #[serde(default)]
  pub rtt_mic_device: Option<String>,
  /// PulseAudio source for captions of what others say; `None` uses the default sink's monitor.
  #[serde(default)]
  pub rtt_loopback_device: Option<String>,
  /// Speech model: a name looked up in the models dir (`ggml-base` → `ggml-base.bin`) or a path.
  #[serde(default = "default_rtt_model")]
  pub rtt_model: String,
  /// Spoken language as an ISO 639-1 code, or `auto` to detect it.
  #[serde(default = "default_rtt_language")]
  pub rtt_language: String,
  /// Threads the speech engine may use; `None` picks from the available cores.
  #[serde(default)]
  pub rtt_threads: Option<usize>,
  /// Shifts Discord speaking events (ms) to line them up with the captured audio.
  #[serde(default)]
  pub rtt_event_offset_ms: i32,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      #[cfg(not(target_os = "macos"))]
      overlay_keybind: default_overlay_keybind(),
      display_idx: None,
      port: Some(6888),
      enable_message_notifications: true,
      message_alignment: None,
      user_alignment: None,
      message_offset_x: 0,
      message_offset_y: 0,
      user_offset_x: 0,
      user_offset_y: 0,
      user_width: default_user_width(),
      user_avatar_size: default_user_avatar_size(),
      user_font_size: default_user_font_size(),
      user_avatar_gap: default_user_avatar_gap(),
      user_name_padding: default_user_name_padding(),
      user_spacing: default_user_spacing(),
      display_voice_members: Some(DisplayVoiceMembers::AlwaysSemiTransparent),
      messages_semitransparent: false,
      is_keybind_enabled: None,
      transport_mode: TransportMode::Ipc,
      software_rendering: None,
      xwayland: true,
      accent: default_accent(),
      text_color: default_text(),
      border_radius: 10.,
      ui_scale: default_ui_scale(),
      user_template: None,
      notification_template: None,
      rtt_enabled: false,
      rtt_alignment: None,
      rtt_offset_x: 0,
      rtt_offset_y: 0,
      rtt_max_lines: default_rtt_max_lines(),
      rtt_fade_seconds: default_rtt_fade_seconds(),
      rtt_font_scale: default_rtt_font_scale(),
      rtt_show_avatars: true,
      rtt_mic_device: None,
      rtt_loopback_device: None,
      rtt_model: default_rtt_model(),
      rtt_language: default_rtt_language(),
      rtt_threads: None,
      rtt_event_offset_ms: 0,
    }
  }
}

pub fn config_dir() -> Option<std::path::PathBuf> {
  Some(dirs::config_dir()?.join("orbolay"))
}

pub fn save_config(config: &Config) {
  ensure_config_dirs();

  let Some(dir) = config_dir() else { return };
  if let Ok(json) = serde_json::to_string_pretty(config) {
    std::fs::write(dir.join("config.json"), json).ok();
  }
}

pub fn load_config() -> Option<Config> {
  let dir = config_dir()?;
  let json = std::fs::read_to_string(dir.join("config.json")).ok()?;
  serde_json::from_str(&json).ok()
}

pub fn is_first_run() -> bool {
  load_config().is_none()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn old_config_gets_rtt_defaults() {
    let config: Config = serde_json::from_str("{}").unwrap();
    assert!(!config.rtt_enabled);
    assert_eq!(config.rtt_max_lines, 6);
    assert_eq!(config.rtt_fade_seconds, 8.);
    assert_eq!(config.rtt_font_scale, 1.0);
    assert!(config.rtt_show_avatars);
    assert_eq!(config.rtt_alignment, None);
    assert_eq!(config.rtt_mic_device, None);
    assert_eq!(config.rtt_loopback_device, None);
    assert_eq!(config.rtt_model, DEFAULT_RTT_MODEL);
    assert_eq!(config.rtt_language, "auto");
    assert_eq!(config.rtt_threads, None);
    assert_eq!(config.rtt_event_offset_ms, 0);
  }

  #[test]
  fn old_config_gets_user_layout_defaults() {
    let config: Config = serde_json::from_str("{}").unwrap();
    let default = Config::default();
    assert_eq!(config.user_width, default.user_width);
    assert_eq!(config.user_avatar_size, 48.);
    assert_eq!(config.user_font_size, 14.);
    assert_eq!(config.user_avatar_gap, 8.);
    assert_eq!(config.user_name_padding, 6.);
    assert_eq!(config.user_spacing, 14.);
  }
}
