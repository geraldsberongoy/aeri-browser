# Phase 8 — Address bar, library and command palette

- playbook: feature · risk: medium · size: M
- Intent: the address bar suggests from the profile's own open tabs,
  bookmarks and history while the Person types, and nothing typed leaves the
  machine before Return. Bookmarks can be made, organised in folders and
  opened. A command palette reaches tabs, history, bookmarks, spaces,
  profiles and commands. The address rests at the top of the sidebar and the
  bar floats over the page when called (built in Phase 1).
- Owner direction (2026-11-20): foundations and working functions first; the
  look is plain and polish waits for Phase 12.

## Decisions

1. **Suggestions are local and per profile.** They come from the window's
   profile only: its open tabs (every space and the favourites), its
   bookmarks and its history. No suggestion service, no prefetch; the page
   is not touched until the Person picks a row or presses Return.
2. **Ranking** is a pure function in `aeri-core` (`suggest`). Every word of
   the query must match the address or the title, case-insensitively. An
   address whose host (without `www.`) or full address starts with the query
   ranks first, then title word starts, then any substring; ties go to more
   visits and more recent visits. Bookmarks rank above history at equal
   match. One row per address: an open tab wins over a bookmark, a bookmark
   over history. At most eight rows.
3. **The default row.** If the best history or bookmark row's address starts
   with the typed text (one word, no spaces), it is selected and Return goes
   there, as Chrome's inline autocomplete does, without changing the typed
   text. Otherwise the first row is the typed text itself ("Search for …" or
   "Go to …", classified as today) and is selected. An open tab is never the
   default: picking one switches to that tab (its space, its window).
4. **Keys and mouse**: ↑ / ↓ move the selection, Return picks it, Escape
   closes the bar; a click picks a row. The bar's window grows to fit the
   rows and shrinks back.
5. **Bookmarks** live in `profile.db` (migration 3) as an ordered folder
   tree: a folder has no address; the root holds top-level rows. ⌘D (and the
   ☆ in the bar) bookmarks the shown tab at the end of the root, or, if its
   address is already bookmarked anywhere, removes those bookmarks. The tab
   menu has Bookmark Tab / Remove Bookmark.
6. **Library window** (one at a time, like the profile manager) for the
   profile of the window it was called from: Bookmarks (⌘⌥B) and History
   (⌘Y), with a filter field. Bookmarks: a folder tree, New Folder, and per
   row Open, Rename…, Move to <folder>, Delete. History: newest first; per
   row Open and Remove from History. A click opens the address in a new tab
   of the profile's newest window. Clearing all history is Phase 9 (data
   clearing).
7. **Command palette** is the same floating bar in palette mode, ⌘⇧P
   (VS Code's and Sublime's key; ⌘K is left to pages, which use it for
   their own search). Rows: commands, open tabs, spaces, profiles,
   bookmarks, history, then the typed text as a search or address. With no
   text it lists the commands. Commands carry their shortcut.
8. **Resting address**: the top of the sidebar shows the shown tab's address
   (plain text, not editable, the frame cannot hold a text field); a click
   calls the bar, like ⌘L.
9. **Scripts**: a suggestion or palette row never opens a `javascript:` or
   `data:` address; the classifier's rules apply to every pick.

## Definition of Done (feature, medium)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage 1 Browsing rules, flows 2 and 15 |
| discover | Reuse-first search | `omnibox::classify`, `TabList`, floating bar, profile manager window pattern, native menus |
| discover | Design documented | Decisions above |
| prove | Behaviour and failure-case tests | `suggest` ranking (order, dedup, default row, script addresses), bookmarks tree (add, move, rename, delete folder with children, toggle by address), history search and removal |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | End to end | `cargo xtask isolation`; live checks of suggestions, ⌘D, library, palette, resting address |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc (keys), project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (`suggest`, bookmarks and history queries, migration 3, tests) | ~450 |
| text input events, bar suggestions and palette mode, bar resizing | ~300 |
| shell (queries, picks, commands, bookmarks, library plumbing) | ~350 |
| library window | ~300 |
| sidebar resting address, shortcuts | ~60 |

