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
isolation harness's `devtools`/`site`; no file is over ~630 lines. Phase 9
(trust) is built: certificate exceptions cleared before a profile is ready,
Chrome's popup blocker kept (allowed sites in its site settings) with
refused popups reported from each tab's `Page.windowOpen` as a Popup blocked
notice, downloads placed by `aeri-core::trust` (`aeri-cef::downloads`) with a
strip menu, Chrome's own history off (`history.saving_disabled` set through
CEF's C API: cef-rs's `set_preference` fails), Clear History… by range,
Chrome's settings pages for site settings and clearing, Open in Default
Browser, usage descriptions for macOS prompts; isolation 13/13 and live
checks. Evidence: `.monozukuri/specifications/phase-9-trust.md`. Phase 10
(passwords) is built: S6 showed Chrome's password manager works in Aeri's
windows; the owner chose it as the basic store (ADR 0009, stage-1 storage
rule revised). Provider per profile (`aeri-core::credentials`, `profile.db`
migration 6), Chrome's preferences set before pages open (save only for
Aeri's store, fill only on pick after the screen lock, no auto sign-in, no
leak checks), profile menu and palette, Manage Passwords…. Evidence:
`.monozukuri/specifications/phase-10-passwords.md`. Phase 11 (resource
stewardship) is built: `aeri-core::resources` decides which tabs rest
(period in `app.db` migration 4, keep-awake sites in `profile.db`
migration 7, `LoadState::Resting`, typed input from CEF's key handler); the
shell rests through the put-away path on a minute check and a libdispatch
memory-pressure source; a Resources window reads CEF's task manager (held
only while open; cef-rs's `task_info` drops its out-param, so it is called
raw). Evidence: `.monozukuri/specifications/phase-11-resource-stewardship.md`.
Phase 12 (appearance and customisation) is built from the GPT-6.2 Sol design
(`docs/design/phase-12-appearance.md`): `aeri-core::appearance` (OKLCH,
contrast-kept tokens, recipes and `.aeritheme` files, harmonies, library),
`aeri-core::layout`, `aeri-core::keymap` (one map for pages and Aeri's
windows); page-first chrome in transparent overlay windows with an
app-local edge monitor; scoped themes (space over profile over app,
`profile.db` migrations 8–9 also keep tab names and icons); theme editor,
Settings and tab editor windows; see-through window via GPUI's blurred
background. Evidence: `.monozukuri/specifications/phase-12-appearance-and-customisation.md`.
Next: Phase 13 (benchmarks and optimisation).

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
