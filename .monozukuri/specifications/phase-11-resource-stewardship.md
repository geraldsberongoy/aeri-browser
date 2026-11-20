# Phase 11 — Resource stewardship

- playbook: feature · risk: medium (closes pages the Person may want) · size: M
- Intent: many tabs across many profiles stay cheap. Tabs not shown for a
  while, or every eligible tab when macOS is short of memory, rest: their
  page is released and comes back when shown. A resource view shows what
  each tab and profile costs and lets the Person rest or close it.
- Owner direction (2026-11-20): foundations and working functions first;
  plain look.

## Stage-1 rules (01-business-logic.md)

- Rest tabs not shown for the Person's chosen period; the Person can turn
  resting off.
- Never rest a tab that plays sound, uses the camera or microphone, holds
  typed input since it loaded, or whose site the Person keeps awake.
- Under memory pressure, rest eligible tabs least recently shown first,
  before the system has to.
- A resting tab keeps its place, title and address; showing it loads again.
- The resource view shows memory and CPU per tab and per profile, with
  rest and close there.

## Decisions

1. **Resting is the existing put-away** (`TabEntry::put_away`, used for
   restored and parked tabs): the page's browser closes; the tab keeps its
   id, place, title and address and is recreated when shown. The core tab
   gains `LoadState::Resting` (`Tab::rest`), which the page's `Closed`
   event does not end, so a woken tab keeps following its events.
2. **The rules are pure core** (`aeri_core::resources`): `awake_reason`
   (shown, sound, capturing, typed, kept awake), `idle_rests` and
   `pressure_rests` (warning: the older half of tabs unseen for a minute or
   more; critical: all eligible, oldest first).
3. **Typed input**: CEF's key handler reports a key pressed in an editable
   field without ⌘ or ⌃ as `Event::Typed`; a new load clears it.
4. **Period**: app-wide, `app.db` migration 4 `settings`
   (`rest_after_minutes`); choices never / 15 min / 30 min / 1 h / 4 h,
   default 1 h. Chosen from the resource view.
5. **Keep awake**: per profile, per site (`trust::site_of`), `profile.db`
   migration 7 `keep_awake_sites`; toggled from the tab menu ("Keep <site>
   Awake" / "Let <site> Rest"), which also has "Rest Tab".
6. **Triggers**: a check each minute (idle period and lasting pressure)
   and a libdispatch memory-pressure source (warning / critical) that
   checks at once.
7. **Resource view**: a plain GPUI window like the library. Chromium's
   task manager (CEF `CefTaskManager`) gives memory and CPU per task; each
   tab's task is found by its browser id. Rows per profile: its tabs
   (shown / awake reason / resting), with Rest and Close; then Aeri itself,
   GPU, extensions and workers, which Chromium does not attribute to a
   profile. Refreshes every two seconds while open. Palette: "Show
   Resources" opens it (no shortcut).
8. Throttling of hidden pages stays Chromium's.

## Definition of Done (feature, medium)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | Stage-1 rules above; this file |
| discover | Reality checked before design | CEF task manager and dispatch memory-pressure APIs read; live numbers checked |
| prove | Behaviour tests | exclusions; idle and pressure selection; resting survives the page's close and wakes; typed cleared by load; period and keep-awake persistence |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | End to end | Live: idle rest, exclusions, pressure rest, wake with live title, resource view numbers and actions; `cargo xtask isolation` |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc, project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (`resources`, Resting, typed, settings, keep-awake, tests) | ~300 |
| cef (typed event, task manager snapshot) | ~120 |
| shell (rest/wake, triggers, keep-awake menu, view plumbing) | ~350 |
| resource view window | ~250 |

## Checkpoints

1. Core rules and storage with tests.
2. Engine: typed event, task manager snapshot.
3. Shell: rest, idle and pressure triggers, keep awake.
4. Resource view.
5. Verification and docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done |
| 4 | done |
| 5 | done |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Exclusions, idle and pressure selection | Tested | `resources` tests |
| Resting survives the page's close; a load wakes it; typing cleared by a load | Tested | `tab` tests |
| Period default and persistence; keep-awake persistence | Tested | `store`, `profile_db::keep_awake` tests; `cargo test --workspace` 66 + 3 + 2 passed |
| Unseen tabs rest after the period | Observed | `AERI_REST_AFTER_SECS=20`: log "tab rests" for each unseen tab about 20–40 s after it was last shown; its page left the DevTools target list; the sidebar kept its row and title |
| A rested tab wakes with its page and keeps following events | Observed | Click on the rested Target row: `target.html` loaded again; `document.title` set afterwards showed in the sidebar ("Woken and retitled"): before, a put-away tab's model went `Closed` and ignored later events |
| Typed and kept-awake tabs stay | Observed | After typing "bob" into `/login` and keeping `http://127.0.0.1:8766` awake, 55 s unseen: only the Chrome Web Store tab rested; the view showed "Has typed input" and "Kept awake" |
| Audible tabs stay | Tested | `resources` tests. Live, a WebAudio tone (log `activity … audible: true`) showed "Playing sound" and stayed awake over a minute unseen, but its site was also kept awake, so this does not isolate the rule |
| Resource view figures and actions | Observed | Per tab memory and CPU (e.g. Sign in 30 MB, 0.2%), profile total, Aeri 298 MB, GPU 177 MB, network/storage utilities, extension service workers; Rest rested the Tone tab (log "tab rests tab=tab-1", state "Resting"); Close removed the Chrome Web Store row; the period menu's Never saved `rest_after_minutes = 0`; Keep … Awake saved `keep_awake_sites` = `http://127.0.0.1:8766` |
| Figures need a live sampler | Observed | Reading through a fresh `CefTaskManager` each time gave 0 for every task: CEF's manager samples (every 2 s on macOS) only while referenced, so the view holds one while open |
| Memory pressure rests tabs | Inferred | Source registered on the main queue (`mac::pressure`); `memory_pressure -S` needs root here ("kern.memorypressure_manual_trigger … Operation not permitted"); selection is unit-tested |
| Profiles stay isolated | Tested | `cargo xtask isolation`: 13 checks, 0 failed |
| Change size vs budget (~1,020) | Observed | ~1,200 lines: new files 1,166 (tests included), +246 / −210 in existing code, of which ~190 is `AppMsg` moving to `shell/msg.rs` to keep `shell/mod.rs` at 476 lines |

## Residual risks

- A rested tab loses its back/forward trail (CEF cannot restore history;
  Phase 6 residual).
- "Typed" counts any key in an editable field, including arrows; such tabs
  stay awake until they load again (errs towards keeping input). Input
  made without keys (paste by menu, drag) is not seen.
- Memory is per process: tabs sharing a renderer show the same figure, and
  totals count equal figures once (a heuristic; Inferred).
- The memory-pressure path is not exercised live (needs root to simulate).
- Waking a tab within moments of resting it can see the old page's late
  close; the shell ignores that close for resting tabs, but a woken page's
  model could briefly take the old page's last events (Inferred, unseen).
