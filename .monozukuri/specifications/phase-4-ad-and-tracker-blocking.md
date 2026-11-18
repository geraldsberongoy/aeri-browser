# Phase 4 — Ad and tracker blocking through uBlock Origin Lite

- playbook: feature · risk: low · size: XS
- Intent: a new profile is offered uBlock Origin Lite once, at its first
  launch: its Chrome Web Store page opens in a tab, one click installs it.
- Design: ADR 0008, which supersedes ADR 0003 (built-in adblock-rust
  blocker). Spike S5 is dropped with it.

## Owner decision (2026-11-18)

Asked "why can't we just use uBlock Origin?" The facts given: classic uBO
(MV2) cannot run on Chromium 156; uBO Lite already works in Aeri (Phase 2);
uBO Lite's limits as its author lists them. Answer: "Use uBO Lite: drop
built-in blocking, have Aeri offer uBO Lite at first launch, skip Phase 4,
go to Phase 5."

## Definition of Done (feature, low)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; ADR 0008 |
| discover | Design documented | ADR 0008; ADR 0003 marked superseded |
| prove | Behaviour test | `store::tests::the_blocker_is_offered_once_per_profile` |
| prove | Existing test suite passes | `cargo test --workspace` |
| prove | Docs updated | ADR 0008, roadmap, phase plan, tech stack, project memory |

## Checkpoint

1. **Offer once per profile.** `app.db` migration 2 adds
   `profiles.blocker_offered`; `AppDb::take_blocker_offer` claims it
   atomically. When the profile is ready and uBO Lite
   (`ddkjiahejlhfcafbddmgiahcphecmpfh`) is not installed, the shell opens
   its store page as a foreground tab. Check: a fresh data directory opens
   the store page; a second launch does not.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Classic uBO cannot run on Chromium 156 | Observed (source) | Research `blocking-and-credentials.md` §2.4 |
| uBO Lite installs, persists and opens its popup in Aeri | Observed | Phase 2 spec ledger |
| The offer is claimed once per profile | Tested | `store::tests::the_blocker_is_offered_once_per_profile` |
| A fresh data directory opens uBO Lite's store page; the next launch does not | Observed | `AERI_DATA_DIR=/tmp/aeri-p4`: first launch page targets = store page + `about:blank`, log "offering uBlock Origin Lite"; relaunch = `about:blank` only, no log line |
