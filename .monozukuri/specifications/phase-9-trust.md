# Phase 9 — Trust: permissions, certificates, popups, downloads

- playbook: feature · risk: high (security-sensitive defaults) · size: M
- Intent: permission prompts are per site and per profile, attached to their
  tab. Certificate warnings are never passed automatically and a proceed
  lasts only until quit. Popups need a Person's action, become tabs, and a
  blocked one is reported with a way to open it. Downloads land, cleaned and
  never overwriting, in the download folder, and the Person sees their
  progress. A profile's data can be cleared. Any page can be handed to the
  system browser (sites that refuse Aeri's lack of DRM).
- Owner direction (2026-11-20): foundations and working functions first; the
  look is plain and polish waits for Phase 12.

## What Chromium already does here (spike, 2026-11-20)

Observed in Aeri's Chrome-style tab windows, before any Phase 9 code:

- **Permission prompts** are Chrome's own bubble (Allow while visiting / Allow
  this time / Never allow), shown as a child of the tab's window; it hides
  with its tab and comes back when the tab is shown again. Decisions are
  Chrome content settings of the profile's request context (per profile).
- **Certificate errors** show Chrome's interstitial ("Your connection is not
  private", Back to safety / Advanced → Proceed).
- **Certificate exceptions survive a restart**: after Proceed, quit and
  relaunch, the page loaded without a warning; the profile's `Preferences`
  holds `ssl_cert_decisions`. This breaks stage 1's "until quit".
- **Popups without a gesture are blocked silently** by Chrome's popup
  blocker before `OnBeforePopup` runs (page saw `window.open` return null;
  no Aeri log line, no tab). With no omnibox there is no blocked-popup icon,
  so the Person is never told. Gesture popups reach `OnBeforePopup` and
  become tabs (Phase 3).
- **Downloads** go to `~/Downloads` with Chrome's own name cleaning
  (`../../.profile` → `_.._.profile`) and no visible progress (no toolbar,
  no download bubble). Chrome keeps a per-profile download list
  (`chrome://downloads`).
- **Chrome records its own history** (the profile's `History` database) next
  to Aeri's `profile.db` history: clearing one leaves the other.
- `chrome://settings/clearBrowserData` and the other settings pages work in a
  tab, per profile, with time ranges.

## Decisions

1. **Permissions stay Chrome's prompts** (they already attach to the tab and
   are per profile). The tab menu gets **Site Settings…**, which opens
   `chrome://settings/content/siteDetails?site=<origin>` in a new tab of the
   same profile, where a decision can be reset to Ask.
2. **Certificate exceptions are cleared** for each profile when its request
   context initializes, and the profile is reported ready only after the
   clear completes, so no page of it loads first. Aeri never sets
   `ignore_certificate_errors` and never answers certificate errors itself:
   passing a warning always takes the Person's click on Chrome's page.
3. **Popups are Aeri's decision.** Each profile's default popup setting is
   set to Allow, so every popup reaches `OnBeforePopup`, where Aeri allows it
   only if it came with a Person's gesture or the opener's site is on the
   profile's always-allowed list (`profile.db` migration 4). A blocked popup
   is reported on its window: the strip shows **Popup blocked** while the
   tab that asked is shown; its menu offers **Open <address>** (once, as a
   tab next to the opener) and **Always Allow Popups from <site>**. The tab
   menu offers **Block Popups from <site>** for an allowed site. The
   decision is a pure function in `aeri-core::trust`.
