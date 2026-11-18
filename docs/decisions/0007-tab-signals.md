# ADR 0007: Background tabs: unseen tabs, Chromium throttling, and tab signals

- Status: accepted
- Date: 2026-11-18
- Deciders: principal engineer (agent); the phase plan only sends S7 back to
  the owner if throttling or the audio signal were missing, and neither is
- Spike: S7 (Phase 3)

## Context

Tabs out of sight must cost little, and Aeri must know when a tab plays
sound or records the Person, even when it is hidden. Stage 1 rules depend on
that: an audible or capturing tab is never put to rest (Phase 11), and the
sidebar marks such tabs (Phase 6).

Facts from CEF 156 (evidence in the Phase 3 spec ledger):

- Every Aeri tab is its own Chrome-style CEF window (ADR 0002). A tab whose
  window is ordered out reports `visibilityState: hidden`, and Chromium
  throttles it with no help: `requestAnimationFrame` stops and timers drop
  to about one run a second within five seconds. An audible hidden tab
  keeps its timers.
- A ⌘-, ⇧- or middle-clicked link reaches `CefRequestHandler::OnOpenURLFromTab`.
  Left alone, Chrome opens it as a tab of its own: a Chrome-UI browser that
  loads at once, which Phase 2 then closes and reopens as an Aeri tab in the
  foreground.
- CEF reports camera and microphone use
  (`CefDisplayHandler::OnMediaAccessChange`), but nothing reports sound.
  `CefAudioHandler` takes the audio stream away from the speakers; it is a
  capture API, not a signal.
- Chrome itself knows: `chrome.tabs` reports `audible` (with Chromium's own
  short hold after the sound stops) for Aeri's tabs, by Chrome window id.

## Decision

1. **Unseen tabs.** Aeri handles `OnOpenURLFromTab` for new-tab
   dispositions and opens the link as its own tab, next to the opener. A
   background tab gets no CEF window, browser or renderer until it is first
   shown; then its window is created and it loads. Foreground and new-window
   dispositions open at once. Page-opened popups (`window.open`, `target`
   links) are unchanged: Chrome needs their window inside `OnBeforePopup`'s
   sequence (ADR 0006).
2. **Throttling is Chromium's.** Aeri adds no throttling of its own; ordering
   hidden tab windows out is enough.
3. **Camera and microphone** come from `OnMediaAccessChange`.
4. **Sound** comes from Aeri's own extension, `tab-signals`, bundled in
   `Aeri.app/Contents/Resources/tab-signals` and loaded with
   `--load-extension` into every profile. It has only the `tabs`
   permission, no UI and no network use. Its service worker reports the
   whole set of Chrome window ids with an audible tab. Aeri attaches to that
   worker over its private DevTools pipe (ADR 0006; target discovery finds
   restarted workers), adds the `aeriTabSignals` binding and asks for a
   first report. Each tab window reads its Chrome window id when it opens
   and registers it, so a report names Aeri tabs. Reports are complete sets,
   so a lost one is repaired by the next.
5. The tab model keeps `Activity { audible, camera, microphone }`; a crash
   or close clears it.

## Alternatives considered

- **`CefAudioHandler`.** Rejected: it captures the audio instead of playing
  it, so Aeri would have to play every tab's sound itself.
- **Watching media elements with injected script.** Rejected: misses Web
  Audio, cross-origin frames and Chromium's real audibility.
- **Patching CEF** to add an audible callback. Rejected for now: no fork
  (handoff); revisit when Aeri builds its own CEF (Phase 14).
- **Lazy creation by cancelling `OnBeforePopup`.** Not taken: ⌘-clicked
  links never reach it in Chrome style.

## Consequences

- One extra service worker per loaded profile, kept alive by the DevTools
  attachment. S4 (Phase 5) measured it: one extension renderer of about
  30 MB per open profile; it exits when the profile goes dormant.
- The extension is visible on `chrome://extensions` as "Aeri tab signals".
  It shows no button: Aeri lists only extensions with an action.
- Without the DevTools pipe (it failed to open) or outside the bundle
  (`cargo run`), tabs report no sound; Aeri logs it and carries on.
- A background tab opened from a link loses the referrer and any POST body:
  it loads by URL. Same as typing it.
- `--load-extension` is development-grade in Chrome. If Chromium removes it
  from unbranded builds, the alternative is `Extensions.loadUnpacked` over
  the pipe, which needs the same `--enable-unsafe-extension-debugging`
  switch Aeri already passes.
