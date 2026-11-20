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
3. **Popups stay Chrome's decision; Aeri tells the Person.** (Revised after
   the first build, which set every profile's default popup setting to
   Allow and decided in `OnBeforePopup`: that likely also turned off the
   redirect blocking Chrome files under "Pop-ups and redirects" (Inferred),
   showed "Allow (default)" in Site Settings, and let a per-site Block there make
   Chrome refuse silently again.) Chrome's popup blocker refuses popups
   without a Person's gesture unless the site's popup setting is Allow;
   a default of Allow is reset at launch. The sites always allowed are
   Chrome's popup site settings, so Site Settings shows and changes the
   same choice (`profile.db` migration 5 drops migration 4's list). Each
   tab's own DevTools session has `Page` enabled; `Page.windowOpen` reports
   every `window.open`, including those Chrome refuses before any CEF
   callback, and Aeri applies Chrome's rule (`aeri-core::trust::popup_allowed`
   with the site setting) to tell a refused one. It is reported on its
   window: the strip shows **Popup blocked** while the tab that asked is
   shown; its menu offers **Open <address>** (once, as a tab next to the
   opener) and **Always Allow Popups from <site>**. The tab menu offers
   **Block Popups from <site>** for an allowed site (back to Chrome's
   default).
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
5. **One history.** Chrome's own history recording is off for every
   profile (`history.saving_disabled`), so Aeri's `profile.db` history is the
   only browsing history. cef-rs's `set_preference` returned false with no
   reason (with an empty error string and with none); the same call made
   directly through CEF's C API succeeds, so Aeri makes it that way. The
   library's History gets **Clear History…** with ranges (last hour, last
   24 hours, last 7 days, last 4 weeks, all time), and its menu's **Cookies,
   Cache and Site Data…** — like **Clear Browsing Data…** in the palette —
   opens `chrome://settings/clearBrowserData` in a new tab of the profile
   for cookies, site data, cache, site settings and the downloads list.
   Chrome's downloads list is kept: it lives in Chrome's `History` database
   but is not browsing history.
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
| core (`trust`: download path, popup decision, site key; migrations 4–5; history clearing; tests) | ~350 |
| cef (download handler, popup reports, context set-up: certificate clear, history off) | ~250 |
| shell (popup notices, downloads state and menu, commands, clear history, default browser) | ~350 |
| strip buttons, library buttons, palette entries | ~150 |

## Checkpoints

1. Core: `trust` module, migration 4, history clearing, tests.
2. Context set-up: certificate exceptions cleared before ready, Chrome
   history off, no allow-everything popup default.
3. Popups: refused ones reported from `Page.windowOpen`, blocked notice,
   open once / always allow / block through Chrome's site settings.
4. Downloads: handler, path, progress signals, strip button and menu.
5. Clearing and hand-off: Clear History…, Clear Browsing Data…, Site
   Settings…, Open in Default Browser.
6. Verification and docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done (history off revised: made through CEF's C API) |
| 3 | done |
| 4 | done |
| 5 | done |
| 6 | done |

Change size: 1,306 lines added, 55 removed in code (23 files), against
~1,100 budgeted (+19%): the shell part (`shell/trust.rs`, 427 lines with
tests) grew with the menus' wording and the per-window notice plumbing.
The revision (popups through Chrome, history off, usage descriptions)
removed the popup list and its table code and added the DevTools watch.

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Download names are cleaned, confined and numbered | Tested | `aeri-core::trust` tests (separators, `..`, forbidden characters, empty and dot names, collisions, `.crdownload`, 255-byte limit) |
| Popup decision and site key | Tested | `trust::popup_allowed`, `trust::site_of` tests |
| History clears by range | Tested | `clear_history_since` test; `cargo test --workspace` 55 + 3 + 2 passed |
| Chrome refuses a popup without a gesture; Aeri reports it | Observed | Revised build: log "default popup setting reset from Allow" (profile from the first build), then from `aeri_cef::window` "popup without a gesture blocked"; strip showed **Popup blocked**; menu listed Open <address> and Always Allow Popups from http://127.0.0.1:8766 |
| Open once, Always Allow, Block | Observed | Always Allow opened the refused popup; Chrome's site details for http://127.0.0.1:8766 then showed **Pop-ups and redirects: Allow**; a reload's non-gesture popup opened ("auto opened"); tab menu **Block Popups from http://127.0.0.1:8766** made the next reload's popup refused and reported ("auto blocked") |
| One click opens one popup | Observed | `multi.html` calling `window.open` three times on one click: first became a tab, the other two were reported blocked ("opened 1") |
| `Page.windowOpen` fires for popups Chrome refuses | Observed | Spike with Chrome's default: event with `userGesture: false` for `target.html?nogesture`, which never reached `OnBeforePopup` |
| Migration 5 drops the popup list | Observed | The Phase 9 profile's `profile.db` lists no `popup_sites` table after relaunch |
| Downloads land cleaned and never overwrite | Observed | With `AERI_DOWNLOAD_DIR=/tmp/p9dl`: `report.txt`, `report (1).txt`, `_.._.profile` (from `../../.profile`) |
| Downloads show progress and a menu | Observed | Strip ↓ button; menu showed each file "in Finder" and **Show All Downloads**, which opened `chrome://downloads` listing all three |
| Certificate proceed lasts until quit | Observed | Proceeded past the self-signed `https://127.0.0.1:8443` warning (page "Target"), quit, relaunched: the restored tab showed "Privacy error" again |
| cef-rs's download handler default refuses every download | Observed | Before `can_download` returned 1, link clicks to `/file` produced no `OnBeforeDownload`; after, downloads ran |
| Site Settings, Clear Browsing Data | Observed | Palette **Site Settings…** opened `chrome://settings/content/siteDetails?site=https%3A%2F%2F127.0.0.1%3A8443`; library **Cookies, Cache and Site Data…** opened `chrome://settings/clearBrowserData` |
| Clear History by range | Observed | Library **Clear History… → Last Hour** removed 7 recent rows and kept a seeded 2-day-old entry, which the list still showed |
| Open in Default Browser | Observed | Palette command logged "page handed to the system browser" for the 8443 page; Google Chrome came to the front |
| Chrome records no browsing history | Observed | Fresh profile: after visiting `dl.html?fresh` and `target.html?fresh2`, Chrome's `History` had 0 URLs and 1 download; `chrome://downloads` listed it; `Preferences` has `"saving_disabled":true` |
| cef-rs's `set_preference` fails where the C API succeeds | Observed | Wrapper: false with an empty error string and with none; direct call with a zeroed `cef_string_t`: true, and the preference persisted |
| Profiles stay isolated | Tested | `cargo xtask isolation`: 13 checks, 0 failed |
| Chrome asks the Person about a site before macOS asks about Aeri | Observed | Clicking a geolocation request showed Chrome's "wants to Know your location" bubble and no macOS prompt (dismissed: "geo err 1"). Why the spike saw the macOS prompt first is not known; it was still up at the next launch, before any page asked |
| macOS prompts explain themselves | Observed | Bundle `Info.plist` has camera, microphone, location, Bluetooth and passkey usage descriptions (cef-rs wrote "aeri"; location had none) |

## Residual risks

- Chrome's `:visited` link colours come from its history, which is now off
  (Inferred, not checked).
- `Page.windowOpen` covers the tab's own page and its same-process frames;
  a popup a cross-site frame tries without a gesture is still refused, but
  may go unreported (Inferred).
- Choosing "allow all sites" for popups in Chrome's settings is undone at
  the next launch.
