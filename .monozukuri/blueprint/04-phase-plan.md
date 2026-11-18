# Phase plan — Aeri

Status: **approved** 2026-11-18 (owner). Phase 1 is on hold while the owner's
request to reconsider WKWebView (macOS) and WebView2 (Windows) is researched
(`docs/research/wkwebview.md`, `docs/research/webview2.md`). If the engine
changes, ADRs 0001/0002/0005 and phases 1–4 and 14 are revised and
re-approved.
Inputs: stages 1–3 (approved), ADRs 0001–0005 in `docs/decisions/`.

Sizes: S (one concern, few files) · M (one boundary) · L (several modules or
a data model). Spikes S1–S7 are defined in `02-tech-stack.md`. Each phase's
Definition of Done comes from its playbook and risk tier when it starts. Its
task list is written then, not here.

## Phase 1: Browse one page (vertical slice)
- id: 1
- intent: A signed dev build of Aeri opens a page-first window: a frameless
  Chrome-style CEF window with a GPUI address bar in an attached child window.
  It browses a real HTTPS site with keyboard, scroll, resize, full screen,
  error and crash states. It quits with no orphan processes.
- affected:
  - Cargo workspace: `apps/aeri`, `apps/aeri-helper`, `crates/aeri-core`
    (engine seam, tab state machine, `app.db` with one profile),
    `crates/aeri-cef` (CefWindow, one request context at
    `Chromium/<profile-id>`, external pump, lifecycle).
  - `scripts/` (bundle and ad-hoc signing), `.github/workflows/` (build
    and test on macOS arm64).
- prereqs: none (external: cef-rs 156.x, CEF stable binaries, GPUI pinned
  zed commit, Xcode command-line tools)
- playbook: feature
- risk: high — proves ADR 0002 (S1: layering, focus hand-off, resize, full
  screen, swapping between two browser views) and S3 (run loop, clean
  shutdown). If S1 fails, stop and bring options C/A to the owner.
- size: M

## Phase 2: Install extensions from the Chrome Web Store
- id: 2
- intent: In a non-default profile, the Person installs uBlock Origin Lite,
  Bitwarden and a dark-mode extension from their store pages. Extensions
  persist across restart. Their buttons and popups work in an Aeri window
  without a Chrome toolbar.
- affected: `crates/aeri-cef` (extension UI placement, task costs),
  `apps/aeri` (extension buttons in the GPUI chrome)
- prereqs: 1
- playbook: feature
- risk: high — S2, the crux of the extension requirement; failure reopens
  ADR 0002
- size: S

## Phase 3: Background tabs behave
- id: 3
- intent: Tabs not on screen are created lazily and slowed when hidden, and
  Aeri knows when a tab is playing audio or capturing.
- affected: `crates/aeri-cef` (visibility, audio and capture signals),
  `crates/aeri-core` (tab visibility state)
- prereqs: 1
- playbook: feature
- risk: medium — S7; if throttling or the audio signal is missing, the
  stage-1 resource rules are revisited with the owner
- size: S

## Phase 4: Built-in ad and tracker blocking
- id: 4
- intent: Blocking is on by default with uBO's lists downloaded at first
  launch. It covers network and cosmetic filtering, can be switched off per
  site, and has measured per-request cost.
- affected: `crates/aeri-cef` (`blocking`, `OnBeforeResourceLoad`),
  `apps/aeri-helper` (cosmetic injection), `crates/aeri-core` (filter-list
  metadata, site settings), `Blocking/` cache
- prereqs: 1
- playbook: feature
- risk: medium — S5; licence attribution in the UI
- size: M

## Phase 5: Profiles
- id: 5
- intent: The Person creates, renames, switches, deletes and opens profiles,
  one window per profile. Two profiles stay signed in to the same site with
  different accounts. Dormant profiles release their resources.
- affected: `crates/aeri-core` (`identity`, per-profile `profile.db`,
  deletion), `crates/aeri-cef` (request context per profile, release),
  `apps/aeri` (profile switcher, profile manager window), isolation
  integration tests on local test sites
