# ADR 0001: Chromium via CEF, bound with cef-rs

- Status: accepted
- Date: 2026-11-18
- Deciders: owner (kuya-egg); principal engineer (agent)

## Context

Aeri must load websites the way Chrome does and run Chrome Web Store
extensions in its first usable release. It also has to be lighter than Helium
and must not fork Chromium (`docs/handoff.md` §13). The core is written in
Rust. Evidence: `docs/research/cef.md`, `docs/research/gpui.md`.

## Options considered

1. **Chromium through CEF (binary builds) with the `cef` crate (cef-rs).**
   - Pros: real Chromium, so sites behave as in Chrome. Chrome-style browsers
     run Chrome's extension system and Web Store install. Monthly security
     updates come as prebuilt binaries. cef-rs is current (156.6.0+156.0.30,
     2026-11-16) and licensed Apache-2.0/MIT, and it ships a macOS bundler
     and sandbox loader.
   - Cons: the framework alone is ~341 MB on disk. Standard builds lack
     H.264/AAC/Widevine. cef-rs effectively has one maintainer. There is no
     CEF API for installing extensions at runtime.
2. **Chromium fork (Helium, Brave, Arc).**
   - Pros: full control.
   - Cons: a non-goal. It means a Chromium build pipeline and rebasing
     patches every milestone.
3. **System WebKit (Search's route).**
   - Pros: ~10 MB app, very light.
   - Cons: not Chrome. Extensions run through a different engine and need
     many shims.
4. **Writing CEF bindings by hand (C API or ObjC++ shim).**
   - Pros: no third-party binding risk.
   - Cons: much more unsafe code to own, and it duplicates cef-rs.

## Decision

Option 1. Use CEF stable-channel binaries from the Spotify CDN, consumed
through cef-rs, with `CEF_API_VERSION` pinned. Aeri's core talks to the
engine only through a small command/event seam. CEF types stay inside the
`aeri-cef` crate.

## Why this one

It is the only option that meets both "Chrome compatible including Web Store
extensions" and "no fork". cef-rs removes the largest pile of unsafe FFI and
already handles the macOS helper bundle layout.

## Tradeoffs and consequences

- **Updates:** a new framework ships with each Chromium security release,
  roughly monthly. Code churn is small while the API version is pinned; the
  real cost is behaviour regressions.
- **Binding risk:** cef-rs has a single maintainer. If it stalls, we fork
  it; it is generated code over the C API.
- **Codecs and DRM:** standard builds can't play them. Handled in a separate
  decision (see `docs/research/codecs-drm.md`).
- **Lightness:** this has to come from Aeri's behaviour (lazy tabs, rest,
  blocking, shared engine), not from the engine itself.
- **Revisit** if CEF drops Chrome-style extension support or cef-rs goes
  unmaintained.
