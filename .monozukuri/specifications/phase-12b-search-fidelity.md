# Phase 12b — Search fidelity

Owner request (2026-11-20): rebuild Aeri's window to match Search
(github.com/driceroland/Search, MIT, Swift/AppKit) 1:1: layout, sidebar,
motion and smoothness; experiment from there. Phase 12's themes, layouts
and keymap stay; the default theme and layout become Search's.

Source of every number: Search's code, extracted with file:line citations
into three session notes (design tokens, sidebar layout, motion). The
values that matter are restated in the slices below and in code comments
naming the Search file they come from.

## Owner decisions (2026-11-20)

| Question | Decision |
| --- | --- |
| Reference | Search (driceroland/Search) |
| A new window | Sidebar beside the page card (Search's default); ⌘S folds it, the edge brings it back |
| Tab icons | Letter squares (Search's default), from the site's host |
| Aeri's top bar (address row, Copy, strip) | Removed from the default; the address lives in the shown tab and the centred field |

## Rules carried over

- Page-first is now a preset, not the default (owner, above).
- Search's idle-tab title (`quiet`) and New Tab ink (`faint`) sit below
  4.5:1, as in Search; Increase Contrast turns them into text (Search's
  "High contrast text"). Every other text pair keeps 4.5:1.
- Hover on rows is instant; everything that moves uses one of three
  motions (`motion.rs`): glide (0.34, 0.82), settle (0.30, 0.86), quick
  (0.14 s ease-out); the fold uses (0.14, 0.86) out, (0.13, 1.0) in.
  Reduce Motion or a theme's Motion Off gives the end state at once.
- The page is never re-laid-out per frame: sliding chrome moves over it.

## Slices

| # | Slice | Done when |
| --- | --- | --- |
| 1 | Look and motion: Search greys and tab tokens; card frame; 52 pt band with the lights at (26, 26) and Back/Forward/Reload doors; pinned squares, 28 pt rows, New tab row, foot doors; SF Symbols; live ground gliding between tabs; rows arriving, leaving and making room on settle; select on press; the fold sliding on its springs with the peek shadow; the centred field (560 × 50) over a dimmed page | Screenshots match Search's measurements; motion observed |
| 2 | Customisation 1:1: Search's Settings (rail, find, groups, switches, segmented, pictures, key boxes) over a data index; every Appearance/Tabs preference applied (cards, slide, ⌘S icons, compact strip, nav left, roomy, letters/site icons, high contrast, address on the shown tab, ⌘ numbers, reduce animations, save power); site icons; space looks (colours, presets, strength, custom gradient, grain, 24 icons, Search's space menu) | Each row changes the chrome; screenshots |
| 3 | Address in the shown tab (click the live row), ⌘K tab finder with ⌘ held | Driven live |
| 4 | Carrying tabs (held row follows the pointer, others make room), two-finger space swipe | Driven live |
| 5 | Every Search setting (inventory `settings-parity`, ~110 rows: A in Aeri, B behaviour without row, C buildable, D not on Chromium): Settings as a tab; preference store (`aeri_core::prefs`); General, Tabs, Browsing (engine), Privacy rows with their behaviour; then find, zoom, link bubble, dusk, proxy, bookmarks bar, close idle tabs, launcher; then reader, floating video, peek, split, pin folders, groups, ⌃Tab | Each row changes behaviour; no row without it; D rows absent |
| Owner | ⌘L and ⌘T as Zen's floating field (no dim, acrylic card, 150 ms grow, instant close); app icon light/dark; double-click empty chrome zooms; column ↔ icons glides | Observed |

## Status

| Slice | State |
| --- | --- |
| 1 | done (`5e40c86`, `c96deb5`); motion not yet observed |
| 2 | done (`dd9f167`); parity pass against Search.app side by side (below) |
| 3 | pending |
| 4 | pending |
| 5 | Reader, Peek, floating video, small windows, recent tabs, and two-pane Split View built. Tab groups and pin homes/folders remain. |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Search's springs in GPUI's form | Tested | `motion` tests: glide k 341.5, c 30.31; fold-in critically damped; ease-out 0.6846 at half time |
| The default theme is Search's greys and keeps 4.5:1 for every text pair but the two quiet inks | Tested | `tokens` tests (reference verbatim, neutral pairs) |
| The default layout is Search's sidebar, 232 wide in 176–440 | Tested | `layout` tests |
| The page card sits at Search's place: 232 from the left, 8 from the top, right and bottom, radius 10 | Observed | fresh profile, frame 1280×820 at (260, 194): page window at (492, 202) 1040×804 |
| Lights centred at (26, 26), history doors from x 90, letter rows from y 52, foot at H−36 | Observed | screenshot of a fresh window (dark): close disc centre (26, 26), back chevron box 90–116, first row 52–80, foot doors along the bottom |
| Pin squares follow Search's grid maths | Tested | `plan` tests at 232 wide |
| Glide, settle, fold springs and the centred field on screen | Not observed | the owner was using the Mac; scripted input stopped when Aeri lost the front |
| Native window buttons stand where the page covers the frame's own (sliding sidebar, strip, a right-hand sidebar), at the same centres | Observed | right-hand sidebar, fresh profile: AppKit's three discs drawn in the band beside the history doors (window capture) |
| The top strip no longer comes down from the top edge unless tabs run along the top | Implemented | `reveal::slides`; not driven live |
| Shown tab says its site; live row, letter tiles and the native lights in the default layout | Observed | fresh profile, window capture: "chromewebstore.google.com" in the live row |
| Search's wash maths (five colours, six presets, opacities per strength) | Tested | `wash` tests |
| Settings rows, preferences on screen, space looks | Not observed | built and compiled |
| Reference: Search.app run in its own test world (`SEARCH_PROBE=aeriref`, separate settings and stores), window captures of sidebar, icons only, top tabs, ⌘L field, Settings | Observed | captures beside Aeri's of the same states |
| Icons-only column matches Search: 52 wide, the three window buttons stacked (centres 22/45/68), Back/Forward/Reload stacked, 32 tiles with the site's icon, live ground, plus after the tiles, bookmark door at the foot; nothing grows under the pointer | Observed | side-by-side capture of both columns |
| Sidebar foot shows only the doors that apply (profile with several profiles, space with several spaces, puzzle with extensions, bookmarks) | Observed | fresh profile: bookmark door alone, as Search's |
| Settings text at AppKit's line height (1.19), not GPUI's 1.618 | Observed | Settings rows now as dense as Search's (row "Hide the column…" ≈ 52 pt in both) |
| Site icons arrive and are kept per host on disk; restored tabs wear them | Observed | log: github favicon downloaded 200; restored apple.com tabs show the Apple icon in the column |
| Settings is a tab (`aeri://settings/<page>`) with Search's pages General · Appearance · Tabs · Browsing · Passwords · Privacy · Extensions · Keyboard & Mouse · About | Observed | ⌘, opened a Settings tab beside the page; General page with Confirm before quitting, Save to, Ask where, Always show downloads |
| ⌘L/⌘T Zen card: no dim, centred above the middle, 15 radius, 62 input row, 45 rows | Observed | captures of ⌘L and ⌘T with "apple.com" typed |
| Double-click on the column's empty band zooms the window and back | Observed | frame 1280×820 → 1800×1130 → 1280×820 |
| Full column ↔ icons glides with the page | Observed | four captures during ⌘S: page edge between 232 and 60 mid-way |
| App icon light/dark/tinted in Assets.car | Tested | `assetutil --info`: Aqua, DarkAqua, Tintable |
| Find on page: capsule top right, live count, ⌘G steps, Esc clears; works on first ⌘F | Observed | "chrome" → "1 of 5" on first use; "browser" → "1 of 178"; ⌘G "2 of 652" |
| ⌘+ zooms the page | Observed | capture after ⌘+ twice |
| New rows on General, Browsing, Spaces & Pins, Privacy (Proxy), Keyboard (From any app) | Observed | captures of each page |
| Launcher key, proxy, default browser, webmail, dusk, capture | Tested (build) | not driven on screen: they change the owner's system state or need a proxy |
| Bookmarks bar above the page after ⌘D; reading progress fill on the shown row; Spaces & Pins, Browsing (Links, Video), About (Help) pages; What's New tab | Observed | captures z-bar, z-row, z-spaces, z-about, z-notes |
| What's New wraps with a heading; reading fill keeps both rounded ends; player card; site-style editor | Tested (build) | fixed after the capture; not re-driven |
| Reader, manual floating video, Peek, sized pop-up | Observed in isolated bundle | ⇧⌘R rendered the article and reloaded on exit; ⇧⌘P moved a playing video into a 440×247 viewport; Shift-click opened Peek, Esc closed it and ⌘Return promoted the same page; a 480×360 pop-up retained `window.opener`. |
| Recent-tab switcher, automatic floating and native overlays | Partially verified | Workspace tests passed. The background input probe did not open the switcher, so its foreground panel and Control-release path remain unverified. Floating gestures, return controls, and system Picture in Picture were not checked. |
| Two-pane Split View and Peek promotion | Observed in isolated bundle | ⌥⌘Return kept the same Chromium target beside its opener. Both native viewports were visible, with widths of 517 and 516 points and a 7-point gutter. ⌃⌘← moved the address-bar host to the left pane, and ⌥⌘N separated the tabs with both drafts intact. The background pointer probe could not drive the divider. |
| Floating video return and Browsing Settings | Partially verified | ⇧⌘P lifted and returned the same playing video (1040 → 440 → 1040 points); the return button was not exercised. The Settings capture showed small-window, Reader and floating-video controls; background pointer input did not change a setting. |

## Residual risks

- GPUI cannot scale a view: rows arrive by growing in height and fading,
  not by Search's 0.94 scale.
- Window buttons are grey in every capture because the Mac uses the
  Graphite accent; Search's are grey too.
- The stacked frame group needs the frame's own zoom button unhidden (only
  transparent, under the page): AppKit hides a window's standalone zoom
  buttons when its own is hidden.
- Restored tabs whose sites were never seen since the icon cache arrived
  keep their letter until they load.
- The site line uses a small built-in list of two-part suffixes, not the
  public suffix list.
