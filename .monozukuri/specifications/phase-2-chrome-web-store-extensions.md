# Phase 2 — Install extensions from the Chrome Web Store

- playbook: feature · risk: high · size: S (grew: see budget)
- Intent: in a non-default profile the Person installs uBlock Origin Lite,
  Bitwarden and a dark-mode extension from their store pages. Extensions
  persist across restart. Their buttons and popups work in an Aeri window
  without a visible Chrome toolbar. Tabs and windows that Chrome or
  extensions open become Aeri tabs, never Chrome-UI windows.
- Proves: spike S2 (ADR 0002 depends on it). If S2 fails, stop and reopen
  ADR 0002 with the owner.
- Design: ADR 0006.

## Definition of Done (feature, high)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage-1 extension flow |
| discover | Reuse-first search done before new code | CEF 8078 + Chromium 156 sources (`chrome_browser_host_impl.cc`, `extensions_handler.cc`, `devtools_pipe_handler.cc`, cefclient `views_window.cc`) |
| discover | Design documented (ADR if architectural) | ADR 0006 |
| prove | Behaviour and failure-case tests written and passing | Unit tests for extension-list parsing; manual S2 checklist below with screenshots/logs |
| prove | Existing test suite passes | `cargo test --workspace` |
| prove | Evidence ledger complete | Ledger below |
| prove | Actual diff within declared change budget | Budget below |
| prove | Docs and operational notes updated | ADR 0002 S2 outcome, architecture doc, `docs/development.md` |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| cef adapter (pipe, toolbar, extension list, routing) | ~700 |
| app shell (extension buttons, routing, profile switch) | ~400 |
| core | ~50 |

Stop and re-plan at roughly twice that.

## Checkpoints (dependency first, smallest first)

1. **Private DevTools pipe.** fds 3/4 reserved in the browser process,
   `--remote-debugging-pipe` and `--enable-unsafe-extension-debugging`
   appended for the browser process only. Check: `Browser.getVersion` answers.
2. **Hidden Chrome toolbar.** Tab windows ask for `CEF_CTT_NORMAL`; the
   toolbar view is added, kept invisible, and its bounds span the window's
   top edge. Check: page still fills the window (`innerHeight` = window
   height); a `triggerAction` popup appears at the top right.
3. **Extension list.** Live list of installed, enabled extensions with an
   action, from the profile's `extensions.settings` preference, refreshed by
   a preference observer. Check: unit test on a recorded preference value;
   install shows the button without restart.
4. **Extension buttons.** GPUI draws the buttons in the address bar; a click
   dismisses the bar and opens the popup for the active tab
   (`Browser.getWindowForTarget` via the tab's own CEF DevTools session →
   tab target with the same window id → `Extensions.triggerAction` on the
   pipe). Check: uBO Lite popup shows the active tab's host.
5. **Routing.** `window.open`/links with an opener become Aeri tabs; browsers
   Chrome creates without a client (install flow, `chrome.tabs.create`,
   `chrome.runtime.openOptionsPage`) are hidden, closed, and their URL opens
   as an Aeri tab in the right profile. Check: no Chrome-UI window during
   install; an extension's options page opens as an Aeri tab.
6. **Profile switch (dev).** `AERI_PROFILE=<name>` opens or creates a named
   non-default profile. Check: extensions installed there are absent in the
   default profile.
7. **The three extensions.** uBO Lite, Bitwarden, a dark-mode extension
   installed from their store pages; persist across restart; popups work.

## Manual S2 checklist

- [x] Store "Add to Chrome" → Chrome's install dialog → installed; no
      Chrome-UI window appears (the one Chrome creates stays at alpha 0).
- [x] Button list refreshes without restart (log `extensions changed
      count=3` right after the Bitwarden install).
- [x] Popup opens over the page for the active tab (uBO Lite, Bitwarden).
      Not yet checked: a second Aeri window.
- [x] Extension opening a tab (uBO dashboard, Dark Reader help, Bitwarden
      onboarding) lands in an Aeri tab.
- [x] Quit and relaunch: extensions still installed and enabled.
- [x] Other profile: none of them installed.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done (2 unit tests) |
| 4 | done |
| 5 | done |
| 6 | done |
| 7 | done |

Actual diff: aeri-cef ≈ 1,000 lines (budget 700), app ≈ 280, core 10;
within twice the budget. The overrun is the DevTools pipe and the orphan
routing, neither foreseen when the budget was set.

Residual risks:

