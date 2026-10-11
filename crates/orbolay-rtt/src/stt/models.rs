//! Speech models on disk and the explicit, user-triggered download of the known ones.
//!
//! Nothing here runs on its own: [`download`] is only called from a button in the settings.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
  Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};

use crate::pipeline::model_path;

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Models offered for download, as names accepted by `rttModel`.
pub const KNOWN_MODELS: &[&str] = &[
  "ggml-tiny",
  "ggml-base",
  "ggml-small",
  "ggml-tiny.en",
  "ggml-base.en",
  "ggml-small.en",
];

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounds the whole body (the largest model is ~490 MB); a stall is caught sooner by [`STALL_TIMEOUT`].
const BODY_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// ureq has no per-read timeout, so the body is aborted when no bytes arrive for this long.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// SHA-256 of each known model, from the `lfs.oid` of
/// `https://huggingface.co/api/models/ggerganov/whisper.cpp/tree/main`.
fn expected_sha256(model: &str) -> Option<&'static str> {
  match model.strip_suffix(".bin").unwrap_or(model) {
    "ggml-tiny" => Some("be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"),
    "ggml-base" => Some("60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"),
    "ggml-small" => Some("1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"),
    "ggml-tiny.en" => Some("921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f"),
    "ggml-base.en" => Some("a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"),
    "ggml-small.en" => Some("c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d"),
    _ => None,
  }
}

/// Streams `path` through SHA-256 and compares it with `expected` (hex).
fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
  let mut file = File::open(path).map_err(|e| e.to_string())?;
  let mut hasher = Sha256::new();
  let mut buf = [0u8; 64 * 1024];
  loop {
    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
    if n == 0 {
      break;
    }
    hasher.update(&buf[..n]);
  }
  let actual: String = hasher
    .finalize()
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
  if actual.eq_ignore_ascii_case(expected) {
    Ok(())
  } else {
    Err(format!(
      "checksum mismatch: expected {expected}, got {actual}"
    ))
  }
}

/// Whether `model` (with or without `.bin`) is one of [`KNOWN_MODELS`].
pub fn is_known(model: &str) -> bool {
  KNOWN_MODELS.contains(&model.strip_suffix(".bin").unwrap_or(model))
}

/// Where `model` lives (or will live) on disk.
pub fn local_path(model: &str) -> Option<PathBuf> {
  model_path(model)
}

pub fn is_installed(model: &str) -> bool {
  local_path(model).is_some_and(|p| p.is_file())
}

pub fn download_url(model: &str) -> String {
  let file = model.strip_suffix(".bin").unwrap_or(model);
  format!("{BASE_URL}/{file}.bin")
}

/// The models that can be selected, downloaded and resolved: exactly [`KNOWN_MODELS`].
pub fn available_models() -> Vec<String> {
  KNOWN_MODELS.iter().map(|s| s.to_string()).collect()
}

/// Downloads a known model into the models directory, through a temporary file so a failed
/// transfer never leaves a truncated model behind. Blocking; returns the final path.
pub fn download(model: &str) -> Result<PathBuf, String> {
  if !is_known(model) {
    return Err(format!(
      "unknown model {model:?}: only {} can be downloaded",
      KNOWN_MODELS.join(", ")
    ));
  }
  let expected = expected_sha256(model).ok_or_else(|| format!("no pinned checksum for {model}"))?;
  let target = local_path(model).ok_or("no config directory")?;
  let dir = target.parent().ok_or("invalid model path")?;
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let partial = target.with_extension("bin.part");
  let result = fetch(&download_url(model), &partial, STALL_TIMEOUT)
    .and_then(|()| verify_sha256(&partial, expected).map_err(|e| format!("{model}: {e}")));
  match result {
    Ok(()) => {
      fs::rename(&partial, &target).map_err(|e| e.to_string())?;
      Ok(target)
    }
    Err(e) => {
      let _ = fs::remove_file(&partial);
      Err(e)
    }
  }
}

