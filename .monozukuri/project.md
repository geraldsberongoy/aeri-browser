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
Phase 1 (Browse one page) is built and verified by scripted checks, except
the owner's 120 fps resize recording and the first CI run on GitHub.
Evidence: `.monozukuri/specifications/phase-1-browse-one-page.md`. Next:
Phase 2 (Web Store extensions, spike S2). Phase 2 also routes Chrome-opened
tabs into Aeri tabs.

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
