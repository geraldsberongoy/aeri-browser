# Logic → stack map — Aeri

Status: **proposed** (awaiting owner approval).
Inputs: `01-business-logic.md` (approved), `02-tech-stack.md` (approved,
codecs/DRM pending).

Every stage-1 element maps to exactly one *owning* home. "Owning" means the
place whose code or data decides the behaviour; other layers may carry it.

Abbreviations:

| Prefix | Meaning |
| --- | --- |
| `core::` | `crates/aeri-core` module (no GPUI, no CEF; unit-testable) |
| `cef::` | `crates/aeri-cef` module (the only code that touches CEF) |
| `app::` | `apps/aeri` (GPUI views, macOS glue) |
| `helper::` | `apps/aeri-helper` (CEF renderer-process hooks) |
| **Chromium** | Chrome-style Chromium machinery, configured through `cef::` and not written by us |
| `app.db.` / `profile.db.` | SQLite tables (ADR 0004) |
| *(S#)* | depends on that spike passing |

## Actors

| Business element | Type | Home in stack | Notes |
| --- | --- | --- | --- |
| Person | actor | `app::` (all input enters through GPUI views, menus, shortcuts) | Chrome UI surfaces (install, permission, passkey prompts) also receive Person input |
| Website | actor | Chromium renderer processes, sandboxed | Aeri exposes no JS bridge; `helper::` injects only CSS and filter scripts |
| Extension | actor | Chromium extension system, per request context | Aeri never grants extra APIs |
| Web engine | actor | `cef::` (the engine seam implementation) | `core::engine` defines the commands and events |
| Operating system | actor | `app::platform` (appearance, memory pressure, menus, file dialogs, notifications) | Secrets via `core::credentials` → keyring crates |

## Domains

| Business element | Type | Home in stack | Notes |
| --- | --- | --- | --- |
| Identity | domain | `core::identity` | |
| Workspace | domain | `core::workspace` | |
| Browsing | domain | `core::browsing` | |
| Trust | domain | `core::trust` | Enforcement of policies is split with Chromium; see rules |
| Credentials | domain | `core::credentials` | |
| Extensions | domain | Chromium extension system | `cef::extensions` positions its UI and reads task costs |
| Library | domain | `core::library` | |
| Continuity | domain | `core::continuity` | |
| Resource stewardship | domain | `core::resources` | Mechanisms live in `cef::` (rest) and Chromium (throttling) |
| Appearance & customisation | domain | `core::appearance` | Rendering in `app::theme` |
| Shell | domain | `app::shell` | Windows, address bar, palette, menus, keymap |

## Entities

| Business element | Type | Home in stack | Notes |
| --- | --- | --- | --- |
| Profile | entity | `app.db.profiles` | `core::identity::Profile`; UUID v7 id |
| Window | entity | `core::identity::WindowRecord` (session row `profile.db.windows`) | On screen: `cef::window` (CefWindow) + `app::shell` child windows *(S1)* |
| Space | entity | `profile.db.spaces` | |
| Favourite | entity | `profile.db.tabs` (`slot = favourite`) | A browser view moving between windows of one profile is unverified *(S1)* |
| Tab | entity | `profile.db.tabs` | Live part: `cef::` browser handle keyed by tab id |
| Navigation entry | entity | `profile.db.nav_entries` | Filled from `cef::` navigation-entry events |
| Closed-tab record | entity | `profile.db.closed_tabs` | |
| History entry | entity | `profile.db.history` | Chromium may also keep its own copy *(S4)* |
| Bookmark | entity | `profile.db.bookmarks` (folder tree via `parent_id`, `position`) | |
| Saved sign-in | entity | Provider-dependent: Keychain / Credential Manager item (basic store), Apple Passwords, or an extension | Metadata (site, username, last used) in `profile.db.signins`; never the secret |
| Password provider | entity | `profile.db.settings.password_provider` | |
| Site permission | entity | Chromium content settings of the profile's request context | Read and cleared through `cef::permissions`; prompts are Chrome UI |
| Certificate exception | entity | Chromium (in-memory decision of the request context) | `cef::` clears exceptions at start-up to enforce "until quit" |
| Site setting | entity | `profile.db.site_settings` | Blocking on/off, keep-awake |
| Installed extension | entity | Chromium extension registry under `Chromium/<profile-id>/` | |
| Filter list | entity | `app.db.filter_lists` + files in `Blocking/` | |
| Download | entity | `profile.db.downloads` | Events from `cef::downloads` |
| Session | entity | `profile.db` (windows, spaces, tabs, nav_entries) | No separate session store |
| Appearance | entity | `app.db.appearance` (+ accent in `profiles`, colour in `spaces`) | |
| Theme | entity | `app.db.themes` (JSON payload, schema-checked) | |
| Shortcut map | entity | `app.db.shortcuts` | Applied as GPUI keybindings and NSMenu key equivalents |

## Business rules

| Business element | Type | Home in stack | Notes |
| --- | --- | --- | --- |
| Stable profile id; name not empty; duplicates allowed | rule | `core::identity` | |
| Window belongs to one profile; switch = focus or open | rule | `core::identity` | |
| Website storage never crosses profiles | rule | Chromium (separate request contexts and renderers) | Verified by an isolation test *(S4)* |
| Aeri records never cross profiles | rule | `core::store` (one `profile.db` per profile) | |
| Window shows its profile identity | rule | `app::shell::sidebar` | Also the window title and accent |
| Delete profile: confirm, remove all | rule | `core::identity::delete_profile` | Removes `Profiles/<id>`, `Chromium/<id>`, keyring items |
| Open in another profile carries only the address | rule | `core::identity` | |
| Tab in exactly one space or favourites | rule | `core::workspace` | |
| Tab never moves across profiles | rule | `core::workspace` | |
| Ordering: favourites, pinned, regular | rule | `core::workspace` | |
| Favourite is one shared page | rule | `core::workspace` | Live view placement *(S1)* |
| Empty space allowed; delete with tabs confirms | rule | `core::workspace` | |
| Duplicate copies address and trail only | rule | `core::workspace` | Recreates the trail by navigating; CEF has no API to restore a trail |
| Address vs search classification | rule | `core::browsing::omnibox` | Pure function |
| Suggestions local only until Return | rule | `core::library::suggest` | No network calls from the address bar |
| Address bar never runs script | rule | `core::browsing::omnibox` | Rejects `javascript:` |
| Internal pages unreachable by websites | rule | `app::` (settings and profile manager are GPUI windows, not web pages) | Holds by construction |
| Load failure keeps the tab with retry | rule | Chromium error page + `core::browsing` state `Failed` | |
| Page crash affects only that page | rule | Chromium process model + `cef::` `OnRenderProcessTerminated` → `Crashed` | |
| Capabilities only after Person allows (per site, per profile) | rule | Chromium permission prompts (Chrome style) | Per profile via its content settings |
| Certificate warning stops; deliberate proceed; until quit | rule | Chromium interstitial | `cef::` start-up clear enforces "until quit" |
| Popups only from Person action; open as tabs | rule | Chromium popup blocker + `cef::` `OnBeforePopup` → `core::workspace` | |
| Blocking with uBO lists, on by default, per-site off | rule | `cef::blocking` (adblock engine on the IO thread) | Cosmetic part in `helper::cosmetic` *(S5)* |
| Download name cleaning, folder confinement, numbered suffix | rule | `core::trust::download_path` | Pure function |
| Websites can't reach Aeri functions | rule | `cef::` (no JS bindings, no message router exposed to pages) | |
| Install from Web Store with permission prompt | rule | Chromium (Chrome-style install flow) | *(S2)* |
| Genuine-package check | rule | Chromium (CRX3 verification) | |
| Updates needing more permissions are held | rule | Chromium extension system | |
| Extensions per profile | rule | Chromium (registry per profile) | |
| Extensions can't see sign-ins, profiles, settings | rule | Chromium extension sandbox + Aeri exposes nothing | |
| Disable, enable, pin, remove | rule | Chromium extension system | Pinning UI placement *(S2)* |
| Crashed extension stopped and reported | rule | Chromium | `cef::extensions` surfaces the notice |
| Extension cost visible | rule | `cef::resources` (`CefTaskManager`) | |
| One provider per profile; defaults | rule | `core::credentials` | |
| System passwords shared across profiles | rule | `core::credentials` (shows the notice) | Unavailable until an organisation account exists |
| Basic-store secrets in OS storage only | rule | `core::credentials::keystore` (keyring crates) | |
| Extension provider ⇒ Aeri silent | rule | `core::credentials` | |
| Offer to save after submit | rule | `helper::forms` (detects submit) → `core::credentials` | *(S6)* |
| Fill only on the matching secure top-level site, on pick | rule | `core::credentials` | Fill injected by `helper::forms` *(S6)* |
| OS confirmation; no caching of secrets | rule | `core::credentials` | `LAContext` / `UserConsentVerifier` via `app::platform` |
| Passkeys use system support | rule | Chromium WebAuthn | Limited until the entitlement exists |
| View, edit, delete basic-store sign-ins | rule | `app::settings::passwords` | |
| History per completed top-level visit | rule | `core::library` | Fed by `cef::` load-end events |
| Clear data per profile, by category and range | rule | `core::library::clear` | Chromium parts via `cef::` (`ClearCertificateExceptions`, cache and cookie APIs) |
| Session remembered on quit | rule | `core::continuity` | Written continuously, not only at quit |
| Offer restore after crash | rule | `core::continuity` | `app.db.clean_exit` flag |
| Quit only after pages shut down | rule | `cef::lifecycle` | *(S3)* |
| Unshown tabs load nothing | rule | `core::resources` | `cef::` creates the browser on first show |
| Extra profile costs little | rule | `core::resources` (dormant release) | Measured in *(S4)* |
| Background pages slowed | rule | Chromium (hidden or occluded view throttling) | *(S7)* |
| Rest policy and exclusions | rule | `core::resources::rest` | Signals: audio and capture from `cef::`, form input from `helper::forms` *(S7)* |
| Rest under memory pressure, LRU | rule | `core::resources::rest` | Signal from `app::platform` (dispatch memory-pressure source) |
| Resource view | rule | `app::resources_view` | Data from `cef::resources` |
| UI idle does no work | rule | GPUI (draws only when notified) | Measured in benchmarks |
| Dormant profile runs nothing | rule | `core::identity` → `cef::` releases the context | *(S4)* |
| Measured lightness vs Helium/Chrome | rule | `benchmarks/` harness | Owned by the Phase 5 benchmarks |
| Page-first, slide-in, full screen hides UI | rule | `app::shell` | *(S1)* |
| Layout and look choices | rule | `core::appearance` | |
| Every command bindable; conflicts refused | rule | `app::shell::keymap` | Validation logic in `core::appearance::shortcuts` |
| Themes import/export, look-only | rule | `core::appearance::theme` (schema validation) | |
| Reduced motion honoured | rule | `app::theme` (GPUI `reduce_motion`) | |
| No activity leaves the machine | rule | `cef::settings` (Chrome-style network services off) | Verified by a network audit *(S4)* |

## Primary flows

| Business element | Type | Home in stack | Notes |
| --- | --- | --- | --- |
| 1 First launch | flow | `app::startup` | |
| 2 Browse | flow | `app::shell::address_bar` → `core::browsing` → `cef::` | |
| 3 New tab | flow | `core::workspace` | |
| 4 Close and reopen | flow | `core::workspace` + `core::continuity` | |
| 5 Organise | flow | `core::workspace` | Drag and drop in `app::shell::sidebar` |
| 6 Spaces | flow | `core::workspace` | |
| 7 Create profile | flow | `core::identity` | |
| 8 Switch profile | flow | `core::identity` | |
| 9 Separate sign-ins | flow | Chromium request contexts | |
| 10 Open in other profile | flow | `core::identity` | |
| 11 Save a sign-in | flow | `core::credentials` | *(S6)* |
| 12 Permission | flow | Chromium prompt | |
| 13 Certificate warning | flow | Chromium interstitial | |
| 14 Download | flow | `cef::downloads` → `core::trust` | |
| 15 Command palette | flow | `app::shell::palette` | Queries `core::library` and `core::workspace` |
| 16 Rest and return | flow | `core::resources::rest` | |
| 17 See the cost | flow | `app::resources_view` | |
| 18 Install an extension | flow | Chromium (Chrome-style) | *(S2)* |
| 19 Extension update | flow | Chromium | |
| 20 Focus on the page | flow | `app::shell` | *(S1)* |
| 21 Customise | flow | `core::appearance` | |
| 22 Quit and restore | flow | `core::continuity` | Lifecycle via `cef::lifecycle` *(S3)* |
| 23 Delete profile | flow | `core::identity` | |

## State transitions

| Business element | Type | Home in stack | Notes |
| --- | --- | --- | --- |
| Tab load state (Unloaded…Closed) | state | `core::browsing::TabState` | Events from `cef::` |
| Tab visibility (Foreground/Background) | state | `core::workspace` | Drives `cef::` show and hide |
| Download states | state | `core::trust::DownloadState` | |
| Site permission states | state | Chromium content settings | |
| Saved sign-in states | state | `core::credentials` | Basic store only |
| Installed extension states | state | Chromium extension system | |
| Profile states (Active/Dormant/Deleted) | state | `core::identity::ProfileState` | |

## Invariants and edge cases

Each invariant is enforced in the home of the rule it protects (rows above).
Each stage-1 edge case becomes a test in that same module. Pure `core::`
cases are unit tests on an in-memory SQLite. Chromium-owned cases (popups,
certificates, isolation) are integration tests against local test pages
(handoff §11).

## Orphans

Stage-1 elements whose home exists but whose mechanism is not proven. None
lacks a home.

- **Background-page slowing and an audible-tab signal** — relies on
  Chromium throttling hidden Chrome-style views, and on a CEF callback for
  "playing audio" that has not been located yet. **S7** must confirm both.
  If either fails, rest becomes the only lever and the rule needs a stage-1
  revisit.
- **Basic-store save/fill** — `helper::forms` (form detection and fill)
  versus Chrome's own password manager inside Chrome-style CEF. **S6**
  decides. If Chrome's manager is used, its secrets sit in Chromium's
  encrypted database with the key in the Keychain, not as Keychain items.
  That conflicts with the stage-1 invariant and would need a stage-1 revisit.
- **Favourite live page across two windows** — whether a CEF browser view
  can move between two Views windows of the same profile. **S1** decides;
  the fallback is the stage-1 edge case's "showing in another window"
  placeholder.
- **Video codecs and DRM** — Chrome-compatible media depends on the pending
  codecs decision.

## Unused components

None. Every stage-2 component is the home of at least one element:

- `uuid` → identity
- `serde_json` → themes, shortcuts
- `tracing` → diagnostics
- `objc2` → `app::platform`
- keyring crates → credentials
- `adblock` → `cef::blocking`
- `aeri-helper` → cosmetic filtering and forms

## New spikes found while mapping

- **S6 Password save/fill in Chrome-style CEF:**
  - Does Chrome's built-in password manager (save bubble, fill) work without
    a Chrome toolbar?
  - Is a renderer-hook form fill reliable on 20 common sign-in pages?
- **S7 Background behaviour:**
  - Do hidden or occluded Chrome-style browser views throttle timers?
  - Which CEF callback reports "tab is playing audio"?
