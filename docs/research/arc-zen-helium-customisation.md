# Arc / Arc Search / Zen / Helium: appearance and customisation inventory

Researched 2026-11-20 for Phase 12 (scout subagent, read-only). Verifies the
"(verify)" items in `docs/design/phase-12-appearance.md`.

## 0. Status and method

- **Arc is being sunset.** The Arc macOS release notes say "Arc is being sunset on December 22, 2026" and that accounts are deactivated that day. Notes since 2025 are only Chromium bumps. Arc 1.173.1 shipped 2026-11-12, and the last features were the Easel ZIP export (1.171.0) and global Boost toggle. Arc's move-to-Dia guidance is in the same notes. [Arc macOS release notes](https://resources.arc.net/hc/en-us/articles/20498293324823-Arc-for-macOS-2024-2026-Release-Notes)
- **Arc help-centre pages are behind Cloudflare.** Direct fetches returned 403. I read them through Wayback copies of the same `resources.arc.net` URLs. The URLs below are the canonical help-centre ones.
- **Zen evidence is strongest.** I read the Zen docs, `prefs/zen/*.yaml` and `ZenGradientGenerator.mjs` on the `dev` branch (checked against tag 1.24.3b), the `zen-preferences.ftl` Settings strings, and release notes up to 1.24.3b (2026-11-17). Source-code facts are marked **[src]**.
- **Helium evidence** is the helium.computer homepage and `i18n/source.gen.json`, which holds the real Settings strings. Helium is beta, version 0.21.3.
- **Arc Search** evidence is the help article plus its iOS release notes.
- **Confidence flags:**
  - **confirmed** = stated in a primary source I read.
  - **inferred** = deduced from source constants or a secondary source.
  - **unconfirmed** = I looked and found no primary evidence.

### Source key

| Tag | URL |
|---|---|
| ARC-SPACES | https://resources.arc.net/hc/en-us/articles/19228064149143-Spaces-Distinct-Browsing-Areas |
| ARC-BOOSTS | https://resources.arc.net/hc/en-us/articles/19212718608151-Boosts-Customize-Any-Website |
| ARC-START | https://start.arc.net/paint-the-internet |
| ARC-CMD | https://start.arc.net/command-bar-actions |
| ARC-KEYS | https://resources.arc.net/hc/en-us/articles/20595231349911-Keyboard-Shortcuts |
| ARC-REMAP | https://resources.arc.net/hc/en-us/articles/25622899860631-How-to-Set-Custom-Keyboard-Shortcuts-on-Arc-for-Desktop |
| ARC-ARCHIVE | https://resources.arc.net/hc/en-us/articles/19228855311127-Auto-Archive-Clean-as-you-go |
| ARC-FAV | https://resources.arc.net/hc/en-us/articles/19230755904151-Favorites-Top-Tabs-Across-Every-Space |
| ARC-PIN | https://resources.arc.net/hc/en-us/articles/19231060187159-Pinned-Tabs-Tabs-you-want-to-stick-around |
| ARC-SPLIT | https://resources.arc.net/hc/en-us/articles/19335393146775-Split-View-View-Multiple-Tabs-at-Once |
| ARC-ICON | https://resources.arc.net/hc/en-us/articles/22352326167703-Change-Arc-for-Desktop-App-Icon |
| ARC-NOTES | https://resources.arc.net/hc/en-us/articles/20498293324823-Arc-for-macOS-2024-2026-Release-Notes |
| ARC-THEME-RESET | https://resources.arc.net/hc/en-us/articles/25625261733143-How-Do-I-Restore-the-Default-Theme-for-Arc-for-Desktop |
| ARC-THEME-2ND | https://candid.technology/how-to-change-theme-on-arc-browser (secondary) |
| ARC-LITTLE | https://resources.arc.net/hc/en-us/articles/19235387524503-Little-Arc-Quick-Lookups-Instant-Triaging |
| ARC-SEARCH | https://resources.arc.net/hc/en-us/articles/20887042551831-Arc-for-iOS-Arc-Search |
| ARC-SEARCH-ICON | https://resources.arc.net/hc/en-us/articles/27777014088215-How-Do-I-Change-the-Arc-Search-App-Icon |
| ARC-SEARCH-NOTES | https://resources.arc.net/hc/en-us/articles/23528454620311-Arc-Search-for-iOS-Release-Notes |
| ARC-RIGHT-THREAD | https://www.reddit.com/r/ArcBrowser/comments/120v1ul/is_there_a_way_to_move_sidebar_to_right_instead (2023, secondary) |
| ZEN-DOCS | https://docs.zen-browser.app/user-manual |
| ZEN-COMPACT | https://docs.zen-browser.app/user-manual/compact-mode |
| ZEN-URLBAR | https://docs.zen-browser.app/user-manual/urlbar |
| ZEN-WS | https://docs.zen-browser.app/user-manual/workspaces |
| ZEN-SPLIT | https://docs.zen-browser.app/user-manual/split-view |
| ZEN-GLANCE | https://docs.zen-browser.app/user-manual/glance |
| ZEN-KEYS | https://docs.zen-browser.app/user-manual/shortcuts |
| ZEN-FLAGS | https://docs.zen-browser.app/guides/about-config-flags |
| ZEN-CSS | https://docs.zen-browser.app/guides/live-editing |
| ZEN-MODS | https://docs.zen-browser.app/themes-store/themes-marketplace and https://docs.zen-browser.app/themes-store/themes-marketplace-preferences |
| ZEN-FAQ | https://docs.zen-browser.app/llms.txt (FAQ section) |
| ZEN-RN | https://zen-browser.app/release-notes |
| ZEN-FTL | https://raw.githubusercontent.com/zen-browser/desktop/dev/locales/en-US/browser/browser/preferences/zen-preferences.ftl |
| ZEN-GRAD | https://raw.githubusercontent.com/zen-browser/desktop/dev/src/zen/spaces/ZenGradientGenerator.mjs |
| ZEN-MIG | https://raw.githubusercontent.com/zen-browser/desktop/1.24b/src/zen/migration/ZenMigrationUtils.sys.mjs |
| ZEN-PREFS | https://github.com/zen-browser/desktop/tree/dev/prefs/zen (view, theme, zen, workspaces, compact-mode, zen-urlbar, folders, boosts, mods .yaml) |
| ZEN-ARCGUIDE | https://flavienbonvin.com/articles/how-to-make-zen-browser-feel-like-arc (secondary) |
| HEL-HOME | https://helium.computer/ |
| HEL-I18N | https://raw.githubusercontent.com/imputnet/helium/main/i18n/source.gen.json |
| HEL-REPO | https://github.com/imputnet/helium |

