# Phase 12 — Appearance and customisation

- playbook: feature · risk: medium (touches every view and window layering) · size: L
- Intent: themes with colours the Person picks, page-first layout, and every
  look, layout and organisation customisation Arc and Zen offer where Aeri's
  stack allows, beyond them where it can.
- Design: `docs/design/phase-12-appearance.md` (GPT-6.2 Sol, owner's choice
  of designer, 2026-11-20). Competitor facts:
  `docs/research/arc-zen-helium-customisation.md` (Arc is end-of-life on
  2026-12-22; Zen is the live reference).

## Owner decisions (2026-11-20)

1. Page-first by default for everyone: the page fills the window; sidebar
   and top bar slide in at the edge or by shortcut; Pin keeps them shown.
2. Extras beyond the design: textures (grain, sand, denim, tweed);
   colour-harmony modes in the picker; collapsed icon-rail sidebar with
   expand-on-hover; layout presets (Only Sidebar / Sidebar + Top Bar /
   Collapsed / Classic Top Tabs); rename tabs and custom tab/space icons or
   emoji; address-bar options (domain only, copy link, window controls hidden
   or left).
3. Not in Phase 12: folders, auto-archive, app icons, Arc/Zen/Dia importer.
   Split View, Peek/Glance and Little Arc belong to a later phase. Boosts and
   Mods are refused (code injection; extensions cover site styling).
4. Hidden top bar: Aeri-drawn window buttons while it slides in; real
   traffic lights when pinned.

## Design deltas from the GPT-6.2 Sol text

- Textures: `texture` key (`none|grain|sand|denim|tweed`) beside `grain`
  (amount); bundled static tiles.
