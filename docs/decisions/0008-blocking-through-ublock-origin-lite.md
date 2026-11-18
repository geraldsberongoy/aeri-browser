# ADR 0008: Ad and tracker blocking through uBlock Origin Lite

- Status: accepted; supersedes [ADR 0003](0003-content-blocking.md)
- Date: 2026-11-18
- Deciders: owner (kuya-egg), at the start of Phase 4

## Context

ADR 0003 chose a built-in blocker: adblock-rust in the browser process,
filter lists downloaded at first launch, cosmetic filtering written by Aeri.
When Phase 4 began, the owner asked why Aeri cannot simply use uBlock
Origin.

- Classic uBlock Origin (Manifest V2) cannot run: Chromium removed MV2 and
  the Chrome Web Store dropped MV2 extensions on 2026-08-31 (research,
  `blocking-and-credentials.md` §2.4). Bringing it back needs a Chromium
  patch, and Aeri forks nothing.
- uBlock Origin Lite (Manifest V3) installs from the store and works in Aeri
  today: Phase 2 installed it, persisted it and opened its popup (ADR 0006).

Its limits, from its own author, are what ADR 0003 weighed: lists refresh
only with extension updates; no `redirect-rule`, `replace=`, regex
`removeparam`, CNAME uncloaking; Chrome's rule caps; generic cosmetic
filtering only in its "Complete" mode; one service worker per profile.

## Decision

Aeri has no built-in blocker. Ads and trackers are blocked by uBlock Origin
Lite, which the Person installs from the Chrome Web Store like any
extension. At a profile's first launch Aeri opens uBO Lite's store page in a
tab (one click on "Add to Chrome"), once per profile, unless it is already
installed. Phase 4 is reduced to that offer; spike S5 is dropped.

## Why

The owner's call: it saves a medium-size phase now and the separate later
work ADR 0003 still needed (CNAME uncloaking, procedural cosmetic filters,
scriptlets), and uBO Lite is maintained by uBO's own author.

## Consequences

- Blocking is not on by default: it starts when the Person adds uBO Lite.
  Stage 1's "on by default" rule is relaxed to "offered at first launch".
- Blocking quality is uBO Lite's, including its limits above.
- Per-site switching off is uBO Lite's own (its popup).
- No filter lists, engine cache or `Blocking/` directory in Aeri's data; no
  list-licence attribution in Aeri's UI.
- Each profile runs uBO Lite's service worker when it wakes (rarely: it is
  declarative). Its cost belongs in Phase 13's measurements.
- A built-in blocker can come back later as its own decision, if uBO Lite's
  limits show up in practice.