---

## 1. Themes and colours

| Capability | Arc (macOS) | Arc Search (iOS) | Zen | Helium |
|---|---|---|---|---|
| Per-space theme | **"Theme Picker"** / "Edit Theme". Opened from the brush icon by the space name, the Cmd-T command "Edit Theme", or right-click on empty sidebar. One theme per Space. [ARC-SPACES], [ARC-START] (confirmed) | none found; spaces sync but no theme editor documented [ARC-SEARCH] (unconfirmed) | **"Change Theme Colors"** (context menu on the browser UI, also on the space-creation form). The theme is stored per space. [ZEN-ARCGUIDE], [ZEN-GRAD] (confirmed) | no spaces. Chromium-style browser colour theme only (homepage: "color themes") [HEL-HOME] (confirmed, details unconfirmed) |
| Gradient stops | Gradient is macOS-only; Windows has a flat colour picker. [ARC-THEME-2ND] (confirmed). Stop cap not stated by Arc. Zen's Arc importer reads up to 3 colours: `kMaxSpaceColors = 3` [ZEN-MIG] (inferred, 3) | n/a | Dots on a colour pad: **max 3** (`static MAX_DOTS = 3`). Left-click adds, right-click removes. [ZEN-GRAD], [ZEN-ARCGUIDE] (confirmed) | n/a |
| Colour harmony presets | "predefined colours" swatches. [ARC-THEME-2ND] (secondary) | n/a | Harmony modes: **complementary, singleAnalogous, splitComplementary, analogous, triadic, floating**. [ZEN-GRAD] [src] (confirmed) | n/a |
| Grain / texture | **"Graininess dial"**, macOS only. [ARC-THEME-2ND]. Theme data stores `noiseFactor`. [ZEN-MIG] | n/a | **Texture dial**. Textures: **none, grain, sand, denim, tweed**, plus a grain amount. Click the dial centre to cycle. Haptic tick on macOS. [ZEN-GRAD] [src], [ZEN-RN] 1.24.1b "New grain options" (confirmed) | n/a |
| Opacity / intensity | **Intensity / "luminosity"** control. Theme data stores `intensityFactor`. [ARC-THEME-2ND], [ZEN-MIG] (confirmed) | n/a | **Opacity slider** in the picker. Source constants: layer alpha 0.2 to 0.88, window alpha 0.35 to 1. Importer maps Arc intensity to opacity, clamped 0.3 to 0.8. [ZEN-GRAD], [ZEN-MIG] (inferred) | n/a |
| Window translucency / blur | Windows only: **"Windows backdrop"** = Mica or Acrylic. [ARC-THEME-2ND] (confirmed). macOS vibrancy: unconfirmed | none | Prefs: `zen.theme.acrylic-urlbar` (default true), `zen.theme.acrylic-sidebar` (Twilight default), `zen.theme.acrylic-elements` (Windows), `zen.widget.linux.transparency`, `browser.tabs.allow_transparent_browser`. [ZEN-FLAGS], [ZEN-PREFS] (confirmed). Release 1.24.2b: "urlbar now has a bit of transparency". [ZEN-RN] | **"Use native frame materials (experimental)"** [HEL-I18N] (confirmed) |
| Light / dark / system | Theme Picker top toggle: **stars (automatic), sun (light), moon (dark)**. Applies to all spaces. [ARC-SPACES] (confirmed). Cmd-T "Switch to Light/Dark Mode". [ARC-CMD] | follows system (tint matches site). [ARC-SEARCH] (confirmed) | `zen.view.window.scheme` 0/1/2 = dark/light/system [src], per-window. Private windows are forced dark. **"Dark Theme Styles"**: Night Theme (AMOLED), Default Dark, Colorful Dark. `zen.theme.dark-mode-bias` = 0.3. [ZEN-GRAD], [ZEN-FTL], [ZEN-PREFS] (confirmed) | system / Chromium default (unconfirmed detail) |
| Custom accent colour | derived from theme (unconfirmed separate control) | none | `zen.theme.accent-color` (default "AccentColor"), `zen.theme.use-system-colors`, `zen.theme.gradient.show-custom-colors` (hex input in the picker). [ZEN-FLAGS], [ZEN-PREFS] (confirmed) | none found |
| Eyedropper | unconfirmed (no evidence) | n/a | unconfirmed. A community mod "Zen Colored Picker" adds a rainbow pad: https://github.com/zen-browser/theme-store/blob/main/themes.json | none |
| Reset to default | "(-)" button in the Theme Picker deselects all colours. [ARC-THEME-RESET] (confirmed) | n/a | remove all dots (inferred) | n/a |
| Auto text and contrast | not documented | n/a | the theme picker derives accent and toolbar text colour from the dominant colour (`getAccentColorForUI`, `toolbarTextStyles`) [ZEN-GRAD] [src] (inferred) | n/a |
| Inactive window fade | unconfirmed | n/a | `zen.view.grey-out-inactive-windows` [ZEN-FLAGS] (confirmed) | n/a |
| Corner radius / gap | unconfirmed | n/a | `zen.theme.border-radius` (-1 = platform), `zen.theme.content-element-separation` (gap, default 8). about:config only. [ZEN-PREFS] (confirmed) | **"Show a rounded frame around web contents"** toggle [HEL-I18N] (confirmed) |
| Theme share / import / export | Boost FAQ links a CSS export of theme colours: arc.net/colors.html [ARC-BOOSTS]. Shared Spaces share contents; whether theme travels: unconfirmed. Community tool Arc Palette edits gradients: https://github.com/neurokitti/Arc_Palette | none | Spaces/folders/split views share by link (1.2x release notes). **Import Spaces and Profiles from Arc and Dia** (1.24b), mapping colours, intensity and noise. **Mods: "Import mods" / "Export Mods"** (settings strings). No standalone space-theme file found. [ZEN-RN], [ZEN-MIG], [ZEN-FTL] (confirmed) | no theme files found (unconfirmed) |