4. **Downloads are placed by Aeri.** `OnBeforeDownload` saves to the download
   folder (`~/Downloads`; `AERI_DOWNLOAD_DIR` overrides it for tests) under a
   name from `aeri-core::trust::download_path`: path separators and
   characters macOS forbids are replaced, a name that is empty or only dots
   becomes `download`, the result never leaves the folder, and a name that
   exists (or is being downloaded, or has a `.crdownload` partial) gets
   ` (1)`, ` (2)`, … before its extension. Progress comes from
   `OnDownloadUpdated`. The strip shows a **Downloads** button once the
   window's profile has downloaded something this session, with the
   progress of running downloads; its menu cancels a running download,
   reveals a finished one in Finder, and **Show All Downloads** opens
   `chrome://downloads` (Chrome's per-profile list) in a new tab.
5. **Two histories, both clearable.** The plan was to turn Chrome's own
   history recording off (`history.saving_disabled`) so `profile.db` held the
   only history; CEF refuses that preference (`SetPreference` returns false
   with no error text although `HasPreference` and `CanSetPreference` are
   true, both at context initialization and after the certificate clear —
   Observed), so Chrome's `History` database keeps recording. The library's
   History gets **Clear History…** with ranges (last hour, last 24 hours,
   last 7 days, last 4 weeks, all time) for Aeri's history, and its menu's
   **Cookies, Cache and Site Data…** — like **Clear Browsing Data…** in the
   palette — opens `chrome://settings/clearBrowserData` in a new tab of the
   profile, which clears Chrome's history, cookies, site data, cache, site
   settings and the downloads list.
6. **Open in Default Browser** (tab menu and palette) hands the shown tab's
   address to the system's default browser, or Safari if Aeri is the
   default. Only the address crosses.
7. The strip's Popup blocked and Downloads buttons are plain text buttons;
   Phase 12 styles them.

## Definition of Done (feature, high)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage 1 Trust rules, flows 12–14, edge cases |
| discover | Reality checked before design | Spike above |
| discover | Design documented | Decisions above |
| prove | Behaviour and failure-case tests | `download_path` (separators, `..`, forbidden characters, empty and dot names, collisions, partials, stays in folder); popup decision; site key; history clearing by range; popup allow list |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | Security boundaries | Certificate exception gone after relaunch; popup without gesture blocked and reported; download names cannot escape the folder |
| prove | End to end | `cargo xtask isolation`; live checks of each decision |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc, project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (`trust`: download path, popup decision, site key; migration 4; history clearing; tests) | ~350 |
| cef (download handler, popup gate, context set-up: popups default, certificate clear) | ~250 |
| shell (popup notices, downloads state and menu, commands, clear history, default browser) | ~350 |
| strip buttons, library buttons, palette entries | ~150 |

## Checkpoints

1. Core: `trust` module, migration 4, history clearing, tests.
2. Context set-up: certificate exceptions cleared before ready, popups
   default Allow (Chrome history off: refused by CEF, see decision 5).
3. Popups: gate in `OnBeforePopup`, blocked notice, open once / always allow
   / block.
4. Downloads: handler, path, progress signals, strip button and menu.
5. Clearing and hand-off: Clear History…, Clear Browsing Data…, Site
   Settings…, Open in Default Browser.
6. Verification and docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done (history off dropped: CEF refuses the preference) |
| 3 | done |
| 4 | done |
| 5 | done |
| 6 | done |

Change size: 1,306 lines added, 55 removed in code (23 files), against
~1,100 budgeted (+19%): the shell part (`shell/trust.rs`, 427 lines with
tests) grew with the menus' wording and the per-window notice plumbing.

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Download names are cleaned, confined and numbered | Tested | `aeri-core::trust` tests (separators, `..`, forbidden characters, empty and dot names, collisions, `.crdownload`, 255-byte limit) |
| Popup decision and site key | Tested | `trust::popup_allowed`, `trust::site_of` tests |
| Always-allowed popup sites persist; history clears by range | Tested | `profile_db` sites and `clear_history_since` tests; `cargo test --workspace` 56 + 3 + 2 passed |
| A popup without a gesture is blocked, logged and reported | Observed | Live: log "popup without a gesture blocked"; strip showed **Popup blocked**; menu listed Open <address> and Always Allow Popups from http://127.0.0.1:8766 |
| Open once, Always Allow, Block | Observed | Open made a tab at the address; Always Allow opened it, wrote the site to `popup_sites`, and a reload's non-gesture popup then opened ("auto opened"); tab menu **Block Popups from http://127.0.0.1:8766** removed the row |
| One click opens one popup | Observed | `multi.html` calling `window.open` three times on one click: first became a tab, the other two were blocked ("opened 1") |
| Downloads land cleaned and never overwrite | Observed | With `AERI_DOWNLOAD_DIR=/tmp/p9dl`: `report.txt`, `report (1).txt`, `_.._.profile` (from `../../.profile`) |
| Downloads show progress and a menu | Observed | Strip ↓ button; menu showed each file "in Finder" and **Show All Downloads**, which opened `chrome://downloads` listing all three |
| Certificate proceed lasts until quit | Observed | Proceeded past the self-signed `https://127.0.0.1:8443` warning (page "Target"), quit, relaunched: the restored tab showed "Privacy error" again |
| cef-rs's download handler default refuses every download | Observed | Before `can_download` returned 1, link clicks to `/file` produced no `OnBeforeDownload`; after, downloads ran |
| Site Settings, Clear Browsing Data | Observed | Palette **Site Settings…** opened `chrome://settings/content/siteDetails?site=https%3A%2F%2F127.0.0.1%3A8443`; library **Cookies, Cache and Site Data…** opened `chrome://settings/clearBrowserData` |
| Clear History by range | Observed | Library **Clear History… → Last Hour** removed 7 recent rows and kept a seeded 2-day-old entry, which the list still showed |
| Open in Default Browser | Observed | Palette command logged "page handed to the system browser" for the 8443 page; Google Chrome came to the front |
| Chrome's history stays on | Observed | `history.saving_disabled` refused (see decision 5); Chrome's `History` gained 7 URLs in the run |
| Profiles stay isolated | Tested | `cargo xtask isolation`: 13 checks, 0 failed |
| Location requests trigger macOS's own location prompt for Aeri | Observed | Spike: a page asking for geolocation raised the TCC prompt; left unanswered |

## Residual risks

- Chrome's `History` database records every visit next to `profile.db`;
  clearing Aeri's history does not clear it (Clear Browsing Data does).
- Chrome's own **Pop-ups and redirects** setting is shown in Site Settings
  as "Allow (default)". If the Person sets a site to Block there, Chrome
  blocks that site's non-gesture popups silently again, before Aeri sees
  them; Aeri's always-allowed list cannot override it.
- A page asking for location, camera or microphone makes macOS ask whether
  Aeri may use them (once per app) before Chrome's per-site prompt.
