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
Next: Phase 4 (ad and tracker blocking, S5).

## Stack in one line
Rust · CEF (Chromium) via cef-rs, one frameless Chrome-style window per tab ·
GPUI frame window with the tab windows and GPUI overlays as AppKit child
windows (ADR 0002 "S1 outcome") · SQLite per profile · adblock-rust · OS
keychain ("Aeri Safe Storage").

## Constraints from the owner
- Individual Apple Developer account. Without an organisation account there
  are no iCloud passkeys and no Apple Passwords.
- Filter lists are downloaded at first launch; none are bundled.

## Spikes the stack depends on
S1 layering · S2 Web Store install · S3 run loop · S4 profile cost/isolation ·
S5 blocking hook · S6 passwords · S7 background behaviour. Details are in
`02-tech-stack.md`.

## Deferred until an organisation Apple account exists
iCloud passkeys, Apple Passwords, Widevine VMP signing.
