# Phase 7 — Spaces and favourites

- playbook: feature · risk: medium · size: M
- Intent: each profile is organised into spaces (name, colour, icon). A
  window shows one space of its profile at a time; tabs move between spaces;
  favourites are profile-wide tabs shown at the top of every space, one live
  page each. Spaces, favourites and which space each window shows survive
  restarts.
- Owner direction (2026-11-20): foundations and working functions first; the
  look is plain and polish waits for Phase 12.

## Decisions

1. **A space keeps its tabs when no window shows it.** Its live pages stay
   alive, hidden and throttled by Chromium; unseen tabs stay unseen. Closing
   a window keeps its space and tabs (their pages close and become unseen);
   only the profile's last window closing puts every page of the profile
   away (dormancy, ADR 0004). Earlier, closing a non-last window forgot its
   tabs (Phase 6 decision 3); spaces replace that rule.
2. **One window per space.** Choosing a space another window shows brings
   that window forward (stage 1 invariant).
3. **Favourites are one page.** A favourite's page sits in the window that
   showed it last, or the window brought forward while showing it (the
   frame becoming *main*: the frame itself rarely becomes key, a child that
   shares its key state does). Another window showing the same favourite
   shows an empty card with the tab's title in the sidebar (stage 1 edge
   case). A closing window hands the page to the newest window showing it.
4. **Order in the sidebar**: favourites, then pinned, then regular tabs.
   ⌘1–⌘8 count in that order; ⌘9 is the last.