---

## 2. Layout

| Capability | Arc (macOS) | Arc Search | Zen | Helium |
|---|---|---|---|---|
| Vertical tabs | the only layout (sidebar). [ARC-SPACES] (confirmed) | bottom tab switcher | the only layout. FAQ: "will not support horizontal tabs in the near future". [ZEN-FAQ] (confirmed) | **"Vertical"** layout, "Vertical tab strip" (experimental, Feb 2026 per the project X post). [HEL-I18N] (confirmed) |
| Horizontal / top tabs | none | n/a | none | **"Dynamic"** and **"Compact"** layouts plus a classic mode. The site says "4 layout options". [HEL-I18N] strings: Dynamic, Compact, Vertical, "Switch to Classic layout". Names of all four: Classic, Dynamic, Compact, Vertical (inferred) |
| Layout switcher | n/a | n/a | Settings > Look and Feel > **"Browser Layout"**: **"Only Sidebar"** (Single Toolbar), **"Sidebar and Top Toolbar"** (Multiple Toolbars), **"Collapsed Sidebar"** (Collapsed Toolbar). [ZEN-FTL], [ZEN-COMPACT] (confirmed) | **"Browser layout"** picker. Shortcuts "Switch to Classic / Compact / Vertical layout". [HEL-I18N] (confirmed) |
| Sidebar side | left only. 2023 thread: "There is not" [ARC-RIGHT-THREAD]. Current state unconfirmed | n/a | **"Tabs on the right"** (`zen.tabs.vertical.right-side`). Bottom buttons reverse on the right (release notes). [ZEN-COMPACT], [ZEN-PREFS], [ZEN-RN] (confirmed) | **"Show vertical tabs on right side"** / "Tabs on right side" [HEL-I18N] (confirmed) |
| Sidebar width | drag-resize: unconfirmed | n/a | drag to resize (`zen.view.draggable-sidebar`), also while hidden. Max width `zen.view.sidebar-expanded.max-width` = 500. Shortcut **"Toggle Sidebar's Width"** (Alt+B). [ZEN-PREFS], [ZEN-FLAGS], [ZEN-KEYS], [ZEN-RN] (confirmed) | unconfirmed |
| Collapsed / icon rail | **"Toggle Sidebar"** hides it entirely (Cmd-S). No icon rail documented. [ARC-KEYS] (confirmed) | n/a | "Collapsed Sidebar" layout = favicon-only rail. Hides the mute button via pref. [ZEN-FTL], [ZEN-FLAGS] (confirmed) | vertical tabs toggle (Ctrl/Cmd+S) in Vertical layout [HEL-I18N] (confirmed) |
| Expand behaviour | n/a | n/a | Vertical Tabs: **"Expand Tabs by Default" / "Don't Expand by Default" / "Expand Tabs on Hover"**. Also **"Show Expand Button"**. [ZEN-FTL] (confirmed) | unconfirmed |
| Compact / auto-hide | Cmd-S toggles the sidebar. Hover to float it. Optional **"Arc Toolbar"** (View > Show/Hide Toolbar) for a top bar. [ARC-KEYS], [ARC-NOTES] (confirmed) | n/a | **Compact Mode** (toggle Alt+Ctrl+C). Variants **"Hide Tab bar" / "Hide Top bar" / "Hide Both"**. Hover edge to reveal. Floating toggles for sidebar and toolbar. [ZEN-COMPACT], [ZEN-FTL] (confirmed) | **"Frameless mode"**: "Automatically hide browser UI until you hover the window edge", plus **"Always show the sidebar"**, **"Always show the top bar"**, **"Pin top toolbar"** shortcut. [HEL-I18N] (confirmed) |
| Compact timings and colour | n/a | n/a | `toolbar-hide-after-hover.duration` 1000 ms, `toolbar-flash-popup` (+ `.duration` 800), `sidebar-keep-hover.duration` 150, outside-window edge offsets 200/100 px, `animate-sidebar`, `color-sidebar`, `color-toolbar`, "Use themed background for compact toolbar". [ZEN-FLAGS], [ZEN-PREFS], [ZEN-FTL] (confirmed) | n/a |
| URL bar modes | Cmd-T opens a floating Command Bar. Cmd-L edits the current URL. No mode setting found (unconfirmed) | search bar at bottom, keyboard up on launch [ARC-SEARCH] | Settings > Look and Feel > **"Zen URL Bar"**: **"Floating only when typing"** (default, pref `floating-on-type`), **"Always floating"**, **"Normal"**. [ZEN-URLBAR], [ZEN-PREFS] (confirmed) | standard omnibox. Option: "Show loading progress in the address bar" [HEL-I18N] |
| URL bar details | n/a | n/a | `show-domain-only-in-sidebar` (default true), `single-toolbar-show-copy-url`, `single-toolbar-show-zoom-button`, `show-pip-button`, `show-protections-icon`, `hide-one-offs`, `wait-to-clear` 45000 ms, `replace-newtab`. [ZEN-PREFS], [ZEN-FLAGS] (confirmed) | **"Show the zoom indicator in the address bar"** [HEL-I18N] (confirmed) |
| Toolbar customisation | extensions pinned to sidebar/toolbar. Slot editor: none found (unconfirmed) | n/a | layouts are fixed. Extension overflow: `zen.view.overflow-webext-toolbar` with threshold 55. Firefox "Customize toolbar": unconfirmed. [ZEN-PREFS] | Chromium toolbar plus **"Focus toolbar"** shortcut. Slot editor unconfirmed |
| Window controls | unconfirmed | n/a | `zen.view.hide-window-controls` (default true), `experimental-force-window-controls-left`, `experimental-no-window-controls`, `zen.widget.mac.mono-window-controls`. [ZEN-FLAGS], [ZEN-PREFS] (confirmed) | unconfirmed |
| Window drag from content | n/a | n/a | **"Allow dragging the window from web pages"** (top 5% of the page). [ZEN-FTL], [ZEN-PREFS] (confirmed) | n/a |
| UI density | none found (unconfirmed) | text scales with iOS text size in onboarding, settings, Browse for Me [ARC-SEARCH-NOTES v1.22.2] | none found. Sidebar font size increased once (release notes). Only radius and gap prefs | **"Interface density"** dropdown: default compact / **"Spacious"** (larger tabs, toolbar buttons, address bar). [HEL-I18N] (confirmed). Only two levels |
| Fonts / UI size | unconfirmed. Page zoom Cmd +/-/0 [ARC-CMD] | iOS Dynamic Type in parts | UI font: none. Page fonts via Firefox settings (unconfirmed). Zen boosts have `zen.boosts.custom-fonts` (default off) | page zoom and Chromium font settings (unconfirmed) |
| Spaces switcher location | bottom of sidebar [ARC-SPACES] | tab switcher | bottom of sidebar. Emojis in the switcher were enlarged (release notes) | n/a |