- Harmonies: picker modes complementary, single analogous, split
  complementary, analogous, triadic, free (Zen's set), all offline.
- Layout gains `collapsed` (icon rail, 52 pt) and expand mode (always /
  never / on hover), plus four presets that set several layout keys at once.
- Tabs and spaces gain an optional custom title and icon (emoji or symbol);
  stored in `profile.db`, exported nowhere.

## Build slices (each committed and verified on its own)

| # | Slice | Acceptance |
| --- | --- | --- |
| 1 | Theme core and token spine: `aeri-core::appearance` (OKLCH, contrast, recipe, `.aeritheme`, resolver, 10 presets); GPUI `theme` global; every view paints from tokens; app appearance (Follow Theme / System / Light / Dark) and app theme in `app.db`; follows macOS appearance and accessibility live; palette commands | Light, dark and a gradient preset render in every window; switching macOS appearance repaints at once; pages report the scheme |
| 2 | Page-first shell: auto-hide sidebar and top bar, edge reveal, pin, compact mode, full screen, proxy window buttons | Default shows only the page; edge hover and ⌘S/⌘L reveal; pin persists |
| 3 | Layout: sidebar right, width, icon rail and expand modes, presets, top tabs, favourites grid/rows, density, address-bar modes and options, toolbar slots | Each control flips live with a page open; state survives relaunch |
| 4 | Theme scopes and editor: profile and space themes, editor window (swatches, gradient, harmonies, shuffle, textures, opacity, radius), undo | A 3-colour space gradient and profile accent show with correct precedence |
| 5 | Theme library and files: import/export `.aeritheme`, user themes, optional macOS material | Round-trip; malformed files refused with the reason |
| 6 | Settings window (⌘,), command registry, keymap editor with conflicts and swap | Rebind, conflict, swap, reset, relaunch |
| 7 | Tab and space custom titles and icons | Rename and icon survive relaunch |
| 8 | Visual polish, accessibility, motion, idle-cost check | VoiceOver and keyboard paths; no idle redraw |

## Definition of Done (feature, medium)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, rules, competitors understood | Stage-1 Appearance rules; design; research file |
| discover | Owner decisions recorded | Above |
| prove | Behaviour tests | colour maths, contrast audit over a seed corpus, file validation, presets, scope precedence, keymap conflicts |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | End to end per slice | Live checks listed per slice |
| prove | No idle cost | Idle CPU sample with chrome hidden and shown |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc, project memory |

## Status

| Slice | State |
| --- | --- |
| 1 | done |
| 2 | done (slide motion lands in slice 8) |
| 3 | done (toolbar slot editor moves to slice 6; floating bar offers Page Centre and Upper Centre, not Attached) |
| 4 | done (the editor reverts with Cancel; it has no step-by-step undo) |
| 5 | done (export written but not driven live: the save panel run was stopped when keystrokes reached another app) |
| 6 | done (toolbar slots stay fixed: the strip's few buttons did not earn an editor) |
| 7–8 | pending |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Any 1–3 seed colours give readable chrome in both appearances, labels on the backdrop included | Tested | `appearance::tokens` corpus test (black, white, neon pairs, primaries, muddy, Tide) |
| Theme files refuse anything but look choices, with the reason | Tested | `appearance::recipe` tests: unknown key with full path, duplicate key, range, colour, stops, version, name, trailing text, size |
| Bundled presets are valid files and readable | Tested | `appearance` preset test |
| Every view paints from tokens | Observed | no `rgb(0x`/`rgba(0x` left outside identity colours (grep); dark, Tide dark and Tide light screenshots |
| Gradients of any stop count render with GPUI's 2-stop primitive | Observed | Tide gradient (navy to teal) behind strip and sidebar |
| Choosing Light repaints every window and pages follow | Observed | palette "Appearance: Light": chrome light; `matchMedia('(prefers-color-scheme: dark)')` true before, false after (CDP) |
| Theme and appearance survive relaunch | Observed | Tide restored from `app.db` after quit and relaunch |
| Following macOS light/dark and accessibility changes live | Inferred | observers on `AppleInterfaceThemeChangedNotification` and the workspace accessibility notification; not toggled live (changing system appearance needs an automation prompt) |
| A fresh install opens page-first, with the sidebar called once to show the note | Observed | fresh data dir: page covers the window; sidebar card with Aeri's window buttons, the note, Pin |
| Resting 120 ms in the left or top lane slides the chrome in; leaving or a click on the page sends it away | Observed | pointer moves at x=1 pt and y=1 pt; `chrome slides` log lines and screenshots |
| Transient chrome floats over CEF without moving the page | Observed | GPUI overlay child windows ordered between the tab window and the bar; page rect unchanged while shown |
| Pin and Auto-hide persist and lay the page out beside pinned chrome | Observed | Pin buttons and palette commands; relaunch keeps the layout; side-only layout carries real traffic lights in the sidebar's top row |
| ⌘S steps a pinned sidebar aside and back; calls an unpinned one | Observed | screenshots before and after ⌘S |
| Aeri's window buttons work while the strip slides in; full screen pins nothing and drops them | Observed | proxy zoom entered full screen; in full screen the sliding sidebar shows no window buttons and no Pin |
| Five layout presets set several choices at once and are recognised afterwards | Tested | `layout` tests: default is Page First; every preset round-trips and keeps side and width |
| Stored layouts survive missing keys, bad values and bad JSON | Tested | `layout::stored_layouts_survive_missing_and_bad_values` |
| Sidebar width snaps to 184–360 in 4 pt steps and leaves the page 320 pt | Tested | `layout::widths_snap_to_the_range_and_leave_the_page_room` |
| Collapsed rail with hover expand, sidebar on the right (window buttons drawn by Aeri there), top tabs, favourites grid | Observed | palette layout rows; screenshots of each; hovering the rail slid the full sidebar in |
| Dragging the sidebar's edge resizes it and the width persists | Observed | drag 240 to 324: `sidebar resized width=324`; relaunch kept 324 |
| Domain-only address and Copy Link | Observed | address row showed `127.0.0.1:8766`; Copy put `http://127.0.0.1:8766/target.html` on the clipboard (pbpaste) |
| Density and larger text change the chrome's rows and type | Observed | Density: Spacious + Larger Text screenshot |
| Harmonies turn only the hue (complementary 180°, triadic 120°) and keep lightness; Shuffle is repeatable per seed | Tested | `harmony::harmonies_turn_only_the_hue`, `harmony::shuffle_is_repeatable_and_varied` |
| A space's theme and a profile's theme are stored and survive restart | Tested + Observed | `workspace` round trip carries `theme`; `settings::a_profile_theme_is_kept_until_it_follows_the_app_again`; relaunch kept Home's triadic gradient |
| Space over profile over app: each window paints with its nearest theme | Observed | Home (own theme) showed its gradient while new Space 2 showed the app theme; a profile preview reached Space 2 only |
| The editor previews live and Cancel restores the previous look | Observed | Triadic and Tweed strength 6 appeared in the window before Save; profile Solid preview vanished on Cancel |
| Textures (grain, sand, denim, tweed) draw over the backdrop | Observed | Tweed at strength 6 visible as a weave in a 2× crop of the sidebar |
| Library files round-trip, replace by name, and get safe file names | Tested | `library::themes_round_trip_through_files_and_replace_by_name`, `library::file_names_are_safe_and_never_empty` |
| Malformed or newer theme files are refused with the reason | Tested + Observed | `library::imports_refuse_bad_files_with_the_reason`; live import of `{"density":"roomy"}` showed a sheet with the reason; an unknown value now names the allowed ones (`recipe` test) |
| A good file imports into `<data>/Themes` and can be used for the space at once | Observed | open panel import of Glass: `theme imported`, `Themes/Glass.aeritheme`, sheet "Use for This Space" applied it |
| The macOS material shows the desktop through a blurred, tinted window | Observed | Glass (85%) over a light page behind the window showed through at the window's upper right; overlays stay opaque |
| One keymap drives pages and Aeri's windows; conflicts refuse, replace or swap; typing and system keys cannot be taken; storage drops bad entries | Tested | `keymap` tests (5): defaults distinct, chord parsing and labels, refuse/replace/swap, reserved and modifier-less keys, reset and JSON round trip |
| Settings (⌘,) shows appearance, themes, layout and keyboard | Observed | ⌘, opened the window; Appearance pane ticks Follow Theme and Standard density |
| Rebind, conflict, swap, reset, persistence | Observed | New Tab to ⌘K (`keys changed`), then ⌘W showed "⌘W already runs Close Tab." and Swap stored `{"close-tab":"cmd-k","new-tab":"cmd-w"}` in app.db; Reset All stored `{}` |
| A rebound key works inside a page | Observed | after New Tab = ⌘K, ⌘K in the page opened an `about:blank` tab (CDP target list) |

## Residual risks

- Pages follow only an app-wide colour scheme unless CEF allows per-browser
  emulation (design §3.7); per-space page scheme may not ship.
- Window translucency uses GPUI's blurred window background (the whole
  frame window, tinted by the theme); sliding chrome stays opaque.
- Proxy window buttons lack AppKit's tiling menu on the green button; the
  Window menu still has it.
- With nothing pinned the page reaches the window edge, so the inner few
  points of the edge resize from outside the window only.
