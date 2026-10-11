# Orbolay (fork holetz/Orbolay) — operating notes

Lives under `.claude/` on purpose: the upstream export (`feat/rtt`) excludes `specs/` and
`.claude/`, so nothing here leaks into a PR to `SpikeHD/Orbolay`.

## Operating

- **Base / integration branch:** `develop`. `main` carries the fork's released work: it is fast-forwarded to
  `develop` (it no longer mirrors `upstream/main`; compare against `upstream/main` directly).
- **Specs store:** `cq specs` with the `git` backend (branch `quenching` on `origin`), no tracker
  cards. Never create issues or PRs on `SpikeHD/Orbolay` from automation.
- **Gate** (run from the repo root before every commit and merge):

  ```bash
  cargo fmt --all -- --check && cargo clippy --workspace -- -D warnings && cargo test --workspace
  ```

  Once the `rtt` Cargo feature exists, also run:

  ```bash
  cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt
  ```

- **Ephemeral area:** `target/quenching/` (ignored by git through `target/`).
- **Conventions:** 2-space indent (`rustfmt.toml`), `#[cfg]`-gate platform-specific code, prefer
  RPC-available features (CONTRIBUTING.md). Linux build deps: `build-essential libssl-dev
  pkg-config cmake libgtk-3-dev libclang-dev libglib2.0-dev libx11-dev`.