## Checkpoints

1. Core: bookmarks storage, history search, suggestion ranking, tests.
2. Address bar suggestions: input events, rows, keys, resizing, picks.
3. Bookmarks: ⌘D, star, tab menu, library window.
4. Command palette.
5. Resting address in the sidebar.
6. Verification and docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done |
| 4 | done |
| 5 | done |
| 6 | done |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Ranking: address prefix > title words > substring, every word required, one row per address as its best source, default row never a tab, no script addresses, capped rows | Tested | 7 tests in `suggest` |
| Bookmarks: ordered tree, moves refused into self/contents/bookmarks, folder delete takes contents, ⌘D toggle removes every copy; history search escapes LIKE wildcards and needs every word; removal | Tested | 3 tests in `profile_db` (49 in `aeri-core` pass) |
| fmt, clippy `-D warnings`, signed bundle | Observed | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` clean; `codesign --verify --deep --strict` passes |
| Isolation and dormancy unchanged | Tested | `cargo xtask isolation`: 13 checks, 0 failed |
| Typing in the bar lists the typed text, open tabs ("Switch to Tab") and history; the bar's window grows (56 → 216 pt for four rows) and shrinks back after a pick | Observed | Live; `winlist` heights; screenshot `/tmp/p8-sugg.png` |
| ↓ moves the selection; Return opens the selected history row in the shown tab | Observed | Live; CDP: the shown tab went to `b.html` |
| A known address that starts with the typed text is the default row (Return goes there); the typed text is second | Observed | Typed `127.0.0.1:8765/c`: "Page C" first and selected (`/tmp/p8-auto.png`) |
| ⌘D bookmarks the shown page; the bar's star fills | Observed | `bookmarks` row `Page C`; screenshot `/tmp/p8-star.png` |
| First live run: reopening the bar showed the last typed text, not the shown address | Observed | Fixed: the bar keeps the shown address and restores it on reveal and dismissal |
| ⇧⌘P opens the palette listing commands with their shortcuts; toggles read as what they do ("Remove Bookmark" on a bookmarked page); typing filters; Return runs the command | Observed | Screenshots `/tmp/p8-pal.png`, `/tmp/p8-pal3.png`; "Show History" opened the library |
| The palette opens a history row in a new tab; picking an open tab switches to it | Observed | CDP: new `b.html` target; sidebar shows Page C selected (`/tmp/p8-after-sw.png`) |
| Library: history rows with Remove from History; New Folder enters rename; a bookmark moves into a folder (tree indented); a click opens the bookmark in a new tab and brings the window forward | Observed | `history` lost `a.html`; `bookmarks` rows `(1, parent 2, 'Page C')`, `(2, 'Reading')`; screenshot `/tmp/p8-bm2.png`; window order after the click |
| The resting address shows the shown tab without its scheme; a click calls the bar | Observed | Sidebar screenshot; typing after the click reached the bar |
| The space menu still opens under its header, now below the address row | Observed | Screenshot `/tmp/p8-hdr.png` |
| ⌘L sent through CDP to a page whose own search field had focus did not call the bar; real ⌘L and ⇧⌘P keys do | Observed | Live; harness-only path, not a Person's path. Not investigated further |
| Suggestions read only the window's profile's `profile.db` and tabs | Inferred | `candidates` takes the window's profile; each profile has its own file (ADR 0004) |
| Change budget ~1,460 lines exceeded: ~2,400 added (new files 1,544: `suggest.rs` 323, `library.rs` 427, `shell/library.rs` 794; +864 / −65 in existing files; ~230 of it tests) | Observed | `wc -l`, `git diff --numstat`. Mostly the palette command table and the library window's menus |
