# Aeri — project memory

## What it is
Aeri is an open-source, native desktop web browser. It ships on Mac first
(Apple Silicon), then Windows. It works like Chrome for websites and Chrome
Web Store extensions, is page-first and polished like Search and Arc,
customisable like Zen, and lighter than Helium. Many isolated profiles can be
open at once, one window per profile, inside one running app.

- Framed intent: `docs/handoff.md`.
- Requirements (approved): `.monozukuri/blueprint/01-business-logic.md`.
- Stack (approved except codecs/DRM): `.monozukuri/blueprint/02-tech-stack.md`.
- ADRs live in `docs/decisions/` (public repo convention from the handoff).
- Research notes live in `docs/research/`.

## Actors
- Person: the sole human, with all privileges.
- Website: untrusted.
- Extension: semi-trusted, scoped to one profile.
- Web engine: external.
- Operating system.

## Current phase
Blueprint stage 2 is approved; stage 3 (logic-to-stack map) is next. The
codecs/DRM decision is pending research in `docs/research/codecs-drm.md`.

## Stack in one line
Rust · CEF (Chromium) via cef-rs, Chrome-style Views windows · GPUI interface
in attached child windows (ADR 0002, gated by spike S1) · SQLite per profile ·
adblock-rust · OS keychain.

## Constraints from the owner
- Individual Apple Developer account. Without an organisation account there
  are no iCloud passkeys and no Apple Passwords.
- Filter lists are downloaded at first launch; none are bundled.

## Spikes the stack depends on
S1 layering · S2 Web Store install · S3 run loop · S4 profile cost/isolation ·
S5 blocking hook. Details are in `02-tech-stack.md`.
