---
slug: rtt-capture-windows-macos
title: RTT: loopback capture on Windows and macOS
date: 2026-10-11
verification: per-section
---

# RTT: loopback capture on Windows and macOS

<!-- ONE spec is ONE file for its whole lifecycle. Phases enrich it; they never split it.

     `specs.py new` stamps the frontmatter and `## Problem` ALONE — a captured spec is four
     lines of body, not a fourteen-heading skeleton. Every other heading below is created on
     first write by `specs.py section <slug> "<Heading>" --write`, which inserts it in the
     canonical position with the guidance comment kept here.

     THE STAGE-SCOPED EXPLICIT-NONE RULE. A heading is required — and required to carry
     `- none — <reason>` when it has nothing in it — only once ITS OWN gate is reached:

       new (capture)        `## Problem`
       ready (derived)      the nine definition sections (`## Problem` .. `## Risks`)
                            AND `## Tasks`
       ready (warn only)    `## Overview` non-empty, `## Handoff` non-empty
       promote -> archive/  `## Outcome`

     `ready` is a DERIVED STAGE, not a folder: a spec lives in `plans/` for its whole active
     life, and filling those ten sections is what makes it ready. Nothing refuses on that
     gate — it is a floor `execute` reports against, and the human's go-ahead is the
     `approved:` frontmatter record, asked for inline.

     Before its gate, a heading's absence is NOT an omission — it is a not-yet. After its
     gate, three rules decide whether a section counts as filled:

       1. `- none — <reason>` counts as filled. An omission and a null are different facts.
       2. A heading present with an EMPTY body is malformed and refuses. It is neither an
          answer nor a not-yet.
       3. An absent heading before its gate is legal.

     Headings are a PARSED contract — canonical English, exactly as written here. Body prose
     follows the repo's language. A heading outside this set is a stray and validate flags it.
     *(`standards/agents/communication.md` owns that language rule for a repo whose bundle has
     one. This template states it self-contained rather than citing it: `/specs:align` is native
     and installs here into repos that never adopted the bundle, where that path resolves to
     nothing.)*

     MOMENT. Each section belongs to one of three moments on the spec's timeline: `decision`
     (the human, deciding whether to build), `build` (the executor, in step 4 of
     `/specs:execute`), `close` (`/specs:conclude`, at archive time). `## Discoveries` belongs
     to none of them — captured indiscriminately while building, resolved later by
     `/specs:develop`'s triage sweep on its own schedule. An orchestrator sends an executor
     exactly the `build` set; that is what lets one file serve every moment without bloating
     agent context. -->

## Overview

This spec makes captions work beyond Linux. **Proposal** adds the Windows and macOS versions of
"capture what Discord plays", preferring each OS's ability to record a single app so game sound
isn't captioned. The rest of the pipeline is untouched. The main risks are testing without local
hardware and macOS permissions, covered in **Risks**.

## Problem

Orbolay ships for Windows, macOS and Linux, and most of its users are on Windows. An upstream PR
that only works on Linux would be a hard sell. Loopback capture is the only platform-specific
part of RTT.

## Proposal

- `LoopbackSource` for Windows via WASAPI loopback on the default render device (through `cpal`'s
  loopback support if adequate, else the `windows` crate already in the workspace). Use process
  loopback (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`, Win10 2004+) to capture only
  Discord's process tree when available.
- `LoopbackSource` for macOS via ScreenCaptureKit audio capture (macOS 13+), filtered to the
  Discord app. Handle the Screen Recording permission prompt with a clear status in the RTT settings.
- Builds of `--features rtt` verified on all three CI targets (whisper.cpp with MSVC and Apple clang).
- Platform-specific settings `#[cfg]`-gated, as CONTRIBUTING.md requires.

## Out of Scope

- macOS < 13 (no ScreenCaptureKit audio). RTT shows "mic only" there.
- Virtual audio driver approaches (BlackHole, VB-Cable). Users can still select such a device
  manually via `rttLoopbackDevice`.

## Impact

### Product code this spec expects to touch

- `crates/orbolay-rtt/src/audio/loopback_windows.rs` (new), `loopback_macos.rs` (new), `audio/mod.rs`
- `crates/orbolay-rtt/Cargo.toml`: target-specific deps
- `crates/orbolay-ui/src/config.rs`: permission/status display (macOS)
- `.github/workflows/check.yml`, `build.yml`: `--features rtt` on all OS
- `README.md`: platform notes for captions

## Validation

- `cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt` passes on Windows, macOS and Linux CI runners.
- Manual on Windows 11: Discord call captions work, and game audio isn't transcribed when process
  loopback is available.
- Manual on macOS 14 (Apple silicon): permission prompt appears once, then captions work.

## Design

- Prefer process-scoped capture on both OSes (Windows process loopback, ScreenCaptureKit app filter).
  It solves the "game audio becomes captions" problem that Linux may only solve partially.
- Fall back to whole-system loopback on older Windows, and to mic-only with an explanatory status
  on old macOS.

## Alternatives Considered

- Requiring a virtual audio device on all platforms. Rejected: poor UX and a support burden for
  upstream.

## Open Decisions

- Whether `cpal`'s WASAPI loopback can target a process, or raw `windows` crate calls are needed.
  Decided by spike in task 1.1.
- Which Rust ScreenCaptureKit binding to use (`screencapturekit` crate vs. hand-written
  `objc2` bindings). Decided by spike in task 2.1, based on maintenance status and build impact.

## Risks

- No local Windows/macOS hardware for manual tests. Mitigation: CI builds, plus a call for testers
  in the upstream issue. A VM is fine for Windows.
- macOS notarization/entitlements for screen capture in the bundled `.app`. Mitigation: check
  `build.yml` bundling, and add the usage description key to Info.plist via cargo-bundle metadata.

## Handoff

- Base branch `develop`, after `rtt-audio-capture` is merged (it can run in parallel with
  `rtt-stt-engine` / `rtt-speaker-attribution`).
- Implement the same `AudioSource` trait. Output is 16 kHz mono after the shared resampler.

## Tasks

### 1. Windows

- [ ] 1.1 Spike WASAPI process loopback (cpal vs windows crate) and resolve the open decision
- [ ] 1.2 Implement `LoopbackSource` for Windows with process-loopback and system-loopback fallback
      files: crates/orbolay-rtt/src/audio/loopback_windows.rs (new), crates/orbolay-rtt/src/audio/mod.rs, crates/orbolay-rtt/Cargo.toml

### 2. macOS

- [ ] 2.1 Spike the ScreenCaptureKit binding and resolve the open decision
- [ ] 2.2 Implement `LoopbackSource` for macOS filtered to Discord, with permission status surfaced in settings
      files: crates/orbolay-rtt/src/audio/loopback_macos.rs (new), crates/orbolay-rtt/src/audio/mod.rs, crates/orbolay-rtt/Cargo.toml, crates/orbolay-ui/src/config.rs, Cargo.toml

### 3. CI and docs

- [ ] 3.1 Build `--features rtt` on all three OS in CI
      files: .github/workflows/check.yml, .github/workflows/build.yml
      verify: cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt
- [ ] 3.2 README platform notes for captions
