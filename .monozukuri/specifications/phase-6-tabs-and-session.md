# Phase 6 — Tabs and session

- playbook: feature · risk: medium · size: L
- Intent: many tabs per window, listed in a sidebar. Tabs open, close,
  reopen, duplicate, reorder and pin. Each profile's session (windows, their
  tabs, pinned state, the active tab, window placement) is restored when the
  profile opens again, with only the shown tab loading. History is recorded
  per profile.
- Owner direction (2026-11-20): "UI/UX enhancements later; make sure the
  foundations are solid and functionality okay". The sidebar is a plain,
  working list; its look, the slide-over mode and animation move to the
  appearance phase (12).

## Decisions

1. **Back/forward trails are not restored.** CEF can read a browser's
   navigation entries (`GetNavigationEntries`) but has no call to set them
   (Observed: cef 156 bindings). Restoring, reopening and duplicating a tab
   load its current address only. Stage 1 asks for the trail; this is a
   recorded deviation until CEF offers restore (or a Chromium patch, which
   Aeri does not make).
2. **Session writes** happen after the message that changed them, once per
   message, on the main thread (a few small rows; WAL). Measured below.
3. **What is remembered**: closing a window that is not its profile's last
   forgets it (as in Chrome). Closing a profile's last window, or quitting,
   keeps the session as it was just before.
4. **Start-up**: with no `AERI_START_URL`, each opened profile restores its
   session (a new tab if it has none). An explicit start URL (development,
   tests) opens one window on it instead.
5. **Sidebar** is shown by default and toggles with ⌘S (Arc's key; Aeri has
   no "Save Page" yet). ⌘1–⌘8 select a tab, ⌘9 the last; ⌘⇧T reopens the
   last closed tab of the profile. Tab context menu (native): Pin/Unpin,
   Duplicate, Close, Close Other Tabs (other unpinned tabs). Reorder by
   dragging within the pinned or unpinned group.
6. **History** records a visit when a top-level load of an http(s) or file
   address finishes; titles follow later title changes. No visits for
   failed loads, `about:`, `chrome:` or `data:`.
7. **Last tab**: the Person closing a window's last tab (⌘W, ×) leaves a
   new tab, so the window stays. A page closing its window's only tab
   (`window.close()`) closes the window, as in Chrome; the Phase 5 dormancy
   check relies on it.
8. **Open in another profile** (moved here from Phase 5): the tab context
   menu lists "Open in <profile>" for every other profile. It carries the
   address only, into a new tab of that profile's front window, or a new
   window if the profile has none (waking it). The same item on a page's
   link context menu needs CEF's context-menu handler and goes with Phase
   7's "move to space" menu.

## Definition of Done (feature, medium)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage 1 Workspace and Continuity rules |
| discover | Reuse-first search | Existing unseen-tab mechanism (S7) restores lazily; CEF navigation API checked |
| discover | Design documented | Decisions above |
| prove | Behaviour and failure-case tests | `workspace` (ordering, pinning, close/next active), `profile_db` (session round trip, closed tabs, history) |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | End to end | `cargo xtask isolation` restores both profiles' sessions on the second run |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc, project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (`workspace` tab list, `profile_db`: session, closed tabs, history) | ~450 |
| cef adapter (shortcuts) | ~20 |
| shell (tab list, session save/restore, history, reopen, duplicate, pin, reorder) | ~350 |
| sidebar view and frame layout | ~300 |
| harness (restore check) | ~40 |

## Checkpoints

1. Core tab list and profile database with tests.
2. Shell on the tab list; tab operations and shortcuts.
3. Sidebar (list, close button, context menu, drag to reorder, toggle).
4. Session save and restore; history.
5. Verification: harness, live checks, docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done; mouse interactions not driven live (below) |
| 4 | done |
| 5 | done |

Change budget: about 1,770 lines against ~1,160 (core 750 of which ~300 are
tests; shell +575 net; sidebar 230; harness 115). Over mostly in core tests
and the shell's window/tab module; no part was cut.

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| CEF cannot restore a navigation trail | Observed | cef 156 bindings: only `navigation_entries` (read) |
| Tab list ordering, pinning, close → next active, group-clamped moves | Tested | `aeri-core` `workspace` tests |
| Session round trip, closed tabs (25 kept), history rules | Tested | `aeri-core` `profile_db` tests |
| Each profile records its own history, session and closed tabs | Tested | `cargo xtask isolation`: "each profile records its own history and session" (13/13 pass) |
| Sessions restore on launch without a start address | Tested | `cargo xtask isolation`: "sessions restore", then the Phase 5 checks on the restored pages |
| Restore loads only the shown tab; others load when selected | Observed | Live, over CDP: two saved tabs, active index 1 → one target after launch; ⌘1 → second target appears |
| ⌘T, ⌘W, ⌘⇧T (at its old position), ⌘1, ⌘9 work from a page | Observed | Live, keys sent with `Input.dispatchKeyEvent`; targets and `profile.db` `windows.active` follow |
| ⌘W on the last tab leaves a new tab | Observed | Live: both tabs closed → one `about:blank` target; session holds it |
| ⌘S hides and shows the sidebar; the page widens by the sidebar | Observed | Live: shown tab's `innerWidth` 1032 ↔ 1264 |
| A session save costs 50–450 µs | Observed | `session saved us=` debug lines in the harness log |
| The last-active profile never went dormant: Chromium kept it as its startup profile (`Local State` `profile.last_used`); its tab-signals worker outlived its window | Observed | Live over CDP: closing that profile's page left its `service_worker` target past a 20 s wait (4/4) and past 120 s in the harness; the other profile's went in 0.6 s; harness failed 4 of 5 runs |
| `--profile-directory=Default` fixes it | Tested | Same live repro: 0.6–0.7 s, nothing left (3/3); `cargo xtask isolation` 13/13 on 4 runs in a row; a third tab-signals worker now runs in `Default` |
| Sidebar mouse actions (click, middle click, ×, context menu, drag) and "Open in <profile>" | Inferred | Built on the tested tab list and `TabCommand`; not driven with the mouse (owner's OK needed) |
