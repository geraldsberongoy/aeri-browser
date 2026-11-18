# ADR 0002: Chrome-style CEF window with GPUI interface in attached child windows

- Status: accepted, gated by spike S1 (Phase 1)
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
- **Tabs.** Each tab is a `CefBrowser`, and switching tabs swaps the browser
  view in the window. S1 must prove the swap is cheap and flicker-free.
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