5. **Switching spaces**: the space header's menu (every space, New Space…,
   Edit Space…, Delete Space…), the switcher row at the sidebar's foot, and
   ⌘⌥← / ⌘⌥→ (previous, next; Arc's keys). Each space remembers its shown
   tab.
6. **Editing a space** happens in a small native window (name, colour,
   icon), like the profile manager; the frame window hands the keyboard to
   the page and cannot hold a text field. Icons are a small set of glyphs.
7. **Deleting a space** is refused for the profile's only space. A space
   with tabs asks first; its tabs become closed-tab records. A window
   showing it moves to another space not shown elsewhere, or a new empty
   one.
8. **Moving tabs**: the tab menu has Add to / Remove from Favourites and
   "Move to <space>". Dragging a row between the favourites and the space's
   tabs favourites or unfavourites it.
9. **Reopening a closed tab** puts it back in its space (or favourites) at
   its place; if the space is gone, into the current space.
10. **Storage**: `profile.db` migration 2 turns each saved window into a
    space ("Space 1", "Space 2", …) holding its tabs, so Phase 6 sessions
    carry over. Windows record which space they show.
11. **A start address** (`AERI_START_URL`) opens a window on the first space
    no window shows (a new one if all are shown, or "Home" for a new
    profile) with the address as a new tab. That tab loads even if another
    tab (the uBO offer) is shown first.
12. **⌘N** opens a window on the first space no window shows, or a new
    empty space (added during the build: without it a profile could not get
    a second window, so one-window-per-space was unreachable).

## Definition of Done (feature, medium)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage 1 Workspace rules and invariants |
| discover | Reuse-first search | `TabList` per space and for favourites; profile manager patterns for the editor |
| discover | Design documented | Decisions above |
| prove | Behaviour and failure-case tests | `profile_db` (workspace round trip, migration from Phase 6 sessions, closed tab origins) |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | End to end | `cargo xtask isolation` still passes (sessions restore through spaces); live checks of spaces, moves and favourites |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc, project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (`profile_db` workspace tables and migration, tests) | ~300 |
| cef adapter (moving a tab window between frames, shortcuts) | ~40 |
| shell (spaces, parking, favourites, moves, session) | ~550 |
| sidebar (header, favourites, switcher) and space editor window | ~350 |

## Checkpoints

1. Core storage: workspace save/load, migration, tests.
2. Shell: spaces and parking, switching, moving tabs, favourites, session.
3. Interface: sidebar header and switcher, favourites rows, editor window,
   shortcuts.
4. Verification: harness, live checks, docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done |
| 4 | done |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| `profile_db` workspace round trip, migration 2 from Phase 6 sessions, closed-tab origins | Tested | `cargo test --workspace`: 38 in `aeri-core` pass |
| fmt, clippy `-D warnings`, bundle signature | Observed | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` clean; `codesign --verify --deep --strict` passes |
| Sessions restore through spaces; isolation and dormancy hold | Tested | `cargo xtask isolation`: 13 checks, 0 failed (harness reads `space_tabs`) |
| First harness run failed: the start URL tab stayed unseen once the uBO offer tab took the foreground | Observed | "two profiles never showed the test site"; fixed by loading the tab a window opens on (decision 11) |
| New space from the switcher's "+": editor opens with "Space 2" selected and the next unused colour; typed name, picked ★ and purple, Return created it and the window switched to it with a New Tab | Observed | Live, mouse tool; screenshots `/tmp/p7-editor3.png`, `/tmp/p7-work.png` |
| Switching back by chip keeps pages alive (same CDP target ids, no reload) | Observed | CDP `/json` before and after: same three targets |
| Tab menu "Move to work" moves the tab; "Add to Favourites" makes it a favourite shown in every space | Observed | Live screenshots; ⌘⌥→ from the page switched to "work" with the favourite above its tabs |
| Workspace saved: spaces, `space_tabs`, favourites, window → space | Observed | `sqlite3` after SIGTERM: spaces Home and work (★, purple), work's two tabs, one favourite, window on space 2 |
| Restart restores the window on its space with the favourite row and the shown tab | Observed | Relaunch without start URL; screenshot `/tmp/p7-restored.png` |
| ⌘N opens a window on the first parked space; the favourite's page follows the window brought forward, both ways, one CDP target throughout | Observed | Live; log "key window shows a favourite" with homes 2→1→2; `winlist` shows the card moving between frames |
| Frame becomes-key never fires (bar or tab takes key); becomes-main does | Observed | Debug trace showed no becomes-key notifications on clicks; AX: frame `AXMain` true, bar `AXFocused` true; fixed by observing `NSWindowDidBecomeMainNotification` |
| Closing a window that held the favourite hands the page to the other window showing it | Observed | Live: closing window 2 moved the card back into window 1; one store target remained |
| Delete Space asks ("Its tab closes. ⌘⇧T reopens it."), closes the tabs, moves the window to a new space; ⌘⇧T reopens the tab there | Observed | Live prompt screenshot; CDP shows `a.html` back; `closed_tabs` empty after reopen |
| A space id is never reused while a window still shows the deleted space | Inferred | `new_space` takes the max over listed spaces and windows' spaces; found by reading after "Space 3" naming |
| Drag a tab onto the favourites row favourites it; dragging back unfavourites it at that place | Observed | Live drags, screenshots `/tmp/p7-drag1.png`, `/tmp/p7-drag2.png` |
| Edit Space renames a space and changes its icon | Observed | Live: "Space 3" → "read" with ◆ in header and chip |
| Closing a favourite shows the space's tab; the closed favourite reopens into favourites with ⌘⇧T | Observed | First try showed nothing (space had no active tab); fixed with `AeriWindow::leave_favourite`; re-run shows Page A; `closed_tabs.favourite = 1` before reopen |
| Change budget ~1,240 lines exceeded (~1,900 net added, mostly the `windows.rs` rewrite around spots and spaces) | Observed | `git diff --numstat` plus new `spaces.rs` (845) and `space_editor.rs` (228). Follow-up cleanup merged five copies of "add a blank tab" (`add_new_tab`) and two of "prepare a space to show" (`ready_to_show`) |
| Switching to a space whose shown tab is blank raises the address bar with the keyboard; switching to a page hides it | Observed | Live: new space shows the bar, AX `AXFocused` true on the bar window; back to Home hides it (screenshots `/tmp/p7-blank.png`, `/tmp/p7-back.png`) |
| Deleting a space clears its id from closed tabs, so they reopen into the current space and never into a later space given the same id | Tested, Observed | `closed_tabs_of_a_deleted_space_lose_their_origin`; live: `closed_tabs.space_id` 2 → NULL on delete |