- prereqs: 1
- playbook: feature
- risk: high — isolation is a security boundary; S4 measures per-profile
  cost and runs a network audit
- size: M

## Phase 6: Tabs and session
- id: 6
- intent: Multiple tabs in a sidebar that slides over the page or stays
  pinned. Covers open, close, reopen, duplicate, reorder and pin. The session
  restores on relaunch with only visible tabs loading, and history is
  recorded.
- affected: `crates/aeri-core` (`workspace`, `continuity`, history table),
  `crates/aeri-cef` (browser-view swap, navigation entries),
  `apps/aeri` (sidebar, tab context menus)
- prereqs: 1, 3, 5
- playbook: feature
- risk: medium
- size: L (spaces and favourites split out to phase 7)

## Phase 7: Spaces and favourites
- id: 7
- intent: The Person organises each profile into spaces with colours and
  icons, moves tabs between them, and keeps favourites visible in every space.
- affected: `crates/aeri-core` (`workspace` spaces and favourites),
  `apps/aeri` (space switcher, drag and drop)
- prereqs: 6
- playbook: feature
- risk: medium
- size: M

## Phase 8: Address bar, library and command palette
- id: 8
- intent: The address bar classifies input as an address or a search and
  suggests only from local history, bookmarks and tabs. Bookmarks are
  available, and the command palette reaches tabs, history, bookmarks, spaces,
  profiles and commands.
- affected: `crates/aeri-core` (`browsing::omnibox`, `library`),
  `apps/aeri` (address bar, palette, bookmarks panel)
- prereqs: 6, 7
- playbook: feature
- risk: medium
- size: M

## Phase 9: Trust: permissions, certificates, popups, downloads
- id: 9
- intent: Permission prompts are per site and per profile, attached to their
  tab. Certificate warnings are never auto-bypassed and are cleared at quit.
  Popups need a gesture and become tabs. Downloads are confined and
  collision-safe. Data can be cleared per profile. DRM-refusing sites offer
  "Open in system browser".
- affected: `crates/aeri-cef` (permission, certificate, popup and download
  handlers), `crates/aeri-core` (`trust`, `library::clear`), `apps/aeri`
  (downloads panel, clear-data dialog)
- prereqs: 6
- playbook: feature
- risk: high — security-sensitive defaults
- size: M

## Phase 10: Passwords
- id: 10
- intent: Each profile has a password provider (Aeri's basic store in the OS
  keychain, an extension, or none). Sign-ins are saved after submit and filled
  only on pick, on the matching secure site, after OS confirmation.
- affected: `crates/aeri-core` (`credentials`, keystore), `apps/aeri-helper`
  (form detection and fill) or Chrome's manager per S6, `apps/aeri`
  (password settings)
- prereqs: 5, 9
- playbook: feature
- risk: high — handles secrets; S6 decides the save/fill mechanism, and the
  owner is consulted if it changes the stage-1 invariant
- size: M

## Phase 11: Resource stewardship
- id: 11
- intent: Unseen tabs rest after the Person's idle period, with the stage-1
  exclusions. Memory pressure rests least-recently-shown tabs first. Keep-awake
  works per site. A resource view shows the cost of each tab, extension and
  profile.
- affected: `crates/aeri-core` (`resources::rest`), `crates/aeri-cef`
  (close/recreate, task manager), `apps/aeri` (resource view, memory-pressure
  source)
- prereqs: 3, 6
- playbook: feature
- risk: medium
- size: M

## Phase 12: Appearance and customisation
- id: 12
- intent: Tabs on the side or top, sidebar left or right, a hiding sidebar,
  and full-screen chrome hiding. Light/dark/system, density, accents and space
  colours. Theme import/export (look only). Every command can have a shortcut,
  and conflicts are refused.
- affected: `crates/aeri-core` (`appearance`), `apps/aeri` (`theme`,
  `shell::keymap`, settings window)
