# What's New in Aeri

## 0.1.0 (in development)

Aeri is a browser for the Mac in Rust and GPUI, with Chromium (CEF) rendering the pages. This is what has landed so far, newest work first.

### 26 November 2026

- Pick any of 64 offline Blobatar faces in **Manage Profiles**, or reset to the automatic face. The sidebar avatar keeps your profile accent.
- The macOS window buttons stay aligned after exiting full screen or restoring a minimized window, with tabs on the left, right, or top.
- Choose **Bottom** under Settings → Appearance → Window → Tabs, or pick **Layout: Bottom Tabs** in the command palette. In full screen the tab bar appears at the bottom edge on hover.
- The pinned sidebar now clears when entering fullscreen, so hovering the edge doesn't stack a second sidebar on top of it. Exiting fullscreen restores the windowed layout.
- Hover sidebars and tab bars line up with the browser's edges in windowed and fullscreen modes. The fullscreen top bar takes one row, including in Compact Tabs mode. The top and bottom bars no longer bring along a spare sidebar; ⌘S acts on the chosen layout.
- The red close button closes only its own window. Other profile windows stay open.
- Aeri asks which profile to open before restoring a browser window. On a fresh install, the welcome tour leads to the same picker.
- Top and bottom tabs shrink before scrolling, with the New Tab button always visible. Sidebars scroll vertically.
- The New Tab button and ⌘T focus Home’s inline search, reusing an already blank tab. Address suggestions show saved site icons.
- ⌘Delete clears back to the beginning of a line in Aeri's text fields.
- Double-click empty chrome to fill the usable display without entering fullscreen. Edge to edge mode uses the same viewport bounds for the web page, sidebar, and toolbar.
- Edge to Edge is the default for new layouts. Page scrollbars use Chromium’s wider, always-visible style on macOS.
- Web content no longer gets rounded corners in Edge to Edge mode. Cards keep their rounded border.
- Tab hover fades work with the tab strip pinned as well as in the sidebars.
- The active tab highlight glides between tabs in the top and bottom strips, using the same spring as the sidebars.
- Inspect opens Developer Tools for its own tab. In **View → Developer Tools**, dock it right or below, move it to a separate window, or close it.
- The profile picker shows saved profiles as cards.

### 21 November 2026

- Aeri’s black-and-white macOS disk image is a standard drag to Applications. On first launch, Aeri’s ribbon and wordmark ease into view in a silent film. Skip straight to the interactive tour or use it to choose a tab layout and appearance. Replay from Help → Welcome to Aeri….
- Blank tabs open Aeri Home: a solid background, the Aeri ribbon, inline search, and shortcuts to Bookmarks, History, and Settings. ⌘L opens the floating address composer.
- Aeri’s ribbon is visible in the Dock in light, dark and tinted appearances.
- The macOS menu bar has native browser commands, a live Profiles list, and Settings under Aeri (⌘,).
- Tab highlights, close buttons, and title-to-address changes fade smoothly as you move the pointer, without delaying clicks.
- Split View keeps two live tabs side by side. Option-click a tab to pair it, drag the divider to resize the panes, or press ⌥⌘Return to keep a Peek beside its opener. ⌥⌘N starts or separates a split without closing either page.
- The floating video: a video that is playing on a site made for watching comes out of its tab into a small window above everything, on every Space, when you switch tab or Space, and goes back when you return. ⇧⌘P (Float Video) does it by hand on any page. The window has play and pause, 15 seconds either way, a hairline of progress, a button back to the tab and a close button; drag it, pull its bottom-right corner or pinch it to size it, and flick it with two fingers to a corner or into a side. The command palette is now ⌥⌘P.
- Small windows: a link from another app, a site's sized pop-up, or an address from the launcher can open in a small window with just the site and Open in Aeri (⌘O), which moves the page into your window as a tab without loading it again. Esc and ⌘W close it.
- Reading Mode (⇧⌘R) displays the article on its own, with optional Markdown copies and a light/dark control.
- Shift-click a link to Peek. Escape closes it, and ⌘Return keeps the same page in a tab.
- Control-Tab switches to a recent tab. Hold Control to see previews; release it to switch. Sequential tab shortcuts are now ⇧⌘[ and ⇧⌘].
- Settings opens as a tab of its own, in Search's order and wording.
- A floating address field in the middle of the page, as Zen has it.
- Find on page and page zoom, with the keys Search uses.
- Address bar, privacy and launcher settings: the engine words go to, shortcuts for sites, email links in a webmail, the global launcher key.
- Your own style for a site, with an editor for the CSS of the site you are on.
- What's New and Send Feedback in Settings › About.
- A list of the sites kept awake, each with a Remove.

### 20 November 2026

- Themes: a light, dark or automatic look, a theme per space, a theme library, theme files to import and export, and the macOS material behind the sidebar.
- Layout choices: tabs on the left, right or top, an icons-only column, a bar that hides until the pointer reaches the edge, and the page as a card.
- Tabs can have your own name and icon. Spaces can have an emoji.
- Settings and keyboard shortcuts, with a key box for each command and a warning when two commands want the same keys.
- Motion and accessibility: reduced animations, high-contrast tab text, and fewer wakeups while idle.
- Resting tabs: tabs you have not looked at sleep, and wake when you return. A site you keep awake never rests. Memory pressure puts more tabs to sleep.
- Passwords through Chrome's password manager, with the system confirming it is you before a sign-in is filled.
- Popups go through Chrome's blocker, downloads go to your Downloads folder, and certificate errors and clearing browsing data are in reach.
- The address bar suggests from history and bookmarks, and the command palette lists commands, tabs, spaces and profiles. Bookmarks and History have a library window.
- Spaces and favourites: tabs are grouped in spaces, and favourites sit above all of them.
- Tabs and session restore: the sidebar lists your tabs, and a window comes back as you left it. Closed tabs can be reopened.

### 18 November 2026

- Many profiles, each with its own cookies, history, passwords and extensions, and a window of its own.
- Block ads and trackers with uBlock Origin Lite, offered the first time a profile opens.
- Background tabs load only when you go to them, and tabs that play sound or share the screen are never put to sleep.
- A window strip with the traffic lights, the page as a rounded card, and overlay scroll bars.
- Extensions from the Chrome Web Store, with their buttons in Aeri's own chrome.
- Browse a page: a GPUI window over one Chromium window for each tab.
