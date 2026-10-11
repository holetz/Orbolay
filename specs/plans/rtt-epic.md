---
slug: rtt-epic
title: Epic: real-time transcription overlay with speaker labels
date: 2026-10-11
verification: end-of-plan
---

# Epic: real-time transcription overlay with speaker labels

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

This is the umbrella spec for live captions in Orbolay. **Problem** and **Proposal** explain what we
want and break it into five child specs that each build one layer: panel, audio, speech-to-text,
speaker labels, other platforms. **Design** explains the parts that cut across all of them: how the
fork's branches are arranged so specs never leak upstream, the double on/off switch, and the
audio-to-caption pipeline. **Tasks** holds only the work no child owns: asking upstream first, and
packaging the finished result as one pull request. **Open Decisions** and **Risks** are mostly about
upstream's answer and the performance cost while gaming.

## Problem

Orbolay shows *who* is talking in a Discord voice channel, but not *what* they say. Hard-of-hearing
players, players who are deafened or in noisy rooms, and anyone who missed a sentence mid-game have
no in-overlay way to follow the conversation. Discord's own overlay doesn't offer live captions either.

The data Orbolay already receives (per-user `SPEAKING_START`/`SPEAKING_STOP` events, plus names and
avatars) makes speaker-labelled captions unusually cheap to attribute. The missing pieces are audio
capture, speech-to-text and a caption panel.

This epic coordinates the child specs and owns the two things none of them can: agreeing the
direction with upstream before the bulk of the work, and delivering everything back to
`SpikeHD/Orbolay` as **one** pull request.

## Proposal

- An opt-in **real-time transcription (RTT)** feature that renders a scrolling caption panel in the
  overlay. Each line is labelled with the Discord display name (and avatar/colour) of the speaker.
- Delivered as five child specs, built in this order (each merges into `develop`):
  1. `rtt-transcript-panel`: transcript data model in `AppState`, config keys, and the caption
     panel UI, driven by a fake feed so the UI can ship and be reviewed first.
  2. `rtt-audio-capture`: new `crates/orbolay-rtt` crate with an `AudioSource` abstraction,
     microphone capture and **Linux** loopback (PipeWire/PulseAudio monitor).
  3. `rtt-stt-engine`: local speech-to-text (whisper.cpp via `whisper-rs`) with VAD-driven
     chunking, behind an `SttEngine` trait so a cloud backend can be added later.
  4. `rtt-speaker-attribution`: a speaking timeline fed by the existing IPC and websocket
     handlers, used to label each transcript segment with a user.
  5. `rtt-capture-windows-macos`: WASAPI loopback (Windows) and ScreenCaptureKit (macOS).
- Everything sits behind a Cargo feature `rtt` (off by default) **and** a runtime config toggle
  (off by default). A default build of Orbolay is unchanged.
- Integration: one squashed commit on a `feat/rtt` branch cut from `upstream/main`, containing
  only product code. No `specs/`, no `.claude/` and no fork-only `.gitignore` lines.

## Out of Scope

- Per-user separate audio streams (hooking the Discord client or its voice gateway). It's fragile,
  breaks Discord's ToS and conflicts with upstream's "RPC-only features" guideline.
- Cloud STT backends. The `SttEngine` trait leaves room, but none ships in the upstream PR,
  because of privacy/consent and API-key handling. A follow-up spec can add one.
- Translation, transcript export/logging to disk and searchable history. Captions are ephemeral
  by design (privacy), and upstream review is easier without persistence.
- ML-based diarization (pyannote etc.). Discord's speaking events already give us identities.

## Impact

### Product code this spec expects to touch

- `Cargo.toml`: workspace member `crates/orbolay-rtt`, `rtt` feature on the root package
- `crates/orbolay-rtt/` (new): capture, STT, attribution pipeline
- `crates/orbolay-core/src/{app_state.rs,config.rs}`: transcript model and RTT config keys
- `crates/orbolay-transport/src/{ipc,websocket}/`: speaking events also feed the RTT timeline
- `crates/orbolay-ui/src/components/`: new caption panel; configurator entries
- `src/main.rs`: wiring the panel and starting the RTT thread when enabled
- `README.md`: a short "Live captions (experimental)" section
- `.github/workflows/check.yml`: a `--features rtt` check job (if upstream wants it)

