# Phase 3 — Background tabs behave

- playbook: feature · risk: medium · size: S
- Intent: tabs not on screen cost little. A tab opened in the background
  gets no CEF window (and no renderer) until it is first shown. Hidden tabs
  are throttled by Chromium. Aeri knows when a tab is playing sound or
  using the camera or microphone, so later phases (sidebar indicators,
  resting in Phase 11) can act on it.
- Proves: spike S7. If throttling or the audio signal were missing, the
  stage-1 resource rules would go back to the owner.
- Design: ADR 0007 (tab signals).

## Definition of Done (feature, medium)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; phase plan; stage-1 resource rules |
| discover | Reuse-first search done before new code | CEF 156 headers (`cef_display_handler.h`, `cef_audio_handler.h`, `cef_browser.h`); `chrome.tabs` API |
| discover | Design documented (ADR if architectural) | ADR 0007 |
| prove | Behaviour and failure-case tests written and passing | Core unit tests for the new tab state; scripted S7 checks below |
| prove | Existing test suite passes | `cargo test --workspace` |
| prove | Evidence ledger complete | Ledger below |
| prove | Actual diff within declared change budget | Budget below |
| prove | Docs and operational notes updated | ADR 0007, roadmap, architecture doc, project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| cef adapter (media access, signals extension + pipe) | ~250 |
| app shell (lazy tabs, signal routing) | ~120 |
| core (events, tab state, tests) | ~80 |
| bundled extension | ~40 |

Stop and re-plan at roughly twice that.

## Checkpoints (dependency first, smallest first)

1. **Throttling (measure only).** A tab whose window is ordered out reports
   `visibilityState: hidden`; its `requestAnimationFrame` stops and its
   timers drop to about one run per second. An audible hidden tab keeps
   its timers. Check: counters in a page, read over CDP.
2. **Unseen background tabs.** A link opened in a new tab (⌘-, ⇧- or
   middle-click) reaches `OnOpenURLFromTab`; Aeri opens it as its own tab
   next to the opener. A background one gets no CEF window until it is
   first shown; then its window is created and the page loads. Check: no
   page target for it until shown; Ctrl-Tab shows it and it loads.
3. **Capture signal.** `CefDisplayHandler::OnMediaAccessChange` becomes
   `Event::MediaAccessChanged`; the tab model keeps camera and microphone
   use. Check: `getUserMedia` with fake devices sets and clears it.
4. **Audible signal.** A bundled, Aeri-owned extension reports
   `chrome.tabs` `audible` changes through a binding on the private
   DevTools pipe; Aeri maps the Chrome window id to its tab
   (`Event::AudibleChanged`). Check: an oscillator sets audible, stopping it
   clears it; the extension shows no button.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done (measured, no code) |
| 2 | done |
| 3 | built; unit-tested in the core; live check open (see residuals) |
| 4 | done |

Actual diff: aeri-cef ≈ 330 lines (budget 250), app ≈ 60 (120), core ≈ 70
(80), extension 27 (40). Within twice the budget; the overrun is DevTools
events and window-id registration, shared with ADR 0006's code.

Residual risks:

- Camera and microphone signal not observed live. `getUserMedia` from a
  terminal-launched Aeri raised a macOS microphone prompt attributed to the
  terminal (Tern); it was left for the owner. Recheck from `open aeri.app`.
- Screen capture (`getDisplayMedia`) is not covered by
  `OnMediaAccessChange` as far as the header says; unverified.
- Background link tabs load by URL: no referrer, no POST body.
- The tab-signals worker's memory cost is unmeasured (Phase 13).

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| A hidden tab is throttled by Chromium with no Aeri code | Observed | 2026-11-18, example.com hidden behind a second tab: visible 100 timer runs/s (10 ms interval) and 121 rAF/s; hidden: rAF 0 at once, timers 51 → 17 → 13 → 3 → 1/s within 5 s |
| An audible hidden tab is not timer-throttled | Observed | Same tab with an oscillator playing while hidden: 100 timer runs/s, rAF 0 |
| CEF 156 has no audible-state callback | Observed (source) | Headers: only `CefAudioHandler` (captures the stream instead of playing it) and `Set/IsAudioMuted`; no audible notification |
| `chrome.tabs` reports `audible` for Aeri's tabs | Observed | Probe extension (`--load-extension`), `chrome.tabs.onUpdated`: `{audible:true}` for the tab's Chrome window id when an oscillator started, `{audible:false}` after it stopped, also while the tab was hidden |
| `--load-extension` works in Chrome-style CEF and loads into every open profile | Observed | Probe service worker ran once per loaded profile (two targets); `chrome.tabs.query` in the Aeri profile listed the example.com tab |
| CEF reports camera/microphone use | Observed (source) | `CefDisplayHandler::OnMediaAccessChange(has_video_access, has_audio_access)` |
| ⌘-clicked links reach `OnOpenURLFromTab`, not `OnBeforePopup`; unhandled, Chrome opens and loads an orphan Chrome tab | Observed | Before: log `Chrome-created tab opens as an Aeri tab url="https://example.org/"` + `closing a Chrome-created browser`, and the new tab came to the front |
| A ⌘-clicked link opens an unseen tab: no page target until shown, then it loads | Observed | After: log `link opens a tab … foreground=false`; CDP pages = only the opener; Ctrl-Tab → `example.org` page target appears, `visibilityState: visible` |
| The bundled extension loads with its fixed id and reports audible end to end | Observed | Worker `chrome-extension://ffcbpbjbkjhcbbfodkaoflpflihegfjc/worker.js`; oscillator → log `tab audible tab=tab-1 audible=true` 0.3 s later; stop → `audible=false` 2.2 s later |
| Reports flip only changed tabs, and a tab registering late catches up | Tested | `signals::tests::reports_flip_only_changed_tabs_and_late_tabs_catch_up` |
| A crash or close clears a tab's activity | Tested | `tab::tests::activity_follows_signals_and_a_crash_clears_it` |
| `getUserMedia` from a terminal-launched Aeri asks macOS on behalf of the terminal | Observed | System prompt "Allow "Tern" to access your microphone?" despite `--use-fake-device-for-media-stream`; not answered |
