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
| 2 | Address in the shown tab (click the live row), ⌘K tab finder with ⌘ held | Driven live |
| 3 | Carrying tabs (held row follows the pointer, others make room), two-finger space swipe | Driven live |

## Status

| Slice | State |
| --- | --- |
| 1 | in progress |
| 2 | pending |
| 3 | pending |

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

## Residual risks

- GPUI cannot scale a view: rows arrive by growing in height and fading,
  not by Search's 0.94 scale.
- Window buttons, the frame's own as well as the standalone ones, drew
  grey in every window capture, even with Aeri in front; the key-state
  hook (`mac/lights.rs`) is in, but coloured buttons were not seen.