---

## 3. Tabs

| Capability | Arc (macOS) | Arc Search | Zen | Helium |
|---|---|---|---|---|
| **Pinned tabs** | **"Pinned Tabs"**: per-space, above the divider, never auto-archive. Click icon to reset to pinned URL (Cmd-click keeps the previous page). Collapse pinned section via the caret (macOS). [ARC-PIN], [ARC-NOTES] (confirmed) | pin to a space from mobile [ARC-SEARCH] | **"Pinned Tabs"** (Settings pane). Options: restore to pinned URL on startup, **"Close Tab Shortcut Behavior"** (6 modes). "Reset Pinned Tab to Pinned URL" shortcut. [ZEN-FTL], [ZEN-PREFS], [ZEN-KEYS] (confirmed) | pins (Chromium). Tab groups pin to the bookmarks bar via option [HEL-I18N] |
| **Favourites / Essentials** | **"Favorites"**: up to 12, shown as an icon grid at the top above the space title, shared across all spaces. [ARC-FAV] (confirmed) | **"Top Sites"**: star to add, otherwise most visited. [ARC-SEARCH] (confirmed) | **"Essentials"**: up to 12 (`zen.tabs.essentials.max`), icon-only, shown across spaces. Options: container-specific essentials, `zen.workspaces.separate-essentials`, `zen.theme.essentials-favicon-bg`, `essentials-unread-dot` (Twilight). Glance auto-opens for external links. [ZEN-PREFS], [ZEN-FTL], [ZEN-FLAGS] (confirmed) | none found |
| **Folders** | folders inside Pinned, custom folder icons. [ARC-PIN]; folder icons: https://allthings.how/how-to-set-a-custom-icon-for-folders-in-arc-browser (secondary) | spaces, pinned, today synced | **folders**, nested up to 5 levels (`zen.folders.max-subfolders`), folder search, hover delay 500 ms. **Live Folders** (GitHub issues/PRs, filters). [ZEN-PREFS], [ZEN-RN] (confirmed) | **tab groups** (Chromium) with next/previous/close-group shortcuts [HEL-I18N] |
| **Rename tab** | double-click tab name, or Cmd-T "Rename Current Tab". [ARC-START], [ARC-CMD] (confirmed) | none found | any tab: right-click or double-click the title (earlier: pinned only). `zen.tabs.rename-tabs`. [ZEN-RN], [ZEN-FLAGS] (confirmed) | unconfirmed |
| **Custom tab icon** | right-click tab > **"Change Icon"**. [ARC-START] (confirmed) | none found | icon picker for folders, tabs etc. (release notes). [ZEN-RN] (confirmed) | none found |
| **Auto-archive / auto-close** | **"Archive tabs after"**, default 12 h, per Profile. Cannot be disabled. Restore via View Archive. [ARC-ARCHIVE] (confirmed) | **"Archive Inactive Tabs"**: 12 hours, 1 day, 3 days, 7 days, 30 days. [ARC-SEARCH] (confirmed) | **tab unloader** (`zen.tab-unloader.excluded-urls`), dim unloaded tabs (`zen.tabs.dim-pending`). No auto-archive found. "Clear tabs" button (`zen.view.show-clear-tabs-button`). [ZEN-FLAGS], [ZEN-PREFS] (confirmed) | **"Hibernate"** tabs, Memory Saver. [HEL-HOME] |
| Tab cycling | Ctrl-Tab recent tabs [ARC-KEYS] | n/a | **"Ctrl+Tab cycles through tabs in recently used order"** (popup of up to 7). Options: within Essential or Workspace tabs only, ignore pending. [ZEN-KEYS], [ZEN-FTL] (confirmed) | **MRU tabbing** [HEL-HOME] |
| Tab strip niceties | haptic feedback when reordering tabs (setting). [ARC-NOTES] | n/a | `zen.view.show-newtab-button-top`, "Show New Tab Button on Tab List", scroll-to-selected-tab button, switch tabs by scrolling pref. [ZEN-FTL], [ZEN-PREFS], [ZEN-FLAGS] | n/a |
| Tab sync | Arc Sync | Arc Sync with desktop | Spaces/folders/pinned sync via Mozilla account, optional unpinned tabs. [ZEN-FTL] | no sync [HEL-HOME] |

