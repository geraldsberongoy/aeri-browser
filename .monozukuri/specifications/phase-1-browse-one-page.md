# Phase 1 — Browse one page (vertical slice)

- playbook: feature · risk: high · size: M
- Intent: A signed dev build of Aeri opens a page-first window: a frameless
  Chrome-style CEF window with a GPUI address bar in an attached child window.
  It browses a real HTTPS site with keyboard, scroll, resize, full screen,
  error and crash states. It quits with no orphan processes.
- Proves: ADR 0002 (spike S1) and the run-loop integration (spike S3).
- Human gate (high): architecture approval is in ADR 0002. If S1 fails, stop
  and bring options C and A to the owner.

## Definition of Done (feature, high)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage-1 flows 1–2 |
| discover | Reuse-first search done before new code | cef-rs examples, bokuweb/gpui-cef (pump, NSApp retrofit), wry/gpui-webview (handles) |
| discover | Design documented (ADR if architectural) | ADR 0002; `docs/architecture/gpui-cef-integration.md` |
| prove | Behaviour and failure-case tests written and passing | `aeri-core` unit tests (tab state machine, omnibox, `app.db` profile); manual S1/S3 checklist with recordings/logs |
| prove | Existing test suite passes | `cargo test --workspace` |
| prove | Evidence ledger complete | Ledger section below |
| prove | Actual diff within declared change budget | Budget below |
| prove | Docs and operational notes updated | `docs/development.md` commands verified |

## Change budget

New workspace with roughly 2,500–4,000 lines of Rust:

| Part | Budget (lines) |
| --- | --- |
| core | ~600 |
| cef adapter | ~1,500 |
| app shell | ~1,200 |
| helper | ~50 |
| scripts | ~150 |

Stop and re-plan at roughly twice that.

## Checkpoints (dependency first, smallest first)

1. [S] **Toolchain probe.** cef-rs's Views example builds and runs on CEF 156
   on this Mac.
   - Exit: a Chrome-style window shows an HTTPS page.
2. [S] **Workspace and `aeri-core`.** Engine seam types (commands and
   events), the tab load-state machine, `app.db` with one profile record
   (UUID v7), and omnibox classification.
   - Exit: `cargo test -p aeri-core` is green.
3. [M] **`aeri-cef`.**
   - Scope: CEF init with `external_message_pump`; one request context at
     `Chromium/<profile-id>`; a frameless Chrome-style `CefWindow` with no
     Chrome toolbar; a `CefBrowserView`; navigation and load and crash events
     mapped onto the seam; the helper binary; the bundle script.
   - Exit: the bundled app shows a page with no GPUI yet.
4. [M] **GPUI shell.**
   - Scope: GPUI owns `[NSApp run]`; `CefAppProtocol` is added to GPUI's
     `NSApplication`; a CF timer/source pump runs in common and
     event-tracking modes; a transparent GPUI child window with the address
     bar is attached to the CEF `NSWindow` via `addChildWindow`.
   - Exit: the address bar floats over the page and typing a URL navigates.
5. [S] **Focus hand-off.** Cmd-L focuses the address bar; Return navigates and
   returns focus to the page; Esc returns focus to the page; clicking the page
   focuses it.
   - Exit: a manual check passes, with a log of first-responder changes.
6. [S] **Errors and crash.**
   - Exit:
     - an unreachable host shows Chromium's error page, keeps the URL, and
       retry works;
     - `chrome://crash` (or killing the renderer) shows Aeri's crashed state,
       and reload recovers.
7. [S] **Resize and full screen.** The child window tracks the parent through
   live resize and full screen.
   - Exit: a 120 fps screen recording shows no gap or flicker; in full
     screen the address bar hides until called.
8. [S] **Two browser views swap.** Two tabs (no sidebar yet), switched by
   shortcut.
   - Exit: the swap is flicker-free and the hidden tab reports hidden.
9. [S] **Clean shutdown.**
   - Exit: Cmd-Q closes browsers, then `CefShutdown`; `pgrep` shows 0 Aeri
     processes 2 s later; no cache lock remains.
10. [S] **Signing and CI.** An ad-hoc signed bundle launches with the hardened
    runtime off; a GitHub Actions macOS workflow runs `cargo test` and
    `cargo build`.
    - Exit: the workflow file is valid and runs locally via `act`, or is
      documented as untested.

## S1 pass criteria (from ADR 0002)

- The address bar or sidebar slides over the page at display refresh.
- Transparent regions pass clicks to the page.
- Live resize and full screen show no gaps or flicker.
- Focus moves correctly between the address bar and the page.
- Swapping between two tabs' browser views works.

## Status

