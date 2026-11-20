# ADR 0006: Extension UI through a hidden Chrome toolbar and a private DevTools pipe

- Status: accepted
- Date: 2026-11-18
- Deciders: principal engineer (agent), under ADR 0002's owner approval
- Spike: S2 (Phase 2)

## Context

Chrome Web Store extensions must work in the first usable release (uBO
Lite, Bitwarden, a dark-mode extension). Aeri draws its own interface with
GPUI and shows the page edge to edge, so there is no visible Chrome toolbar
(ADR 0002).

Facts from CEF 156 / Chromium 156 (sources and observations in the Phase 2
spec ledger):

- Installing from the store works in a tab window as is: Chrome's install
  dialog is tab-modal, and the extension lands in the profile.
- Each Aeri tab is a Chrome-style CEF browser hosted in a `CefBrowserView`.
  CEF makes every such browser a `TYPE_POPUP` Chrome browser. Chrome never
  finds a "tabbed browser" for the profile, so whenever it needs a tab
  (`chrome.tabs.create`, an options page, the post-install bubble) it
  creates a new Chrome-UI window. CEF gives that window's browser
  `CefBrowserProcessHandler::GetDefaultClient()`, or no client at all.
- Extension popups and bubbles anchor to buttons in Chrome's toolbar, and
  the `ExtensionsContainer` exists only in the `CEF_CTT_NORMAL` toolbar.
  CEF creates that toolbar as a view the application places.
- Chrome has no command that opens an extension's action
  (`IDC_EXTENSIONS_SUBMENU` through `ExecuteChromeCommand` does nothing).
  The DevTools method `Extensions.triggerAction` does it, but only on a
  browser-target session with `--enable-unsafe-extension-debugging`. CEF's
  own per-browser DevTools sessions are page sessions and are refused.
- `--remote-debugging-pipe` works in CEF's Chrome style: one trusted
  browser-target session on fds 3 (Chromium reads) and 4 (Chromium writes).

## Options considered

1. **Visible Chrome toolbar** above the page. Everything works, but it is
   Chrome's UI, not Aeri's. Rejected: breaks the page-first design.
2. **Host the popup page ourselves** (load `popup.html` in an Aeri view).
   Rejected: `chrome.tabs.query({active, currentWindow})`, `activeTab` and
   popup lifetime semantics break.
3. **Hidden Chrome toolbar plus `Extensions.triggerAction` over a private
   pipe.** Chrome keeps its real extension UI and semantics; Aeri only
   chooses what to draw and when to open it.

## Decision

Option 3.

- Every tab window asks for `CEF_CTT_NORMAL`, adds the toolbar view, keeps
  it invisible and outside the layout, and sizes it to the window's top
  edge. The page still fills the window. Chrome anchors action popups,
  permission and install bubbles to the hidden buttons, so they appear at
  the page's top right.
- `aeri_cef::load` puts the Chromium ends of two pipes on fds 3 and 4 in the
  browser process, before anything else opens files, and appends
  `--remote-debugging-pipe` and `--enable-unsafe-extension-debugging`.
  If either fd is taken, the switches are not added and extension actions
  are reported unavailable.
- GPUI draws one button per enabled extension with an action, from the
  profile's `extensions.settings` preference, refreshed by a CEF
  preference observer.
- A click runs `Extensions.triggerAction` for the window's active tab. The
  tab target is found by Chrome window id: the tab's own CEF DevTools
  session answers `Browser.getWindowForTarget`, and each Aeri tab is alone
  in its Chrome window. (Phase 9: that session stays watched for the tab's
  life, with `Page` enabled, to report popups Chrome's blocker refuses.)
- Tabs pages open (`window.open`, links with a target) get a new tab id in
  `OnBeforePopup`. Their popup browser view gets a tab window inside
  `OnPopupBrowserViewCreated` (CEF needs the widget before the callback
  returns) and the application adopts it next to the opener.
- Browsers Chrome creates itself get an orphan client: the window is made
  invisible at once, the first main-frame URL becomes an Aeri tab in the
  same profile (found from the request context's cache path), and the
  browser is closed in a posted task. Blank and new-tab-page windows are
  Chrome's own (the post-install bubble's host) and are closed without
  opening anything.

## Consequences

- Aeri's process exposes a browser-target DevTools session that can also
  load and uninstall unpacked extensions. Only code inside the process can
  reach it (inherited pipe fds, no socket). `--enable-unsafe-extension-debugging`
  also widens the development-only `AERI_REMOTE_DEBUGGING_PORT`, which is
  never set in release builds.
- An extension's `chrome.tabs.create` resolves with the id of the Chrome tab
  Aeri closes; later `chrome.tabs.update` on that id fails. Opening options
  pages and onboarding works. `chrome.tabs.create()` without a URL opens
  nothing.
- `chrome.windows.create` and `window.open` with popup features become tabs,
  not windows.
- Popups anchor to the top right of the page, not to the clicked GPUI
  button. Chrome's own "extension added" bubble is never seen; the new
  button in the address bar is the confirmation.
- Closing a Chrome-created window inside Chrome's install-success task
  crashed the app (observed twice). The close is therefore always posted.
- Chrome still loads the profile recorded as last used at startup, so its
  extensions run even when another profile is open (observed with the
  `AERI_PROFILE` switch). Profiles stay isolated; the cost belongs to
  Phase 5 (S4).
- If a future CEF lets a Views-hosted browser be a normal tabbed browser,
  the orphan routing can go.