- prereqs: 7, 8
- playbook: feature
- risk: low
- size: M

## Phase 13: Benchmarks and optimisation
- id: 13
- intent: A repeatable harness compares Aeri with Helium and Chrome on the
  handoff §8.3 scenarios, on recorded hardware. Measured hot spots are fixed,
  and results are published.
- affected: `benchmarks/`, any module the measurements implicate
- prereqs: 4, 5, 11, 12
- playbook: feature
- risk: medium
- size: M

## Phase 14: Self-built CEF with H.264/AAC
- id: 14
- intent: Aeri ships on a CEF built with `proprietary_codecs=true
  ffmpeg_branding=Chrome` from a reproducible pipeline, and H.264/AAC sites
  play.
- affected: `scripts/` (CEF build), CI (self-hosted or large runner),
  `crates/aeri-cef` (binary source), release notes
- prereqs: 13; external: build machine (owner, decided at phase start),
  legal advice on Via LA AVC/AAC licensing
- playbook: migration
- risk: high — changes the engine binaries every later release depends on
- size: L

## Phase 15: Public macOS beta
- id: 15
- intent: A notarised, signed Aeri with a secure update channel, a completed
  security review, third-party notices, and README/CONTRIBUTING/SECURITY
  docs, released to beta users.
- affected: `scripts/sign-macos.sh`, update mechanism, `docs/`,
  `.github/workflows/`, repository root docs
- prereqs: 2, 4, 8, 9, 10, 14; external: paid Apple Developer membership
  (individual is enough for notarisation)
- playbook: release
- risk: critical
- size: M

## Phase 16: Windows
- id: 16
- intent: The core browsing experience runs on Windows: profiles, tabs,
  extensions, blocking and passwords (Credential Manager), from Windows CI.
- affected: `apps/aeri` (Windows glue), `crates/aeri-cef` (Windows window
  model; spike repeating S1 for `HWND`), packaging, CI
- prereqs: 15; external: Windows test machine, code-signing certificate
- playbook: feature
- risk: high
- size: L (split when it starts: probe, then port)

## Deferred until an organisation Apple account exists
- iCloud passkeys and Apple Passwords (entitlement).
- Widevine VMP signing (Google agreement plus castLabs audit; ADR 0005).

## Phase order rationale

- **Phase 1** is the thinnest path through every boundary: GPUI, the
  child-window layering, the engine seam, CEF, the request context, SQLite
  and signing. It retires the two biggest architectural unknowns (S1, S3)
  before anything is built on them. If it fails, little is thrown away.
- **Phases 2–4** are small probes-with-outcomes, each depending only on
  phase 1. Phase 2 goes first: it tests the extension promise that ADR 0002
  exists for. Phase 3 (S7) decides whether two stage-1 resource rules hold.
  Phase 4 (S5) measures blocking cost while the codebase is still small.
- **Phase 5** comes before tabs because profile isolation is a security
  boundary and defines where every later piece of data lives (`profile.db`).
  Building tabs first would mean migrating them into profiles later.
- **Phases 6–8** are the large workspace and library work. They come after
  the probes, smallest first within their dependencies (spaces after tabs,
  palette after both).
- **Phases 9–10** handle security-sensitive behaviour once there are real
  tabs and profiles to test against. Passwords follow trust, because they
  need secure-origin and top-level checks.
- **Phases 11–12** polish resource use and customisation on a complete
  model.
- **Phase 13** measures before the expensive engine change, so the codec
  build is compared against a known baseline.
- **Phase 14** is the largest and least reversible infrastructure change. It
  goes late, as the rules require, but before release.
- **Phase 15** releases. **Phase 16** follows the handoff: Windows after the
  macOS beta.
- **Before phase 1:** the handoff's Phase-0 documents (`docs/architecture/*`,
  `docs/roadmap.md`, `docs/development.md`) are written from these blueprint
  artifacts at handoff.