/// ureq has no per-read timeout, so a connector clamps every socket read to the stall window plus
/// a margin (so the stall deadline in `fetch` reports first).
/// A stalled read then errors out, ending the reader thread and dropping its socket.
#[derive(Debug)]
struct PerReadTimeout {
  inner: DefaultConnector,
  read: Duration,
}

impl Connector for PerReadTimeout {
  type Out = Box<dyn Transport>;

  fn connect(
    &self,
    details: &ConnectionDetails,
    chained: Option<()>,
  ) -> Result<Option<Self::Out>, ureq::Error> {
    Ok(self.inner.connect(details, chained)?.map(|inner| {
      Box::new(ClampedRead {
        inner,
        read: self.read,
      }) as Box<dyn Transport>
    }))
  }
}

struct ClampedRead {
  inner: Box<dyn Transport>,
  read: Duration,
}

impl fmt::Debug for ClampedRead {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ClampedRead").finish_non_exhaustive()
  }
}

impl Transport for ClampedRead {
  fn buffers(&mut self) -> &mut dyn Buffers {
    self.inner.buffers()
  }

  fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
    self.inner.transmit_output(amount, timeout)
  }

  fn await_input(&mut self, mut timeout: NextTimeout) -> Result<bool, ureq::Error> {
    timeout.after = timeout
      .after
      .min(ureq::unversioned::transport::time::Duration::Exact(
        self.read,
      ));
    self.inner.await_input(timeout)
  }

  fn is_open(&mut self) -> bool {
    self.inner.is_open()
  }
}

