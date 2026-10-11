//! A default workspace build must not pull the audio capture stack (ALSA, cpal, whisper).

use std::process::Command;

#[test]
fn default_dependency_graph_has_no_capture_stack() {
  let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
  let out = Command::new(cargo)
    .args(["tree", "--workspace", "-e", "normal", "--prefix", "none"])
    .current_dir(env!("CARGO_MANIFEST_DIR"))
    .output()
    .expect("run cargo tree");
  assert!(out.status.success(), "cargo tree failed");
  let tree = String::from_utf8_lossy(&out.stdout);
  for krate in ["alsa-sys", "cpal", "whisper-rs-sys"] {
    assert!(
      !tree
        .lines()
        .any(|l| l.split_whitespace().next() == Some(krate)),
      "{krate} is in the default workspace graph"
    );
  }
}
