use std::{
  collections::{HashMap, VecDeque},
  sync::{Arc, RwLock, atomic::AtomicBool},
};

use crate::{
  config::{Config, TransportMode},
  payloads::{Notification, SoundboardSoundPayload},
  speaking_timeline::SpeakingTimeline,
  transcript::{TranscriptLine, push_capped},
  user::{PremiumType, User},
  util::bridge::BridgeMessage,
};

/// Thread-safe shared state for background threads.
pub type SharedAppState = Arc<RwLock<AppState>>;

#[derive(Clone)]
pub struct AppHandle {
  shared: SharedAppState,
  redraw_tx: flume::Sender<()>,
}

impl AppHandle {
  pub fn new(shared: SharedAppState, redraw_tx: flume::Sender<()>) -> Self {
    Self { shared, redraw_tx }
  }

  pub fn shared(&self) -> &SharedAppState {
    &self.shared
  }

  pub fn read<T>(&self, f: impl FnOnce(&AppState) -> T) -> T {
    let state = self.shared.read().unwrap();
    f(&state)
  }

  pub fn update<T>(&self, f: impl FnOnce(&mut AppState) -> T) -> T {
    let result = {
      let mut state = self.shared.write().unwrap();
      f(&mut state)
    };
    let _ = self.redraw_tx.send(());
    result
  }

  pub fn update_if_changed(&self, f: impl FnOnce(&mut AppState) -> bool) {
    let changed = {
      let mut state = self.shared.write().unwrap();
      f(&mut state)
    };

    if changed {
      let _ = self.redraw_tx.send(());
    }
  }

  pub fn send(&self, message: BridgeMessage) {
    self.read(|state| state.send(message));
  }

  pub fn notify(&self, notification: Notification) {
    self.update(|state| state.notify(notification));
  }

  pub fn redraw(&self) {
    let _ = self.redraw_tx.send(());
  }
}

#[derive(Debug, Clone)]
pub struct AppState {
  pub config: Config,
  pub user_id: String,
  pub premium_type: PremiumType,
  pub transport_mode: TransportMode,
  pub current_channel: String,
  pub current_guild_id: String,
  pub is_open: bool,
  pub is_censor: bool, // Used in modded clients but not IPC
  pub voice_users: Vec<User>,
  pub messages: Vec<Notification>,
  pub transcript: VecDeque<TranscriptLine>,
  pub speaking: SpeakingTimeline,
  pub soundboard_cache: HashMap<String, Vec<SoundboardSoundPayload>>,

  // Name caches
  pub guild_names: HashMap<String, String>,
  pub channel_names: HashMap<String, String>,

  pub transport_sender: Option<flume::Sender<BridgeMessage>>,

  pub recording_keybind: Arc<AtomicBool>,
}

impl Default for AppState {
  fn default() -> Self {
    Self::new()
  }
}

impl AppState {
  pub fn new() -> Self {
    let mut default_guild_names: HashMap<String, String> = HashMap::new();
    default_guild_names.insert("0".into(), "Default".into());

    Self {
      config: Config::default(),
      user_id: String::new(),
      premium_type: PremiumType::None,
      transport_mode: TransportMode::Ipc,
      current_channel: String::new(),
      current_guild_id: String::new(),
      is_open: false,
      is_censor: false,
      voice_users: vec![],
      messages: vec![],
      transcript: VecDeque::new(),
      speaking: SpeakingTimeline::new(),
      soundboard_cache: HashMap::new(),

      guild_names: default_guild_names,
      channel_names: HashMap::new(),

      transport_sender: None,
      recording_keybind: Arc::new(AtomicBool::new(false)),
    }
  }

  pub fn send(&self, message: BridgeMessage) {
    if let Some(sender) = &self.transport_sender {
      sender.send(message).unwrap_or_default();
    }
  }

  pub fn notify(&mut self, notification: Notification) {
    let messages_len = self.messages.len();

    // Keep the last 3 elements
    if messages_len > 3 {
      self.messages.drain(0..messages_len - 3);
    }

    self.messages.push(notification);
  }

  /// Adds a transcript line, or replaces the line with the same id, capped at `rttMaxLines`.
  pub fn push_transcript(&mut self, line: TranscriptLine) {
    push_capped(&mut self.transcript, line, self.config.rtt_max_lines);
  }

  /// Refines the text of an existing partial line in place. Returns false when no such line exists.
  pub fn update_partial(&mut self, id: &str, text: String, is_partial: bool) -> bool {
    match self.transcript.iter_mut().find(|l| l.id == id) {
      Some(line) => {
        line.text = text;
        line.is_partial = is_partial;
        true
      }
      None => false,
    }
  }

  pub fn clear_transcript(&mut self) {
    self.transcript.clear();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::transcript::TranscriptSource;

  fn line(id: &str, text: &str, is_partial: bool) -> TranscriptLine {
    TranscriptLine {
      id: id.into(),
      speaker_id: None,
      speaker_name: "Bob".into(),
      text: text.into(),
      started_at: 0,
      ended_at: 0,
      is_partial,
      source: TranscriptSource::Loopback,
    }
  }

  #[test]
  fn push_respects_configured_cap() {
    let mut state = AppState::new();
    state.config.rtt_max_lines = 2;
    for i in 0..4 {
      state.push_transcript(line(&i.to_string(), "x", false));
    }
    assert_eq!(state.transcript.len(), 2);
    assert_eq!(state.transcript[0].id, "2");
  }

  #[test]
  fn update_partial_and_clear() {
    let mut state = AppState::new();
    state.push_transcript(line("a", "he", true));
    assert!(state.update_partial("a", "hello".into(), false));
    assert_eq!(state.transcript[0].text, "hello");
    assert!(!state.transcript[0].is_partial);
    assert!(!state.update_partial("missing", "x".into(), false));
    state.clear_transcript();
    assert!(state.transcript.is_empty());
  }
}
