---
slug: rtt-stt-engine
title: RTT: local speech-to-text with VAD chunking
date: 2026-10-11
verification: per-section
---

# RTT: local speech-to-text with VAD chunking

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

This is the "ears to text" step. **Proposal** puts speech recognition behind a small interface, with
whisper.cpp as the local implementation. A voice-activity detector cuts audio into utterances, so
silence costs nothing. **Design** explains how a non-streaming model is made to look live: show a
provisional line, then replace it when the sentence ends. After this spec, captions work end to end
but are labelled only "you" or "Unknown". The next spec adds real names.

## Problem

With 16 kHz mono audio streams available, we need to turn speech into text quickly enough for live
captions (target: first text within about 2–3 s of an utterance starting). It also has to run on the
same machine as a game without hurting frame rate, and stay local so no voice data leaves the
user's computer.

## Proposal

- `SttEngine` trait: `transcribe(segment: &AudioSegment) -> Vec<TextSegment>` with
  per-segment start/end timestamps on the pipeline clock.
- `WhisperEngine` implementation via `whisper-rs` (whisper.cpp), default model `ggml-base`
  (multilingual, quantized) with `rttModel` and `rttLanguage` (auto or ISO code) config keys.
- VAD segmenter (Silero VAD through `voice_activity_detector`, or WebRTC VAD) that cuts each source
  stream into utterances: max 8 s, emitting a provisional chunk every ~1.5 s for partial captions.
- Worker thread with a bounded queue. If it falls behind, drop or merge the oldest pending
  provisional chunks and never final ones.
- Model management: look for the model in the Orbolay config dir. If it's missing, the RTT settings
  show a "Download model (~X MB)" action rather than downloading silently.
- Pipeline wiring: sources → VAD → engine → `AppState.transcript` (unattributed at this stage:
  mic → local user, loopback → "Unknown").

## Out of Scope

- Cloud engines (Deepgram, Azure, …). The trait allows them later, but they're not shipped here.
- GPU acceleration flags (CUDA/Metal/Vulkan) beyond what `whisper-rs` enables by default. It can be
  exposed later as sub-features `rtt-cuda`, etc.

## Impact

### Product code this spec expects to touch

- `crates/orbolay-rtt/src/stt/{mod,whisper,vad,models}.rs` (new)
- `crates/orbolay-rtt/src/pipeline.rs` (new): thread orchestration, start/stop on `rttEnabled`
- `crates/orbolay-core/src/config.rs`: `rttModel`, `rttLanguage`, `rttThreads`
- `crates/orbolay-ui/src/config.rs`: model status and download action
- `src/main.rs`: start/stop the pipeline on config change

## Validation

- `cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt` passes.
- Fixture test: a bundled short WAV (public-domain speech, a few seconds) transcribes with the
  expected key words (`cargo test -p orbolay-rtt --features test-model`, skipped when the model
  is absent).
- Manual: with a game running, captions appear within ~3 s, and CPU use of the Orbolay process is
  recorded for `tiny`/`base`/`small` in ## Discoveries. Default chosen accordingly.
- Silence for 60 s produces zero STT invocations (VAD works).

## Design

- whisper.cpp through `whisper-rs`: local, multilingual (the user base is international, e.g. pt-BR),
  well-maintained, and builds with cmake/clang, which CI already installs.
- Whisper isn't streaming-native. "Pseudo-streaming" works like this: re-transcribe the growing
  utterance every ~1.5 s as a partial line (`is_partial = true`, same `id`), then emit the final text
  when VAD closes the utterance. Partials are throttled to ≤5 updates/s (see `rtt-transcript-panel`).
- Mic and loopback get separate VAD/segment streams but share one engine worker, to bound CPU. Mic
  utterances have priority only if the queue is empty, so there's no starvation.
- Explicit download, no implicit network: respects users and makes the upstream review question
  ("does Orbolay now phone home?") easy to answer.

## Alternatives Considered

- Vosk / sherpa-onnx (true streaming, lower latency). Weaker accuracy for many languages, and ONNX
  runtime adds a heavy native dependency. Kept as a possible second `SttEngine` if whisper latency is
  unacceptable.
- Cloud STT as default. Rejected for privacy, cost and API-key handling in an upstream PR.

## Open Decisions

- Default model size: decided by the CPU measurements in ## Validation (largest model that keeps
  Orbolay under ~15% of one core on average during conversation on a mid-range CPU).
- Model hosting: Hugging Face `ggerganov/whisper.cpp` URLs vs. user-supplied file only. Decided
  with upstream (epic open decision).

## Risks

- Whisper hallucinations on noise/music ("Thanks for watching!"). Mitigation: VAD gating,
  no-speech probability threshold, and dropping known hallucination phrases on low-confidence
  segments.
- Build complexity on Windows/macOS (cmake, MSVC). Mitigation: verify in `rtt-capture-windows-macos`.
  Feature-off builds are unaffected.

## Handoff

- Base branch `develop`, after `rtt-audio-capture` is merged. Audio arrives as 16 kHz mono `f32`
  with pipeline-clock timestamps.
- Never block the audio threads. All STT work happens on the engine worker.
- Put downloaded models under the Orbolay config dir (see `crates/orbolay-core/src/dirs.rs`).

## Tasks

### 1. Engine

- [ ] 1.1 Define `SttEngine`, `AudioSegment`, `TextSegment`
      files: crates/orbolay-rtt/src/stt/mod.rs (new)
- [ ] 1.2 Implement `WhisperEngine` with model path, language and threads config
      files: crates/orbolay-rtt/src/stt/whisper.rs (new), crates/orbolay-rtt/Cargo.toml, crates/orbolay-core/src/config.rs
      verify: cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt
- [ ] 1.3 Add fixture-based transcription test gated behind `test-model`
      files: crates/orbolay-rtt/tests/transcribe.rs (new), crates/orbolay-rtt/tests/fixtures/ (new)

### 2. Segmentation and pipeline

- [ ] 2.1 Implement the VAD segmenter with provisional and final chunks
      files: crates/orbolay-rtt/src/stt/vad.rs (new)
      verify: cargo test -p orbolay-rtt
- [ ] 2.2 Pipeline thread: sources → VAD → engine → AppState transcript (mic = local user, loopback = Unknown), start/stop on `rttEnabled`
      files: crates/orbolay-rtt/src/pipeline.rs (new), src/main.rs
- [ ] 2.3 Hallucination guard (no-speech threshold and phrase filter)
      files: crates/orbolay-rtt/src/stt/whisper.rs

### 3. Model management and tuning

- [ ] 3.1 Model discovery and explicit download action in RTT settings
      files: crates/orbolay-rtt/src/stt/models.rs (new), crates/orbolay-ui/src/config.rs
- [ ] 3.2 Measure latency and CPU for tiny/base/small while gaming; set the default model; record results in ## Discoveries
      verify: cargo fmt --check && cargo clippy --workspace --features rtt -- -D warnings && cargo test --workspace --features rtt
