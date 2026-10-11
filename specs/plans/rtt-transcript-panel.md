---
slug: rtt-transcript-panel
title: RTT: transcript model, config and caption panel
date: 2026-10-11
verification: per-section
---

# RTT: transcript model, config and caption panel

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

The first visible piece of the feature. **Proposal** defines the caption data model, config keys and
panel. **Design** explains why the model sits in the shared core crate, and why the panel copies
the existing notification panel's patterns. A fake "demo" feed lets the panel be built and shown to
upstream before any audio work exists. The later specs only have to push lines into the model this
spec defines.

## Problem

Before any audio or speech-to-text exists, the overlay needs somewhere to show captions and a data
model the rest of the RTT pipeline writes into. Building the UI first against fake data lets us
settle the look, show upstream a concrete mock-up in the proposal issue, and develop the backend
specs against a stable target.

## Proposal

- `TranscriptLine` type in `orbolay-core`: `id`, `speaker_id: Option<String>`, `speaker_name`,
  `text`, `started_at`, `ended_at`, `is_partial`, `source: Mic | Loopback`.
- `AppState.transcript: VecDeque<TranscriptLine>`, capped at a config-defined length, plus helper
  methods `push_transcript`, `update_partial` and `clear_transcript`.
- Config keys (camelCase, all defaulted): `rttEnabled` (false), `rttAlignment` (bottom-center),
  `rttOffsetX`/`rttOffsetY`, `rttMaxLines` (6), `rttFadeSeconds` (8), `rttFontScale` (1.0),
  `rttShowAvatars` (true).
- `CaptionPanel` Freya component: vertical stack of the latest lines, speaker name in bold with a
  stable per-user colour (or avatar), partial lines in reduced opacity, lines fading out after
  `rttFadeSeconds` when the overlay is closed, auto-scrolled to the newest.
- Configurator entries for the keys above (the RTT group only shows when built with `rtt`).
- Dev-only fake feed (`--rtt-demo` CLI flag under the `rtt` feature) that pushes scripted lines
  for two speakers, for screenshots and UI iteration.

## Out of Scope

- Any real audio, STT or attribution. Those are the next three specs.
- User-customizable HTML template for captions (Orbolay supports templates for users and
  notifications). It's a possible follow-up once upstream accepts the feature.

## Impact

### Product code this spec expects to touch

- `crates/orbolay-core/src/transcript.rs` (new), `crates/orbolay-core/src/lib.rs`
- `crates/orbolay-core/src/app_state.rs`: `transcript` field and helpers
- `crates/orbolay-core/src/config.rs`: RTT keys and defaults
- `crates/orbolay-ui/src/components/caption_panel.rs` (new), `components/mod.rs`
- `crates/orbolay-ui/src/config.rs` and `configurator/`: RTT settings group
- `src/main.rs`: render `CaptionPanel`; `--rtt-demo` flag
- `Cargo.toml`: declare the `rtt` feature (empty for now)

## Validation

- `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo build` and `cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt` both pass.
- Unit tests in `orbolay-core` for the transcript buffer: cap enforced, partial line replaced in place
  by its final version, clear empties.
- `cargo run --features rtt -- --rtt-demo` shows scrolling, labelled, fading captions. Screenshot
  attached to the upstream issue.
- An existing config file without RTT keys still loads (serde defaults).

## Design

- Model lives in `orbolay-core` (not `orbolay-rtt`), so UI and transports can use it without the
  heavy feature. The `rtt` feature only gates the producer side.
- Use `VecDeque` with a cap rather than unbounded `Vec`. Captions are ephemeral, and memory must not
  grow during a long session.
- Partial lines (`is_partial = true`) are keyed by `id` and replaced in place, so streaming STT can
  refine text without the panel jumping.
- Speaker colour: a hash of `speaker_id` into a fixed palette that contrasts with the theme
  accent, so a speaker's colour is stable across sessions without any stored state.
- Follow `MessagesSection`'s pattern (`CornerAlignment`, `UiScale`, semi-transparency) for
  consistency with existing UI and easier review.

## Alternatives Considered

- Reusing the notification `MessagesSection` for captions. Rejected: notifications are discrete,
  actionable and long-lived, while captions are a continuous stream with partial updates. Mixing them
  would complicate both.

## Open Decisions

- Default position: bottom-center vs. near the voice user list. Settle with screenshots in the
  upstream issue. The config allows either.

## Risks

- Freya API churn (Orbolay pins a Freya git branch). Mitigation: mimic existing components
  closely, and keep the component small.
- Redraw pressure from frequent partial updates. Mitigation: use `update_if_changed`, and throttle
  partial updates to about 5/s in the producer (documented for `rtt-stt-engine`).

## Handoff

- Base branch `develop`. Read `crates/orbolay-ui/src/components/messages_section.rs` first. It's the
  closest pattern (alignment, offsets, scale, opacity).
- Config keys are camelCase via `#[serde(rename_all = "camelCase")]`, and every new key needs a
  `#[serde(default...)]`.
- Indent is 2 spaces (`rustfmt.toml`).

## Tasks

### 1. Core model

- [ ] 1.1 Add `TranscriptLine` and `TranscriptSource` in a new `transcript` module with unit tests for the capped buffer
      files: crates/orbolay-core/src/transcript.rs (new), crates/orbolay-core/src/lib.rs
      verify: cargo test -p orbolay-core
- [ ] 1.2 Add `transcript` to `AppState` with push / update_partial / clear helpers
      files: crates/orbolay-core/src/app_state.rs
      pattern: crates/orbolay-core/src/app_state.rs (notify / messages handling)
      verify: cargo test -p orbolay-core
- [ ] 1.3 Add RTT config keys with serde defaults
      files: crates/orbolay-core/src/config.rs
      verify: cargo test -p orbolay-core

### 2. Caption panel

- [ ] 2.1 Implement `CaptionPanel` component (alignment, offsets, scale, per-speaker colour, partial opacity, fade)
      files: crates/orbolay-ui/src/components/caption_panel.rs (new), crates/orbolay-ui/src/components/mod.rs
      pattern: crates/orbolay-ui/src/components/messages_section.rs
- [ ] 2.2 Render the panel from `src/main.rs` when `rttEnabled`
      files: src/main.rs
- [ ] 2.3 Add the RTT settings group to the configurator, shown only with the `rtt` feature
      files: crates/orbolay-ui/src/config.rs, crates/orbolay-ui/src/configurator/mod.rs, Cargo.toml, crates/orbolay-ui/Cargo.toml

### 3. Demo feed

- [ ] 3.1 Add `--rtt-demo` flag (feature `rtt`) that streams scripted two-speaker lines including partial updates
      files: src/main.rs
      verify: cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt
- [ ] 3.2 Capture a screenshot / short GIF of the demo for the upstream issue (epic task 1.1)