- `chrome.tabs.create` returns the id of a Chrome tab Aeri closes.
- Chrome loads the last-used profile at startup even when another one is
  opened, so that profile's extensions run (Phase 5, S4).
- Popups anchor to the page's top right, not under the clicked button.
- Not checked: two Aeri windows; Chrome's bug #4350 popup delay was not
  measured.

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Chrome's install dialog works in an Aeri tab window; the extension installs into the profile's `Extensions/` dir | Observed | E1 (2026-11-18): uBO Lite from its store page with `CTT_NONE`; store then showed "Remove from Chrome" |
| Without routing, Chrome opens a Chrome-UI tabbed window during install | Observed | E1 log: `Creating a chrome browser without a client`; screenshot `/tmp/p2-chromewin.png` |
| CEF browsers hosted in a BrowserView are `TYPE_POPUP`, so Chrome never finds a tabbed browser for new tabs | Observed (source) | `chrome_browser_host_impl.cc` `CreateBrowser`: `TYPE_NORMAL` becomes `TYPE_POPUP` when `browser_view` is set |
| A Chrome `NORMAL` toolbar is created but hidden by Chrome for these browsers; `set_visible(1)` shows it | Observed | V2: `toolbar_visible=0` after `AddChildViewAt`; visible after `SetVisible(true)` (screenshot `/tmp/p2-top.png`) |
| `Extensions.triggerAction` opens the uBO Lite popup with the toolbar invisible, and the page keeps the full window | Observed | V2: popup target `popup.html`, 287×388 window; page `innerHeight` 820 = window height |
| The popup sees the right tab | Observed | V2: `chrome.tabs.query({active, currentWindow})` from the popup returned the example.com tab |
| `IDC_EXTENSIONS_SUBMENU` via `ExecuteChromeCommand` does nothing | Observed | V2: no menu window after the shortcut |
| `Extensions.triggerAction` is refused on page sessions and needs a browser-target session | Observed (source) | `extensions_handler.cc` `allow_loading_extensions_` = browser target |
| `--remote-debugging-pipe` works in CEF Chrome style and is a browser-target session | Observed | Checkpoint 1: `Browser.getVersion` reply over fds 3/4 (`Chrome/156.0.8078.96`) |
| A page session can read its Chrome window id; one Aeri tab = one Chrome window, so window id → tab target is unique | Observed | `Browser.getWindowForTarget {}` from a page session returned the same id as for its tab target |
| `Extensions.getExtensions` lists unpacked extensions only | Observed (source) | `extensions_handler.cc` `GetExtensions` |
| Installed extensions are in the profile preference `extensions.settings` | Observed | `Secure Preferences` of the test profile lists uBO Lite (location 1) and the PDF viewer (location 5) |
| GPUI buttons open the action of the active tab | Observed | Bar showed Bitwarden, Dark Reader, uBO Lite icons (`/tmp/p2-bar3.png`); click opened `popup/index.html` of Bitwarden (`/tmp/p2-bwpop.png`) and uBO Lite's popup showing "example.com" (`/tmp/p2-popup3.png`) |
| `window.open` from a page becomes an Aeri tab next to the opener, keeping `window.opener` | Observed | `Runtime.evaluate` with user gesture → log `popup becomes a tab opener=tab-1 tab=tab-2`; new page `!!window.opener` = true; Ctrl-Tab switches between them |
| A popup's tab window must exist before `OnPopupBrowserViewCreated` returns | Observed; cause from source | Deferring it to the shell's next message crashed the app right after `OnBeforePopup`; creating it inside the callback works. `chrome_browser_host_impl.cc` `Attach`: the callback "will often create the Widget", which `AddNewContents` then uses in the same call stack |
| `chrome.tabs.create({url})` from an extension opens an Aeri tab; Chrome's window never shows | Observed | uBO Lite service worker: log `Chrome-created tab opens as an Aeri tab url="https://example.net/"`; window poll saw the Chrome window only at alpha 0 |
| Closing the install flow's Chrome window inside Chrome's install-success task crashes the app; a posted close does not | Observed | Dark Reader and Bitwarden installs crashed (process corpse 30 ms after `extensions changed`, prefs not saved). Then Dark Reader (window kept open) and Bitwarden (posted close) installed without a crash |
| Extensions persist across restart | Observed | Relaunch: `extensions changed count=3`; Wikipedia rendered dark by Dark Reader |
| Extensions stay in their profile | Observed | `AERI_PROFILE=Work`: no buttons, Wikipedia light, page context `603827…` vs extension workers in `DDA406…` (the last-used Personal profile, loaded by Chrome at startup) |