## Validation

- `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo build` passes with the feature **off**, and the release binary size is within
  1% of `upstream/main`.
- `cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt` passes with the feature **on**.
- Manual end-to-end on Linux (official client, IPC mode): join a voice channel with 2+ people,
  enable captions; lines appear within ~3 s of speech, labelled with the correct names; your own
  speech is labelled with your name; panel scrolls and fades as configured.
- Manual end-to-end in websocket mode (Vencord/shelter bridge): same result.
- `git diff upstream/main...feat/rtt --stat` shows no path under `specs/` or `.claude/`, and
  `git rev-list --count upstream/main..feat/rtt` is `1`.

## Design

**Branch model (fork `holetz/Orbolay`).**
- `main` mirrors `upstream/main` and is never committed to directly; sync with
  `git fetch upstream && git checkout main && git merge --ff-only upstream/main`.
- `develop` is the declared `integrationBranch` (`.claude/quenching.json`). It holds `specs/`, `.claude/`
  and the merged work of every child spec. It periodically merges `main` to stay current.
- Each child spec builds on `plan/<slug>` cut from `develop` and merges back via `/specs:conclude`.
- Export: `feat/rtt` is cut from `upstream/main`. The product diff
  `git diff upstream/main develop -- . ':!specs' ':!.claude' ':!.gitignore'` is applied and
  committed once, with the `.gitignore` hunk dropped by hand if it's the only change there. Then
  push to `origin/feat/rtt` and open the PR against `SpikeHD/Orbolay:main`.
  Why: specs stay versioned in the fork without ever leaking into the upstream diff, and upstream
  reviews one coherent change.

**Feature gating, two layers.** A Cargo feature `rtt` keeps the heavy deps (whisper.cpp, audio
backends) out of default builds and CI, preserving Orbolay's "quick, small" promise. A config flag
`rttEnabled` (default `false`) means a build with the feature still does nothing until the user opts in.

**Crate boundary.** All audio/STT/attribution code lives in `crates/orbolay-rtt`, which depends on
`orbolay-core` only. It talks to the rest of the app the same way transports do: it receives an
`AppHandle` and writes into `AppState` via `update_if_changed`. The UI only reads
`AppState.transcript`, so the panel compiles without the feature (it shows nothing).

**Pipeline.** `AudioSource` (mic, loopback) → resample to 16 kHz mono → VAD segmenter →
`SttEngine` worker thread → segments with timestamps → attribution against the speaking timeline
→ `TranscriptLine`s in `AppState` → `CaptionPanel`. Threads communicate over `flume` channels, as
the existing codebase does.

**Two-stream trick.** Mic and loopback are transcribed as separate streams. Mic segments are always
the local user, and loopback segments are attributed from speaking events. This removes the most
common attribution ambiguity (you vs. others) for free.

## Alternatives Considered

- **Long-lived fork, no upstreaming.** Rejected as the *default*, but kept as the fallback if
  upstream declines (GPL-3.0 allows it). Designing for upstream costs little extra (feature flag,
  clean crate) and keeps the fork easy to rebase either way.
- **One spec for the whole feature.** Rejected: capture, STT and attribution have independent
  risks and verification. Splitting lets the UI ship first against fake data and isolates the
  platform-specific capture work.
- **External sidecar process (e.g. Python + faster-whisper) feeding Orbolay over websocket.**
  Rejected for upstream: a second runtime contradicts the single native binary. It may still be
  useful as a quick prototype to validate latency and UX before task 1.1.

## Open Decisions

- **Will upstream accept RTT, and in what shape?** Decided by the maintainer's answer on the
  upstream issue (task 1.1). Outcomes: accept as optional feature → proceed as specified; accept
  with changes → amend specs via `/specs:develop`; decline → keep `develop` as the fork's product
  branch and skip section 3.
