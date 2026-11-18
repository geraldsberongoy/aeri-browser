# ADR 0002: Chrome-style CEF window with GPUI interface in attached child windows

- Status: accepted; S1 run in Phase 1 (layering revised 2026-11-18, see
  "S1 outcome")
- Date: 2026-11-18
- Deciders: owner (kuya-egg); principal engineer (agent)

## Context

Aeri's interface (sidebar, address bar, command palette) is drawn with GPUI.
Pages fill the window edge to edge, and the interface slides over them.
Chrome Web Store install, extension popups and passkey/permission prompts
are required.

Facts (`docs/research/cef.md`, `docs/research/gpui.md`):

- **Chrome style vs Alloy style.**
  - Web Store install, extension action popups, the `tabs` and `windows`
    APIs, and passkey dialogs work only in CEF's Chrome-style browsers.
  - Alloy-style browsers run extension service workers, content scripts,
    `declarativeNetRequest` and native messaging. However, `tabs.query`
    returns nothing and there is no extension UI (Observed in a probe).
- **macOS hosting limit.** On macOS a browser placed in another app's
  `NSView`, or rendered off-screen, is forced into Alloy style
  (`MaybeSetWindowInfo`, CEF branch 8078; issue #3294 is open). Chrome
  style is available only in CEF Views windows.
- **GPUI z-order.** A native view inside a GPUI window always covers GPUI
  drawing. Stock GPUI has no composition API.
- **Off-screen rendering cost.** Frames reach GPUI through a CPU copy (BGRA
  `surface()` is unsupported; Windows has no external-surface path at all).
  Off-screen rendering is also Alloy-only.

## Options considered

| Option | What it is | Pros | Cons |
| --- | --- | --- | --- |
| **A** | Alloy browser inside a GPUI window, with Aeri building its own install and popup UI | Simplest layering | No `tabs`/`windows` APIs, no popups, no Web Store install. Breaks password-manager extensions. We would rebuild a large part of Chrome. |
| **B** | Frameless Chrome-style CEF Views window, no Chrome toolbar, with GPUI windows attached as transparent child windows (`addChildWindow`) | Keeps Chrome's extension, install, permission and passkey UI. The page is edge to edge, and GPUI keeps its own window and Metal layer. | Two windows per Aeri window: focus hand-off, click-through and resize sync must work. The single-Chrome-browser-view-per-window rule needs tab swapping. Untested. |
| **C** | GPUI view inserted into the CEF window's content view | One window | GPUI can't attach to a foreign `NSView` without patches. It mixes AppKit into a window that Chromium's Views framework owns. |
| **D** | Off-screen rendering into GPUI | Full control of compositing | Alloy only (same gaps as A). CPU copy per frame. No smooth scrolling. We re-own input and IME. |
| **E** | Patch and self-build CEF to allow Chrome style in a foreign view (#3294) or restore an extension API (PR #4187) | Clean single window | Effectively owning a Chromium build and monthly rebases, which is close to a fork. |

## Decision

Option B, gated by spike S1. If S1 fails, try C. Fall back to A only after
the owner approves a weaker first-release extension promise. D and E are
rejected.

## Why this one

B is the only option that keeps every extension requirement in Chrome's own,
maintained code while GPUI still draws the whole interface. Its risks are
local to the windowing layer and can be tested in a few days. A's risks are
an open-ended reimplementation of Chrome.

## Tradeoffs and consequences

- **Engine seam.** `aeri-cef` owns `CefWindow` creation and exposes the
  window's `NSWindow`. The GPUI layer attaches its child windows and keeps
  their frames in sync with layout.
- **Tabs.** Each tab is a `CefBrowser` in its own Chrome-style `CefWindow`
  (see "S1 outcome": views cannot be swapped in one window).
- **Positioning Chrome UI.** Extension popups, the Web Store install prompt,
  and permission and passkey sheets are Chrome UI that Aeri positions but
  does not draw. Visual consistency is limited to what Chrome allows.
- **Windows.** Chrome style does accept a parent `HWND` there, but GPUI
  drawing would still be covered by it. The same child-window approach is
  the starting assumption, re-checked by a spike in the Windows phase.
- **S1 pass criteria.**
  - The sidebar slides over the page at display refresh.
  - Transparent regions pass clicks to the page.
  - Live resize and full screen show no gaps or flicker.
  - Keyboard focus moves between the address bar and the page correctly.
  - Swapping between two tabs' browser views works.

## S1 outcome (Phase 1, 2026-11-18)

Observed on the bundled app (CEF 156.0.30, macOS 27.2, arm64):

- **One Chrome-style browser view per window, for the window's lifetime.**
  Adding a second `CefBrowserView` to a window logs "Cannot add multiple
  Chrome style BrowserViews" (`browser_view_impl.cc:324`), and the app
  exits. The check fires once the window has hosted a Chrome browser, even
  if the first view was removed. Swapping views in one window is
  impossible.
- **Chrome opens its own windows for tabs it creates.** CDP
  `Target.createTarget` (and so `chrome.tabs.create`) opened a full Chrome
  window, with tab strip and toolbar, beside Aeri's window. Phase 2 must
  catch these (`GetDefaultClient`, popup handling) and turn them into Aeri
  tabs.
- **Transparent GPUI pixels do not pass clicks through.** A click on a
  fully transparent margin of a GPUI window did not reach the page below.
  Overlay windows are therefore sized to their content, and they ignore the
  mouse while hidden.

Revised layering, approved by the owner ("stacked" option):

```text
  address bar   GPUI child window (pop-up kind, re-levelled to the frame)
  active tab    frameless Chrome-style CefWindow, child window covering the frame
  other tabs    CefWindows, ordered out (Chromium reports them hidden)
  frame         GPUI window: owns size, position, minimise, full screen, Space
```

- Tab windows show their own traffic lights, because the frame's are
  covered. Close and minimise are retargeted to the frame (target/action).
  The zoom button ignores target/action, so an app-local mouse-down monitor
  sends it to the frame. Option-click zooms instead.
- A 10 pt strip along the top of the page drags the frame (same monitor).
  A double-click there zooms.
- When the frame becomes key, it hands the keyboard to the address bar if
  shown, otherwise to the active tab.
- `NSWindowDidResize` and the full-screen notifications on the frame refit
  every child. Child windows follow the frame into its full-screen Space
  (`FullScreenAuxiliary`). Traffic lights are hidden in full screen.
- Checked: drag, edge resize, minimise and restore, full screen from the
  green button and Ctrl-Cmd-F, tab switching (the hidden tab reports
  `visibilityState: hidden`), close button, Cmd-Q.
- Not yet measured: per-tab cost of the extra window (Phase 13), and
  flicker during live resize at 120 fps (owner's manual check).