---

## 4. Spaces / workspaces

| Capability | Arc | Arc Search | Zen | Helium |
|---|---|---|---|---|
| Space icon | per-space Icon. Emoji picker with skin tones for Space/Tab icons. [ARC-SPACES], [ARC-NOTES] (confirmed) | spaces synced (display unconfirmed) | each workspace has a custom **icon** and name. Emoji supported, icon picker. [ZEN-WS], [ZEN-RN], [ZEN-MIG] (confirmed) | no spaces |
| Space colour | Theme per space (section 1) | none found | Gradient theme per workspace | n/a |
| Space profile / container | per-space **Profile** (separate cookies/history/extensions). Per-profile archive timing and search engine in Settings > Profiles. [ARC-ARCHIVE], [ARC-NOTES] (confirmed) | n/a | **default container tab** per workspace. Containers have 9 colours and 13 icons. Option to jump to the workspace when opening a container tab. Containers do not isolate history or extensions. [ZEN-WS] (confirmed) | n/a |
| Link routing | **"Air Traffic Control"** (Settings > Links): URL contains / equals to a Space. [ARC-SPACES] (confirmed) | n/a | **"Space Routing"** (space menu > Space Routing Settings), regex, "Add Route for Domain" menu item. Settings moved to Library. [ZEN-RN] (confirmed) | n/a |
| Switching | click icon, two-finger swipe, Ctrl+N, Cmd-T name. [ARC-SPACES] | n/a | Alt+Ctrl+Q/E, "Switch to Workspace 1-10" (unbound by default), swipe with wrap-around, natural scroll, scroll modifier key, switch animation 250 ms. [ZEN-KEYS], [ZEN-FLAGS], [ZEN-PREFS] | n/a |
| Share | **"Share Space"** permalink; Folders and Splits too. Non-Arc users see a web view. [ARC-SPACES] (confirmed) | n/a | share Spaces, folders, split views via link (release notes). [ZEN-RN] (confirmed) | n/a |
| Import from other browser | n/a (Dia migration) | n/a | import Spaces and Profiles from **Arc and Dia**. [ZEN-RN] 1.24b (confirmed) | n/a |

