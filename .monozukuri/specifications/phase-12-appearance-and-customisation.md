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
| 2–8 | pending |

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

## Residual risks

- Pages follow only an app-wide colour scheme unless CEF allows per-browser
  emulation (design §3.7); per-space page scheme may not ship.
- Window translucency behind GPUI and transient chrome over CEF child
  windows need spikes (slices 2 and 5).