| # | Checkpoint | State |
| --- | --- | --- |
| 1 | Toolchain probe | Done |
| 2 | Workspace and `aeri-core` | Done (17 tests) |
| 3 | `aeri-cef` | Done; reshaped to one `TabWindow` per tab |
| 4 | GPUI shell | Done; stacked layering (ADR 0002 "S1 outcome") |
| 5 | Focus hand-off | Done (scripted checks) |
| 6 | Errors and crash | Done (Chromium error page and sad tab; Aeri marks the tab crashed) |
| 7 | Resize and full screen | Done. 120 fps recording: no gap of another colour; the page trails the frame by up to 10 pt in some frames (residual) |
| 8 | Two tabs | Done, as two windows; the hidden tab reports `hidden` |
| 9 | Clean shutdown | Done |
| 10 | Signing and CI | Ad-hoc signing done; workflow written, not run on GitHub |

## Evidence ledger

| Kind | Entry |
| --- | --- |
| Observed | Bundled app loads `https://example.com` (CDP title "Example Domain"); typing an address or a query in the GPUI bar navigates (`en.wikipedia.org/...`, DuckDuckGo search). |
| Observed | Chrome-style windows take one `CefBrowserView` for life: "Cannot add multiple Chrome style BrowserViews" (`browser_view_impl.cc:324`), then the app exits. |
| Observed | CDP `Target.createTarget` opens a full Chrome-UI window (tab strip, toolbar) beside Aeri's. |
| Observed | Transparent pixels of a GPUI window swallow clicks (CDP `mousedown` listener saw nothing on two clicks). |
| Observed | A Chrome-style `CefRequestContext` must finish initialising before `CefBrowserView` creation. Before that, creation returns null. |
| Observed | Default Keychain item "Chromium Safe Storage" belonged to another app; its prompt stalled every network load. Fixed with CEF's `keychain_service_name` → "Aeri Safe Storage" (item created without a prompt). |
| Observed | Copying a new binary over a recently run bundle gets it SIGKILLed ("load code signature error 2"); fixed by recreating the bundle. |
| Observed | Xcode 26+ needs `xcodebuild -downloadComponent MetalToolchain` for GPUI's build-time shaders. |
| Decided | Stacked layering: GPUI frame window, one Chrome-style CEF window per tab as child windows, GPUI overlays above (owner, 2026-11-18). |
| Decided | AppKit/CEF side effects run after `cx.update` (`Shell::later`) to avoid GPUI `App` re-entrancy (observed "RefCell already borrowed" on resize otherwise). |
| Changed | ADR 0002 (S1 outcome), `docs/architecture/gpui-cef-integration.md`, `docs/development.md`, phase plan (Phase 2 routes Chrome-opened tabs; Phase 3 lazy window creation). |
| Verified | Cmd-L shows the bar (alpha 1). Return navigates; page `document.hasFocus()` is true and Space scrolls 800 px. Escape closes the bar and focus returns. |
| Verified | Cmd-T opens a tab with the bar. Ctrl-Tab and Ctrl-Shift-Tab switch tabs; the inactive tab is `visibilityState: hidden`. Cmd-W closes the tab. |
| Verified | Drag strip moves the frame with every child. Edge resize refits the page (`innerWidth` 1185). Minimise and restore act on the frame. The green button and Ctrl-Cmd-F enter and leave full screen (page 1800×1130). |
| Verified | Unreachable host shows Chromium's error page, keeps the URL, and retry reloads. `Page.crash` shows the sad tab, Aeri logs `TS_PROCESS_CRASHED`, and Reload recovers. |
| Verified | Cmd-Q, the close button and an Apple-event quit all close the browsers and run `CefShutdown`. No Aeri process remains after 0.5–1.5 s, and there is no `Singleton*` lock. |
| Verified | A cookie persists across restart in `Chromium/<profile-id>`. |
| Verified | `cargo test --workspace` (17 passed), `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt --check`, `codesign --verify --deep --strict`. |
| Measured | Live resize recorded at 120 fps with ScreenCaptureKit (2026-11-18, sRGB, card layout): frames 8.4 ms apart (median) during a 1.5 s corner drag. No foreign colour between frame and page. The card's 7 pt margin widened by up to 10 pt in 18 of 314 frames (page trails the frame). |
| Residual risk | Page trails the frame by up to 10 pt during live resize; needs the CEF window resized in the frame's Core Animation transaction. Per-tab cost of a window per tab not measured (Phase 13). |
| Changed | 2026-11-18: tab windows lost their traffic lights and drag strip. The frame draws a strip with its own lights, back, forward and reload; the page is a rounded card below it ([ADR 0002](../../docs/decisions/0002-rendering-strategy.md)). This removes the tab windows' tiling-menu and inactive-lights residuals. |
| Residual risk | Sidebar click-through (Phase 6) needs per-region `ignoresMouseEvents`. GPUI cannot pass transparent clicks. |
| Residual risk | Chromium logs "Signature validation of current process failed (-67030)" on ad-hoc builds; recheck with real signing (Phase 15). |
| Residual risk | `Chromium/Default` is still created under the root; check what it holds during profile isolation (Phase 5). |
| Budget | About 3,980 lines of Rust, including about 730 vendored from GPUI's input example. Within the 2,500–4,000 budget. The app shell is over its own 1,200 share because of window stacking. |
