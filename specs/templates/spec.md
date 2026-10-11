---
slug: <SLUG>
title: <TITLE>
date: <DATE>
verification: <VERIFICATION>
---

# <TITLE>

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

<!-- MOMENT: decision. Warned on when empty once the ready gate is met.

     Connective tissue for a reader who is not holding the whole spec in their head: how the
     other sections relate to one another, not a compressed restatement of each. Plain
     language, assuming no prior context — avoid the jargon the spec itself introduces.

     Written LAST, after every other section has settled, because it can only be correct once
     they have — even though it lives here, first, because that is where a reader starts. -->

## Problem

<!-- MOMENT: decision. Gate: new (capture).

     The problem or opportunity this spec answers, and why now. This is the only section a
     freshly captured spec carries — write it even if it is two sentences. -->

## Proposal

<!-- MOMENT: build. Gate: ready (derived).

     The change at a high level, in bullet points. What will be true afterwards that is not
     true now. -->

## Out of Scope

<!-- MOMENT: build. Gate: ready (derived).

     What this spec deliberately does NOT do, and why it was ruled out.

     Empty is written `- none — <reason>`. "We drew the boundary and nothing fell outside it"
     and "nobody ever drew the boundary" are different answers, and an absent section cannot
     tell them apart. -->

## Impact

<!-- MOMENT: build + PARSED. Gate: ready (derived).

     Declared scope for human review. The `### Standards this spec will write into
     docs/standards/` sub-heading below is PARSED by `specs.py validate`: every
     `docs/standards/**.md` path bulleted under it must be named by a `## Tasks` item, or
     validate emits `sp-impact-uncovered` (warn). Keep that heading text verbatim — it is the
     anchor.

     Example of a parsed bullet:
       - `docs/standards/naming/command-surface.md` — the bijection rule for wrappers

     The sibling sub-headings are prose for the reader and are deliberately NOT parsed: they
     name paths the spec never promised to write. A spec with no such sub-heading declares
     nothing and is never flagged — the check is opt-in by writing the heading. -->

### Standards this spec will write into docs/standards/

- `<docs/standards/subject/concept.md>` — <the rule it states>

### Standards at `authority: background` this spec may resolve

- <path, or `none`>

### Product code this spec expects to touch

- `<path>` — <why>

## Validation

<!-- MOMENT: close (plus the agent's `verify:` fallback, resolved lazily). Gate: ready (derived).

     How anyone confirms this spec actually worked: the commands to run and the output they
     must produce, the fixtures to check, the invariants that must still hold afterwards.

     This section is LOAD-BEARING: a `## Tasks` item with no `verify:` line falls back to it.

     Empty is written `- none — <reason>`, which is a claim that the spec is unverifiable by
     construction. Make it on purpose or fill it in. -->

## Design

<!-- MOMENT: build. Gate: ready (derived).

     The choices made and their rationale, plus the background and binding contracts this
     design must not contradict. For each decision: what was chosen, why, and what was
     weighed against it.

     Empty is written `- none — <reason>` (e.g. "mechanical change, no design surface"). -->

## Alternatives Considered

<!-- MOMENT: decision. Gate: ready (derived).

     Whole-shape alternatives rejected at the spec level, each with the reason it lost.
     Per-decision alternatives can stay inside `## Design`; this section is for the ones that
     would have changed the spec's shape.

     Empty is written `- none — <reason>` (e.g. "only one viable approach"). -->

## Open Decisions

<!-- MOMENT: decision. Gate: ready (derived).

     What is deliberately still undecided, and how each will be decided — the evidence or the
     moment that settles it, not "TBD".

     Empty is written `- none — <reason>`. -->

## Risks

<!-- MOMENT: decision. Gate: ready (derived).

     What could go wrong, and the mitigation for each. A risk taken knowingly is written
     `ACCEPTED — <why>`; a silent failure mode is the shape to hunt for.

     Empty is written `- none — <reason>`. -->

## Handoff

<!-- MOMENT: build. Warned on when empty once the ready gate is met.

     The context an executor needs and cannot derive: the state of play, the conventions in
     force, what was already tried. Small by construction — it is sent with EVERY task.

     Refresh is bound to EVENTS, not judgment: the orchestrator rewrites this after each
     committed task. Staleness is this section's failure mode. -->

## Tasks

<!-- MOMENT: build. Gate: ready (derived).

     Checkboxes `- [ ] <id> <text>` grouped under `### N. <Section>` headings.
     `specs.py task --spec <slug> --check <id>` flips one mechanically — NEVER hand-edit the
     `[ ]` / `[x]` character. `--subject <line>` records the commit that implements it.

     A checkbox MAY carry indented metadata lines directly beneath it:

       - [ ] 3.2 Add rate limiting to the auth middleware
             files: src/middleware/auth.ts, src/config/limits.ts (new)
             pattern: src/middleware/cors.ts
             verify: pnpm test middleware/
             subject: plan/<slug>: 3.2 Add rate limiting to the auth middleware

     files:    the paths this task may touch. Declaring them is what PERMITS the task to be
               handed to an executor sub-agent, and what makes a `[P]` marker checkable.
     pattern:  an existing file to imitate — the cheapest context an executor can be given.
     verify:   the command that proves the task done. WHEN it runs is the `verification`
               frontmatter policy, not this section's business. With no `verify:` line the
               task falls back to `## Validation`.
     subject:  written by `task --check --subject`, never by hand — the SUBJECT of the commit
               that implements this task, resolved by `git log --grep --fixed-strings`. It is
               known BEFORE the commit, so the box is ticked INTO the task's own commit
               instead of a bookkeeping commit that follows it. A spec built before this
               change carries `commit: <sha>`; both forms are read, neither is backfilled.

     `[P]` right after the id marks a task parallel-eligible:

       - [ ] 3.3 [P] Add the rate-limit config loader

     Set HERE, at definition time, and NEVER inferred while building. Honoured only when the
     marked tasks' `files:` sets are provably disjoint and none writes into `docs/` —
     `specs.py parallel` checks the disjunction mechanically rather than judging it in prose.
     Serial execution is the default and needs no marker.

     A BLOCKED task is a visible marker, not a hidden counter:

       - [!] 2.3 Implement the gate check — blocked: the vendor SDK has no hook for it

     Written by the orchestrator when it decides to stop retrying; `next` skips it. There is
     no attempt budget — an honest written reason serves better than a counter nobody sees. -->

### 1. <Section>

- [ ] 1.1 <first task>
- [ ] 1.2 <next task>

## Discoveries

<!-- MOMENT: none — triage, resolved by `/specs:develop`'s discoveries bank whenever it runs,
     not tied to one of the three. No gate — appended during execution.

     One line per discovery, appended by `specs.py discover <slug> "<text>"` while building.
     Captured INDISCRIMINATELY: whether one is worth acting on is triage's judgment, not the
     executor's.

     The triage sweep resolves each entry IN PLACE, so provenance is never lost:

       - the rate limiter double-counts retries → promoted: fix-retry-accounting
       - the config loader is slow on cold start → dismissed: acceptable, runs once -->

## Outcome

<!-- MOMENT: close. Gate: promote -> archive/.

     What actually happened, written at archive time: what shipped, what was left out, what
     the next reader needs to know. `outcome: done | abandoned` is stamped into the
     frontmatter by `specs.py promote --to archive`; this section is the prose behind it.

     For an abandoned spec, the reason it will not be built is the whole content. -->
