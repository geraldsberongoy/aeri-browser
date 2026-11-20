# Aeri — project memory

## What it is
Aeri is an open-source, native desktop web browser. It ships on Mac first
(Apple Silicon), then Windows. It works like Chrome for websites and Chrome
Web Store extensions, is page-first and polished like Search and Arc,
customisable like Zen, and lighter than Helium. Many isolated profiles can be
open at once, one window per profile, inside one running app.

- Framed intent: `docs/handoff.md`.
- Requirements (approved): `.monozukuri/blueprint/01-business-logic.md`.
- Stack (approved, including ADR 0005 for codecs/DRM):
  `.monozukuri/blueprint/02-tech-stack.md`.
- Logic-to-stack map (approved): `.monozukuri/blueprint/03-logic-to-stack-map.md`.
- Phase plan (approved): `.monozukuri/blueprint/04-phase-plan.md`.
- ADRs live in `docs/decisions/` (public repo convention from the handoff).
- Research notes live in `docs/research/`.

## Actors
- Person: the sole human, with all privileges.
- Website: untrusted.
- Extension: semi-trusted, scoped to one profile.
- Web engine: external.
- Operating system.

## Current phase
Phase 1 (Browse one page) is built and verified; CI is green. The 120 fps
resize recording shows the page trailing the frame by up to 10 pt in some
frames (residual). The window now has a strip with its own traffic lights,
back, forward and reload, and the page is a rounded card below it.
Phase 2 (Web Store extensions, S2) is built and verified by scripted
checks: uBO Lite, Bitwarden and Dark Reader install, persist, and open
popups from GPUI buttons; page- and Chrome-opened tabs become Aeri tabs
(ADR 0006). Evidence: `.monozukuri/specifications/phase-2-chrome-web-store-extensions.md`.
Phase 3 (background tabs, S7) is built: background link tabs have no
window until shown, Chromium throttles hidden tabs, and tabs report sound
(bundled tab-signals extension) and camera/microphone (ADR 0007). Open: a
live camera/microphone check from `open aeri.app`. Evidence:
`.monozukuri/specifications/phase-3-background-tabs.md`.
Phase 4 (blocking) is done the light way: Aeri offers uBlock Origin Lite at a
profile's first launch; the built-in blocker and S5 are dropped (ADR 0008,
owner 2026-11-18). Phase 5 (profiles, S4) is built: profile badge and native
menu in the strip, Profiles menu, profile manager window, dormancy, deferred
deletion, orderly quit on SIGTERM, `cargo xtask isolation` (11/11). Still to
drive live: rename, recolour, delete, Dock reopen. Evidence:
`.monozukuri/specifications/phase-5-profiles.md`. Phase 6 (tabs and
session) is built: `TabList` and `ProfileDb` in `aeri-core`, the shell's
window/tab module (`apps/aeri/src/shell/windows.rs`), a plain sidebar, ⌘S,
⌘1–9, ⌘⇧T, session restore with lazy tabs, history, "Open in <profile>"
from the tab menu; `cargo xtask isolation` 13/13; sidebar mouse actions
driven live. Evidence:
`.monozukuri/specifications/phase-6-tabs-and-session.md`. Phase 7 (spaces
and favourites) is built: spaces per profile (header menu, switcher chips,
⌘⌥←/→, a space editor window), one window per space with the others parked
(pages alive, hidden), favourites as one live page that follows the key
window, moves by menu and drag, ⌘N, `profile.db` migration 2; isolation
13/13 and live mouse checks. Evidence:
`.monozukuri/specifications/phase-7-spaces-and-favourites.md`. Phase 8
(address bar, library, palette) is built: local suggestions ranked in
`aeri-core::suggest`, bookmarks as a folder tree in `profile.db` (migration
3), a library window, the ⇧⌘P palette in the floating bar, the resting
address in the sidebar; isolation 13/13 and live checks. Evidence:
`.monozukuri/specifications/phase-8-address-bar-library-palette.md`. Before
Phase 9, oversized files were split by concern with no behaviour change:
`shell/` into one module per concern (windows, tabs, tab menu, spaces,
space settings, favourites, bar, palette, library, profiles, shortcuts,
session, quit), `profile_db/` (workspace, closed, history, bookmarks),
`mac/frame.rs`, `window_frame/sidebar.rs`, `text_input/element.rs` and the
isolation harness's `devtools`/`site`; no file is over ~630 lines. Next:
Phase 9 (trust).

## Stack in one line
Rust · CEF (Chromium) via cef-rs, one frameless Chrome-style window per tab ·
GPUI frame window with the tab windows and GPUI overlays as AppKit child
windows (ADR 0002 "S1 outcome") · SQLite per profile · uBlock Origin Lite · OS
keychain ("Aeri Safe Storage").

## Constraints from the owner
- Individual Apple Developer account. Without an organisation account there
  are no iCloud passkeys and no Apple Passwords.
- No built-in blocker: uBlock Origin Lite is offered instead (ADR 0008).

## Spikes the stack depends on
S1 layering · S2 Web Store install · S3 run loop · S4 profile cost/isolation ·
S5 blocking hook (dropped, ADR 0008) · S6 passwords · S7 background behaviour. Details are in
`02-tech-stack.md`.

## Deferred until an organisation Apple account exists
iCloud passkeys, Apple Passwords, Widevine VMP signing.