---

## 5. Shortcuts

| Capability | Arc macOS | Arc Windows | Arc Search | Zen | Helium |
|---|---|---|---|---|---|
| Remapping | **Settings > Shortcuts**: select a row, press keys; a modifier is recommended. [ARC-REMAP] (confirmed) | not supported [ARC-REMAP] | none found | **Settings > Keyboard Shortcuts** (now `about:keyboard`): click field, press keys, Esc saves. Red outline plus **"Conflict with another shortcut"**; Backspace clears. Groups: Spaces, Tabs, Split View, Layout. `zen.keyboard.shortcuts.enabled`. [ZEN-KEYS], [ZEN-FTL], [ZEN-RN] (confirmed) | **"Keyboard shortcuts"** Settings subpage: record dialog, remove, **reset all**. macOS note on protected system shortcuts. [HEL-I18N] (confirmed) |
| Scope | app-level. Per-space or per-site scope: unconfirmed | n/a | n/a | not every Firefox shortcut is editable. Each action has one binding. Per-space scope: unconfirmed | per-command |
| Layout and sidebar commands | n/a | n/a | n/a | Toggle Floating Sidebar, Toggle Floating Toolbar, Toggle Compact Mode, Toggle Sidebar's Width, Toggle Library, New Empty Split View | Switch to Classic / Compact / Vertical, Toggle vertical tabs, Pin top toolbar, Focus toolbar |
| Command palette | Cmd-T Command Bar actions list incl. Settings. [ARC-CMD] | n/a | n/a | Command bar actions: Expand current view, New space, Reopen closed tab, Duplicate tab, Reset pinned tab. [ZEN-RN] | n/a |