/// Downloads `url` into `dest`, aborting when the body goes `stall` without a byte.
///
/// The blocking read happens in a worker thread, so the deadline holds even when that read never
/// returns; the clamped per-read timeout ends the worker, dropping its socket, within one stall window.
fn fetch(url: &str, dest: &Path, stall: Duration) -> Result<(), String> {
  let config = ureq::Agent::config_builder()
    .timeout_connect(Some(CONNECT_TIMEOUT))
    .timeout_recv_response(Some(RESPONSE_TIMEOUT))
    .timeout_recv_body(Some(BODY_TIMEOUT))
    .build();
  let agent = ureq::Agent::with_parts(
    config,
    PerReadTimeout {
      inner: DefaultConnector::default(),
      read: stall + Duration::from_secs(1),
    },
    DefaultResolver::default(),
  );
  let response = agent.get(url).call().map_err(|e| e.to_string())?;
  let mut reader = response.into_body().into_reader();
  let mut file = File::create(dest).map_err(|e| e.to_string())?;
  let (tx, rx) = mpsc::sync_channel::<io::Result<Vec<u8>>>(4);
  thread::spawn(move || {
    let mut buf = [0u8; 64 * 1024];
    loop {
      let chunk = match reader.read(&mut buf) {
        Ok(0) => break,
        Ok(n) => Ok(buf[..n].to_vec()),
        Err(e) => Err(e),
      };
      let failed = chunk.is_err();
      if tx.send(chunk).is_err() || failed {
        break;
      }
    }
  });
  loop {
    match rx.recv_timeout(stall) {
      Ok(Ok(chunk)) => file.write_all(&chunk).map_err(|e| e.to_string())?,
      Ok(Err(e)) => return Err(e.to_string()),
      Err(mpsc::RecvTimeoutError::Timeout) => {
        return Err(format!(
          "download stalled: no data for {}s",
          stall.as_secs()
        ));
      }
      Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn url_names_the_bin_file() {
    assert_eq!(
      download_url("ggml-base"),
      format!("{BASE_URL}/ggml-base.bin")
    );
    assert_eq!(
      download_url("ggml-base.bin"),
      format!("{BASE_URL}/ggml-base.bin")
    );
  }

  #[test]
  fn known_models_are_listed_without_duplicates() {
    let names = available_models();
    for known in KNOWN_MODELS {
      assert_eq!(names.iter().filter(|n| n == known).count(), 1);
    }
  }

  #[test]
  fn every_known_model_has_a_pinned_checksum() {
    for known in KNOWN_MODELS {
      let sum = expected_sha256(known).unwrap_or_else(|| panic!("no pin for {known}"));
      assert_eq!(sum.len(), 64);
    }
    assert!(expected_sha256("not-a-model").is_none());
  }

  #[test]
  fn corrupt_download_is_rejected() {
    let path = std::env::temp_dir().join(format!("orbolay-model-test-{}.bin", std::process::id()));
    fs::write(&path, b"<html>404 but 200</html>").unwrap();
    let pinned = expected_sha256("ggml-tiny").unwrap();
    let err = verify_sha256(&path, pinned).unwrap_err();
    assert!(err.contains("checksum mismatch"), "{err}");
    // The digest of the exact bytes is accepted (SHA-256 of "abc").
    fs::write(&path, b"abc").unwrap();
    verify_sha256(
      &path,
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
  }

  const EVIL: &[&str] = &[
    "../../evil",
    "..",
    "a/b",
    "a\\b",
    "/nonexistent-dir/evil",
    "",
  ];

  #[test]
  fn traversal_names_never_build_a_path_or_url() {
    for name in EVIL {
      assert_eq!(local_path(name), None, "path built for {name:?}");
      assert!(!is_installed(name), "{name:?} reported installed");
    }
  }

  #[test]
  fn download_refuses_unknown_names_before_any_io() {
    for name in EVIL.iter().chain(&["ggml-unknown", "ggml-tiny/../x"]) {
      let err = download(name).unwrap_err();
      assert!(err.contains("unknown model"), "{name:?}: {err}");
    }
  }

  #[test]
  fn missing_model_is_not_installed() {
    assert!(!is_installed("definitely-not-a-model-xyz"));
  }

  #[test]
  fn stalled_download_aborts_instead_of_hanging() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
      let (mut conn, _) = listener.accept().unwrap();
      let mut req = [0u8; 1024];
      let _ = conn.read(&mut req);
      conn
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\npartial")
        .unwrap();
      // Stall with the connection open until the test is over.
      thread::sleep(Duration::from_secs(8));
    });
    let dest = std::env::temp_dir().join(format!("orbolay-stall-test-{}.bin", std::process::id()));
    let (done_tx, done_rx) = mpsc::channel();
    let guard_dest = dest.clone();
    thread::spawn(move || {
      let _ = done_tx.send(fetch(&url, &guard_dest, Duration::from_millis(500)));
    });
    let result = done_rx
      .recv_timeout(Duration::from_secs(5))
      .expect("fetch still blocked 5s after the body stalled");
    let err = result.unwrap_err();
    assert!(err.contains("stalled"), "{err}");
    let _ = fs::remove_file(&dest);
    drop(server);
  }

  #[test]
  fn stalled_download_releases_its_socket() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
    let (closed_tx, closed_rx) = mpsc::channel();
    let server = thread::spawn(move || {
      let (mut conn, _) = listener.accept().unwrap();
      let mut req = [0u8; 1024];
      let _ = conn.read(&mut req);
      conn
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\npartial")
        .unwrap();
      // Wait for the client to close its end (EOF or reset), never sending more.
      conn
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
      let closed = match conn.read(&mut req) {
        Ok(n) => n == 0,
        Err(e) => !matches!(
          e.kind(),
          io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ),
      };
      let _ = closed_tx.send(closed);
    });
    let dest =
      std::env::temp_dir().join(format!("orbolay-release-test-{}.bin", std::process::id()));
    let err = fetch(&url, &dest, Duration::from_millis(500)).unwrap_err();
    assert!(err.contains("stalled"), "{err}");
    let released = closed_rx.recv_timeout(Duration::from_secs(4));
    assert!(
      matches!(released, Ok(true)),
      "client socket still open 4s after the stall deadline: {released:?}"
    );
    let _ = fs::remove_file(&dest);
    let _ = server.join();
  }
}
