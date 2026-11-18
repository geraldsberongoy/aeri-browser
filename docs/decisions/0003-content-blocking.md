# ADR 0003: Built-in blocking with adblock-rust in the browser process

- Status: superseded by [ADR 0008](0008-blocking-through-ublock-origin-lite.md)
  (2026-11-18: Aeri blocks through uBlock Origin Lite; no built-in blocker)
- Date: 2026-11-18
- Deciders: owner (kuya-egg); principal engineer (agent)

## Context

Ad and tracker blocking is on by default, uses uBlock Origin's filter lists
at uBO quality, can be switched off per site, and must add almost no cost
per extra profile. Classic uBO (MV2) cannot run on current Chromium: the MV2
flags are gone from Chromium 156. Evidence:
`docs/research/blocking-and-credentials.md`, `docs/research/cef.md` §7.

## Options considered

1. **`adblock` crate (brave/adblock-rust) 0.13.**
   - How it runs: in Aeri's browser process. Network rules are checked in
     CEF's `OnBeforeResourceLoad` on the IO thread. Cosmetic CSS is injected
     from the renderer helper at context creation.
   - Pros:
     - ~3.9 µs per request and ~9.2 MB resident, measured with the crate's
       own benches on an M4 Pro.
     - One engine shared by all profiles.
     - Parses uBO and ABP syntax.
     - MPL-2.0, used unmodified.
   - Cons:
     - Aeri must add CNAME uncloaking, procedural cosmetic filters and
       scriptlet injection.
     - Some rules are unsupported: `$popup`, `$replace`, `$header`,
       `$ipaddress`, regex `$removeparam`.
     - CEF's interception does not see WebSockets.
2. **uBlock Origin Lite preinstalled as the built-in blocker.**
   - Pros: maintained by uBO's author, with cosmetic filtering included.
   - Cons:
     - Fixed declarativeNetRequest rule limits.
     - Lists update only when the extension updates.
     - No `redirect-rule`, `replace`, CNAME or regex `removeparam`.
     - A service worker and rulesets per profile.
3. **CEF interception with a hand-written matcher.**
   - Cons: rebuilds adblock-rust, badly.

## Decision

Option 1. Lists are downloaded at first launch and refreshed in the
background. Nothing list-related is bundled (owner decision 2026-11-18). The
serialised engine is cached on disk so start-up does not re-parse the lists.
uBO Lite stays installable by the Person. If it is enabled in a profile,
Aeri suggests turning built-in blocking off there.

## Why this one

It is the lightest choice per profile, has the closest coverage to uBO, and
keeps lists fresh without extension updates.

## Tradeoffs and consequences

- **First-launch gap:** pages load unblocked for the few seconds until the
  lists arrive.
- **Feature work:** CNAME uncloaking, procedural filters and scriptlets are
  Aeri features with their own phases. Parity is measured with adblock-rust's
  uBO coverage test.
- **WebSockets:** not blocked in v1.
- **Licensing:** lists and scriptlets are GPL-3.0 or CC BY-SA. We fetch them
  at runtime, attribute them in the UI, and get a legal check before ever
  bundling them.
- **Spike S5:** measures real per-request latency inside CEF.
