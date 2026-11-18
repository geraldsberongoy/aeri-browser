# Roadmap

Status: Phase 0 (architecture documents). Nothing below is built yet.

The detailed plan, with affected modules, prerequisites and playbooks, is
[`.monozukuri/blueprint/04-phase-plan.md`](../.monozukuri/blueprint/04-phase-plan.md).
This page is the short version. Each phase's task list is written when the
phase starts.

Risk levels: low, medium, high, critical. Sizes are in the detailed plan.

## Phases

| Id | Name | Outcome | Risk |
| --- | --- | --- | --- |
| 1 | Browse one page | A dev build opens a page-first window: a GPUI frame with one frameless Chrome-style CEF window per tab and a GPUI address bar, all as child windows. It browses HTTPS, handles resize, full screen, errors and crashes, and quits with no orphan processes. Proves S1 and S3. Built; S1 revised the layering (ADR 0002). | high |
| 2 | Chrome Web Store extensions | uBlock Origin Lite, Bitwarden and a dark-mode extension install from their store pages into a non-default profile, persist across restart, and show buttons and popups without a Chrome toolbar. Proves S2. Built: hidden Chrome toolbar, private DevTools pipe, Chrome-created tabs routed into Aeri tabs (ADR 0006). | high |
| 3 | Background tabs behave | Unseen tabs are created lazily and slowed when hidden. Aeri knows when a tab plays audio or captures. S7. Built: background link tabs have no window until shown, Chromium throttles hidden tabs, sound comes from Aeri's tab-signals extension (ADR 0007). | medium |
| 4 | Ad and tracker blocking | Through uBlock Origin Lite, not a built-in blocker (ADR 0008, superseding ADR 0003). A profile's first launch opens uBO Lite's store page once. Built. | low |
| 5 | Profiles | Create, rename, switch, delete and open profiles, one window each. Two profiles stay signed in to one site with different accounts. Dormant profiles release resources. S4. Built: a profile badge and native menu in the strip, a Profiles menu, a profile manager window; `cargo xtask isolation` passes on a local site; dormant profiles unload; Aeri keeps running with no windows. | high |
| 6 | Tabs and session | Sidebar tabs (slide-over or pinned): open, close, reopen, duplicate, reorder, pin. Session restores on relaunch with only visible tabs loading. History is recorded. | medium |
| 7 | Spaces and favourites | Spaces with colours and icons per profile, tab moves between spaces, favourites visible in every space. | medium |
| 8 | Address bar, library, palette | The address bar tells addresses from searches and suggests from local data only. It rests in the sidebar and floats over the page centre when called, like Zen's. Bookmarks. A command palette over tabs, history, bookmarks, spaces, profiles and commands. | medium |
| 9 | Trust | Per-site, per-profile permission prompts. Certificate warnings never auto-bypassed. Gesture-only popups. Confined, collision-safe downloads. Per-profile data clearing. "Open in system browser" for DRM-refusing sites. | high |
| 10 | Passwords | A password provider per profile (basic store in the OS keychain, an extension, or none). Save after submit; fill only on pick, on the matching secure site, after OS confirmation. S6. | high |
| 11 | Resource stewardship | Idle and memory-pressure rest, per-site keep-awake, and a resource view of each tab, extension and profile. | medium |
| 12 | Appearance and customisation | Tabs on side or top, sidebar placement, the address bar's floating behaviour (as in Zen), light/dark/system, accents, theme import/export, bindable shortcuts with conflict refusal. | low |
| 13 | Benchmarks and optimisation | A repeatable harness compares Aeri with Helium and Chrome on handoff §8.3 scenarios. Hot spots are fixed and results published. | medium |
| 14 | Self-built CEF with H.264/AAC | Aeri ships on a CEF built with `proprietary_codecs=true ffmpeg_branding=Chrome` from a reproducible pipeline. ADR 0005. | high |
| 15 | Public macOS beta | Notarised, signed build with a secure update channel, security review, third-party notices, and README, CONTRIBUTING and SECURITY docs. | critical |
| 16 | Windows | Core browsing on Windows: profiles, tabs, extensions, blocking, passwords. Starts with a probe that repeats S1 for `HWND`. | high |

Order rationale (from the detailed plan): Phase 1 retires the two largest
unknowns first. Phases 2–4 are small probes that depend only on Phase 1.
Profiles (5) come before tabs (6) because isolation decides where every
later piece of data lives.

## Spikes

| Spike | Question | Runs in |
| --- | --- | --- |
| S1 | Layering: GPUI frame, CEF tab windows and GPUI overlays as child windows. Slide-over, click-through, resize, full screen, focus, tab switching. Run in Phase 1: one browser view per Chrome-style window forced one window per tab; GPUI does not pass transparent clicks. | Phase 1 |
| S2 | Web Store: install uBO Lite and Bitwarden end to end in a non-default profile. Popups without a toolbar. Run in Phase 2: passed with Chrome's toolbar kept hidden and `Extensions.triggerAction` over a private pipe (ADR 0006). | Phase 2 |
| S3 | Run loop: CEF external pump inside GPUI. Live resize, menus, sheets, clean shutdown with N profiles. | Phase 1 |
| S4 | Profile cost and isolation: 1/2/5/10 request contexts, storage does not cross, dormant profiles return memory. Run in Phase 5: about 275 MB per extra profile with a window; dormant profiles unload (11 MB stays); an idle browser only contacts Chrome's component updater. | Phase 5 |
| S5 | Blocking hook: adblock-rust in `OnBeforeResourceLoad`. Dropped with the built-in blocker (ADR 0008). | — |
| S6 | Passwords: Chrome's own manager in Chrome-style CEF without a toolbar, or a renderer-hook save and fill. | Phase 10 |
| S7 | Background: hidden-view throttling and an audible-tab signal in Chrome-style CEF. Run in Phase 3: Chromium throttles ordered-out tabs on its own; CEF has no audible callback, so a bundled extension reports `chrome.tabs` audible over the private pipe (ADR 0007). | Phase 3 |

Pass criteria for S1 and S3 are in
[`architecture/gpui-cef-integration.md`](architecture/gpui-cef-integration.md).
S4 is in [`architecture/profile-isolation.md`](architecture/profile-isolation.md).
If S1 fails, stop and take options C and A to the owner
([ADR 0002](decisions/0002-rendering-strategy.md)).

## Deferred

Deferred until an organisation Apple Developer account exists
([02-tech-stack.md](../.monozukuri/blueprint/02-tech-stack.md), owner
decision 2):

- iCloud Keychain passkeys.
- Apple Passwords. Until then, passkeys work through password-manager
  extensions, phone sign-in and security keys.
- Widevine signing, which needs a Google agreement and a castLabs audit
  ([ADR 0005](decisions/0005-media-codecs-and-drm.md)).

Not planned for v1 (handoff §13 non-goals and the ADRs): a Chromium fork,
a built-in blocker (ADR 0008), sync.