---

## 6. Mods, Boosts, userChrome

| Capability | Arc | Zen | Helium |
|---|---|---|---|
| Per-site customisation | **"Boosts"** (macOS only). Editor: **Color wheel**, **Invert lightness**, **Advanced color controls** (contrast, brightness, original saturation), **Reset to original colors**, **Font selector** (presets), **Size** 90 to 150%, **Case**, **Zap** (remove elements), **Code** (CSS and JS). One domain per Boost; not per Profile. **Boost sharing deprecated.** Global off switch in Advanced Settings. [ARC-BOOSTS], [ARC-NOTES] (confirmed) | **Boosts** (cloned from Arc): colour tinting, size override, zap bar, "focus" feature, sharing with others (1.24b). Prefs: `zen.boosts.enabled`, `dissolve-on-zap`, `custom-fonts` (default off), `invert-channel-floor` 15. [ZEN-RN], [ZEN-PREFS] (confirmed) | none found. Uses Chromium extensions |
| Browser UI CSS | none (no userChrome) | **userChrome.css** in the profile `chrome/` folder. Needs `toolkit.legacyUserProfileCustomizations.stylesheets`; live edit via the Style Editor. [ZEN-CSS] (confirmed) | none found (unconfirmed) |
| Mod registry | n/a | **"Zen Mods"** store. One-click install; **Check for Updates**, **Import mods**, **Export Mods**, disable all, per-mod toggle. Mods ship `preferences.json` with `checkbox` / `dropdown` / `string` options and an OS `disabledOn`. [ZEN-MODS], [ZEN-FTL] (confirmed) | n/a |
| Extensions | Chrome Web Store | Firefox add-ons | all Chromium extensions, proxied via Helium services [HEL-HOME] |

---

## 7. App icon, New Tab, motion