- **Should the PR add a CI job for `--features rtt`?** Asked in the same issue; whisper.cpp needs
  cmake/clang in CI, which the Linux job already installs.
- **Model distribution:** download on first enable into the config dir vs. user-supplied path.
  This is settled in `rtt-stt-engine` and needs agreement with upstream on network access at
  runtime.

## Risks

- **Upstream rejects or ignores the proposal.** Mitigation: open the issue *before* building
  section 2+. The fork is useful on its own regardless.
- **Upstream drifts during development (it depends on a moving Freya git branch).** Mitigation:
  merge `main` into `develop` at the start of every child spec. Keep the RTT surface in a separate
  crate so conflicts concentrate in a few wiring lines.
- **CPU/GPU cost while gaming makes the feature unusable.** Mitigation: small default model
  (`base`/`small` quantized), VAD so silence is never transcribed, a configurable thread count.
  Measured in `rtt-stt-engine` validation.
- **Privacy/consent: transcribing other people.** Mitigation: off by default, local-only
  processing, nothing written to disk, and a clear README note that the user is responsible for
  consent.
- **LLM-assisted code rejected as "slop" (CONTRIBUTING.md item 6).** Mitigation: every task is
  reviewed line by line by the human before merge into `develop`. `cargo fmt` and `clippy` stay
  clean. The final PR description explains design choices in the human's own words.

## Handoff

- Fork: https://github.com/holetz/Orbolay; upstream: https://github.com/SpikeHD/Orbolay (GPL-3.0).
- Local clone: `~/Projects/Orbolay`, remotes `origin` (fork) and `upstream`.
- Base for all child specs is `develop`. Never commit to `main`.
- Upstream conventions (CONTRIBUTING.md): open an issue first; prefer features available over RPC;
  `#[cfg]`-gate platform-specific settings; `cargo fmt` + `cargo clippy` before every commit; 2-space
  indent per `rustfmt.toml`.
- Child specs: `rtt-transcript-panel` → `rtt-audio-capture` → `rtt-stt-engine` →
  `rtt-speaker-attribution` → `rtt-capture-windows-macos`.

## Tasks

### 1. Upstream alignment

- [ ] 1.1 Open a feature-request issue on SpikeHD/Orbolay describing RTT as an optional, feature-flagged, local-only addition (with the panel mock-up from `rtt-transcript-panel`), and ask about the CI job and model download
      verify: issue URL recorded in ## Discoveries
- [ ] 1.2 Record the maintainer's answer and amend child specs if the requested shape differs
      verify: answer summarized in ## Discoveries; child specs updated via /specs:develop

### 2. Build (tracked in child specs)

- [ ] 2.1 `rtt-transcript-panel` concluded and merged into develop
- [ ] 2.2 `rtt-audio-capture` concluded and merged into develop
- [ ] 2.3 `rtt-stt-engine` concluded and merged into develop
- [ ] 2.4 `rtt-speaker-attribution` concluded and merged into develop
- [ ] 2.5 `rtt-capture-windows-macos` concluded and merged into develop

### 3. Integration into upstream

- [ ] 3.1 Sync: fast-forward main to upstream/main, merge main into develop, resolve conflicts
      verify: cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt
- [ ] 3.2 Run the full ## Validation matrix (feature off/on, IPC and websocket, Linux; Windows/macOS smoke tests)
- [ ] 3.3 Export a single-commit `feat/rtt` branch from upstream/main containing only product code (excludes specs/, .claude/, fork-only .gitignore lines)
      verify: git rev-list --count upstream/main..feat/rtt prints 1 and git diff --stat upstream/main feat/rtt lists no specs/ or .claude/ path
- [ ] 3.4 Write the PR description (motivation, design, feature flag, privacy, how to test), push feat/rtt to origin and open the PR to SpikeHD/Orbolay:main referencing the issue from 1.1
- [ ] 3.5 Address review feedback on `develop`, re-export `feat/rtt` and force-push it, until merged or declined