| Capability | Arc | Arc Search | Zen | Helium |
|---|---|---|---|---|
| App icon | Settings > **Icon** (macOS). Originally unlocked by referrals; all unlocked since v1.70.0. Quit dialog uses the chosen icon. [ARC-ICON], [ARC-NOTES] (confirmed) | Settings > **Change App Icon**. iOS 18 dark and tinted variants. [ARC-SEARCH-ICON], [ARC-SEARCH-NOTES] (confirmed) | none found (unconfirmed) | own icon only (unconfirmed alternates) |
| New tab / start page | no NTP: Cmd-T Command Bar. **Easels**, Notes, Library. [ARC-START] (confirmed) | opens with keyboard up. Home-screen widget with **"Show Background"** toggle. [ARC-SEARCH], [ARC-SEARCH-NOTES] (confirmed) | **New Tab = floating URL bar** (`zen.urlbar.replace-newtab`). Legacy page at `about:newtab`. Top Sites in URL bar (`show-pinned`, `show-sponsored` off). [ZEN-URLBAR], [ZEN-PREFS] (confirmed) | default Chromium NTP; `!bangs` in omnibox [HEL-HOME] |
| Little window | **Little Arc** (Cmd-Opt-N). [ARC-LITTLE], [ARC-KEYS] | n/a | **Little Zen** (Twilight 1.25t): shortcut from any app, open external links in it. [ZEN-RN] | n/a |
| Motion | haptic feedback when reordering tabs. Reduced-motion: unconfirmed | smooth animations | respects system "reduce motion" (1.2x fixes). Prefs: `compact.animate-sidebar`, `workspaces.switch-animation-duration` 250, `downloads.download-animation` (+duration 1000), `startup.smooth-scroll-in-tabs`, `zen.haptic-feedback.enabled`, `zen.watermark.enabled`, `zen.view.enable-loading-indicator`. [ZEN-FLAGS], [ZEN-PREFS], [ZEN-RN] (confirmed) | **"battery saver"** limits smooth scrolling. [HEL-I18N] |
| Native chrome | n/a | n/a | macOS: SF Symbols for UI icons (1.24.1b), `zen.view.mac.show-three-dot-menu` | n/a |

---

## 8. Customisation vs browsing features

**Customisation (look, layout, organisation)**

- Space theme: gradient stops, colour harmony, grain/texture, opacity/intensity, light/dark/system, dark styles (Night/Default/Colorful), accent colour, corner radius, gap, translucency (Mica/Acrylic/acrylic prefs), inactive-window fade.
- Layout: vertical tabs, sidebar side, sidebar width, collapsed rail, expand mode, compact/auto-hide, frameless, URL-bar mode, top-toolbar presence, window-controls placement, interface density, Arc's hide-sidebar toggle.
- Organisation: pinned tabs, favourites/essentials, folders and nested folders, rename tab, tab icon, space icon, folder icon, spaces/workspaces, per-space profile/container, link routing rules, auto-archive timing, Live Folders, tab-cycling order, close-tab behaviour for pinned tabs.
- Shortcuts: keymap editor, layout command shortcuts.
- Extras: Boosts (per-site look), userChrome/Mods (UI CSS), app icon, new-tab behaviour, Top Sites, widget background, motion and haptic toggles, space share links, Arc/Dia importer.

**Browsing features (not appearance)**

- Split View (Arc up to side-by-side/top-bottom; Zen up to 4 tabs, horizontal/vertical/grid, `min-resize-width` 7%).
- Peek (Arc) and Glance (Zen: trigger Alt/Ctrl/Shift/Meta + click, buttons close / expand / split).
- Little Arc / Little Zen quick windows, Command Bar actions, Easels and Notes, Live Calendars, Air Traffic Control / Space Routing (logic, even if the rules are user-defined), Picture-in-Picture (incl. Arc-like auto-PiP on tab switch in Zen), Browse for Me, Arc Max (Tidy Tabs/Downloads), media controls, translation, reader mode, sync, container isolation, window sync.

Grey areas: Boosts (look but per-site), link routing (organisation rules), tab unloading (resource management).

---

## 9. Facts that affect scope decisions

1. **Arc is end-of-life on 2026-12-22**, so Arc cannot ship new features. Zen is the live reference and is shipping weekly.
2. **Both Arc and Zen cap gradient stops at 3.** Zen has `MAX_DOTS = 3`, and its Arc importer caps colours at 3 (`kMaxSpaceColors`). I found no 2-to-5 stop UI in either.
3. **Arc's theme model** (from Zen's importer): up to 3 colours, one `intensityFactor`, one `noiseFactor`. Zen maps these to its `opacity` and `texture`. Zen stores `{type:"gradient", gradientColors:[{c,isCustom,algorithm,isPrimary,lightness,type}], opacity, texture}`.
4. **No browser here has a three-level density setting.** Helium has two (default / Spacious). Zen and Arc have none.
5. **No toolbar slot editor** was found in Arc or Zen. Helium inherits Chromium's toolbar.
6. **A theme or settings file import/export** exists only for Zen **Mods**. No browser here exports a per-space theme file.
7. **Keymap editors:** Zen has conflict detection and clearing. Arc macOS has a plain editor (Windows has none). Helium has record, remove and reset-all.
