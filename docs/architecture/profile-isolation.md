# Profile isolation

Status: built in Phase 5; S4 measured (below). Decision: [ADR 0004](../decisions/0004-profile-storage.md).
Evidence: [`cef.md` §5, §6](../research/cef.md). Requirements:
[01-business-logic.md](../../.monozukuri/blueprint/01-business-logic.md).
Isolation is a security boundary. It is built in Phase 5 and measured by
spike S4.

## Profile model

A profile is an isolated identity. It owns:

- a stable id (UUID v7), a display name, an avatar and an accent colour;
- Chromium storage: cookies, HTTP cache, local storage, IndexedDB, site
  permissions and installed extensions;
- Aeri's records: spaces, tabs, navigation trails, closed tabs, history,
  bookmarks, site settings, downloads and the password-provider choice;
- its windows. A window belongs to exactly one profile. A tab never moves to
  another profile.

Names are labels. Duplicates are allowed. Paths and keys use the id only.
Opening a link in another profile carries the address and nothing else.

States: Active (has a window), Dormant (no window, data kept), Deleted.

## Request context per profile

Each profile is one `CefRequestContext` with a disk cache path. Rules from
CEF ([cef.md §5](../research/cef.md)):

| Rule | Consequence for Aeri |
| --- | --- |
| `cache_path` must be absolute and equal to, or a direct child of, `root_cache_path`. Otherwise CEF logs an error and falls back to an off-the-record profile. | Every profile path is `Chromium/<profile-id>/`. Never nest deeper. |
| A `cache_path` equal to the root is the default (primary) profile. | No user profile uses the root. `Chromium/` itself holds no profile. |
| Each disk context is a full Chrome `Profile`. | Per-profile cost is a real cost. S4 measures it. |
| A process-singleton lock is held on the root path. | Only one Aeri app process runs. A second launch forwards to it (`OnAlreadyRunningAppRelaunch`). Aeri's own data lives outside `Chromium/`. |
| Different profiles never share a renderer process. | The same site in two profiles uses two renderers. This is the price of isolation. |

Permission handling is per `CefClient`, not per request context. Aeri's
client must key permission decisions by the browser's profile
([cef.md §5](../research/cef.md), Observed).

## Directory layout

Root: `~/Library/Application Support/Aeri/` (Windows: `%LOCALAPPDATA%\Aeri\`).

```
app.db                              global records (ADR 0004)
Profiles/<profile-id>/profile.db    that profile's Aeri records
Chromium/                           CEF root_cache_path, not a profile
Chromium/<profile-id>/              that profile's Chromium storage and extensions
```

`app.db` holds the profile list (with each profile's one-time uBO Lite offer),
appearance, themes, shortcuts and window placement. Migrations are numbered SQL, keyed on
`user_version`, applied when a database opens. All database work runs off the
UI thread.

## What is isolated and what is not

| Item | Isolated per profile? | Where it lives |
| --- | --- | --- |
| Cookies, cache, local storage, IndexedDB | Yes | `Chromium/<id>/` (Chromium partitions by browser context) |
| Site permissions, certificate exceptions | Yes | Request context content settings; exceptions are in memory and cleared at start-up |
| Installed extensions and their data | Yes | `Chromium/<id>/`. The same extension in two profiles is two installs. |
| Spaces, tabs, history, bookmarks, downloads, site settings | Yes | `Profiles/<id>/profile.db` |
| Basic-store sign-ins | Yes | Keychain or Credential Manager items keyed by profile id. Never in SQLite. |
| Renderer processes | Yes | Chromium rule |
| Browser, GPU, network and storage processes | **No** (shared) | One tree. S4 confirms process counts. |
| Profile list, themes, shortcuts, appearance | **No** (global) | `app.db` |
| Ad and tracker blocking | **Yes** | uBlock Origin Lite is an extension, so each profile has its own copy and settings ([ADR 0008](../decisions/0008-blocking-through-ublock-origin-lite.md)). |
| The system password manager | **No** (exception) | See below |

## The system-password-manager exception

Sign-ins kept in the system's own password manager belong to the Person's
system account, not to a profile. Two profiles that both choose it see the
same sign-ins. Aeri shows a notice when a profile selects it.

- Apple Passwords needs the Apple browser-passkey entitlement, which needs an
  organisation account. Until then it is unavailable, and the default is
  Aeri's basic store (owner decisions 2 and 3,
  [02-tech-stack.md](../../.monozukuri/blueprint/02-tech-stack.md);
  [blocking-and-credentials.md Q4](../research/blocking-and-credentials.md)).
- On Windows the default is Aeri's basic store in Credential Manager.
- This is the only data that crosses profiles.

## Extension scoping

- Extensions are installed into one profile and run in that profile's
  request context. Chrome's extension registry is per profile
  ([cef.md §4, §5](../research/cef.md)).
- Extension processes are separate per extension and per profile (Inferred,
  [cef.md §5](../research/cef.md)).
- Extensions cannot see saved sign-ins, other profiles or Aeri's settings.
  Aeri exposes no JavaScript bridge and no message router to pages
  ([03-logic-to-stack-map.md](../../.monozukuri/blueprint/03-logic-to-stack-map.md)).
- Password-manager extensions are available from day one. If a profile
  chooses an extension as its provider, Aeri's own store stays silent there.

## Dormant profiles

When a profile's last window closes, it becomes Dormant. Aeri releases its
request context. A dormant profile runs no pages and no extensions. Opening it,
switching to it or opening a link in it makes it Active again. Aeri keeps
running with no windows open; the Dock icon or the Profiles menu opens one.

Release lets Chromium unload the profile (S4, Observed): within 0.3 s it has
no DevTools targets left (pages and its extension service worker), its
renderers exit, and the browser process closes every file under
`Chromium/<id>/` (103 open files before, 0 after). About 11 MB per dormant
profile stays in the browser process (S4 table).

Chromium keeps one startup profile loaded for the whole run, normally the
last one whose window was active (`profile.last_used` in `Local State`).
For Aeri that was an Aeri profile, which then never unloaded: its extension
service worker outlived its last window (Observed in Phase 6, every time the
harness closed that profile). Aeri starts Chromium with
`--profile-directory=Default`, an empty profile no window uses, so every
Aeri profile can go dormant. `Default` costs what it cost before on a fresh
data directory, where it was already the startup profile; it runs Aeri's
tab-signals worker and nothing else.

## Deletion

Deleting a profile needs confirmation naming it. Then Aeri:

1. removes its row from `app.db` and records the id in `deleted_profiles`;
2. closes the profile's windows and browsers, which releases its request
   context (if no window of another profile is left, the last-used profile
   opens);
3. once CEF has shut down (at quit), deletes `Profiles/<id>/` and
   `Chromium/<id>/`; anything left over (a crash, a locked file) is deleted
   at the next launch before CEF starts;
4. (Phase 10) deletes the profile's Keychain or Credential Manager items.

Directories are never deleted while Chromium runs, so Chromium can never
write into a profile being deleted. Chromium's own history copy inside
`Chromium/<id>/` goes with step 3 ([ADR 0004](../decisions/0004-profile-storage.md)).
Deletion is terminal. The id is never reused.

## Security notes

- **A request context is not an OS sandbox.** Isolation comes from
  Chromium's process and storage boundaries, not from the operating system
  separating profiles. Aeri must not widen those boundaries
  ([handoff §4.4](../handoff.md)).
- All profiles run inside one app process (the CEF browser process). A bug
  there can reach every profile's data. Renderers, which run web content,
  are sandboxed ([cef.md §8](../research/cef.md)).
- Aeri's records are separate files per profile, so a query in one profile
  cannot read another's database.
- Secrets are never stored in SQLite. They stay in OS-protected storage.
- Settings and profile-manager screens are GPUI windows, not web pages, so
  websites cannot reach them.
- Chrome services that call Google while Aeri sits idle are turned off
  (`--disable-features`: AI-mode eligibility and search preconnects for
  Chrome's own omnibox, network time). The S4 network audit (an idle
  browser on `about:blank`, net log, 3 minutes) then shows one host:
  `update.googleapis.com`, Chrome's component updater, which delivers
  certificate revocation lists and other security components. Safe
  Browsing list updates (`safebrowsing.googleapis.com`) also stay on; they
  send no browsing data. Both are kept on purpose.

## Isolation tests and S4 results

`cargo xtask isolation` runs the bundled app with two profiles, A and B,
against a local test site it serves itself, drives each profile's page over
the DevTools protocol, quits (SIGTERM, handled as an orderly quit),
relaunches and checks again. No check depends on an external site.

| Test | Pass condition | Result |
| --- | --- | --- |
| Two sessions | A and B stay signed in to one local site as different users, also after restart | pass |
| Cookies | A sets a cookie; B sends none; A keeps it after restart | pass |
| Local storage and IndexedDB | A writes values; B reads none, also after restart | pass |
| HTTP cache | A caches a resource; B's request reaches the server | pass |
| Dormancy | After B's last window closes, B has no targets; A keeps running | pass |
| Permissions | A grants a permission; B is asked again | Phase 9: DevTools cannot grant in a profile's context |
| Extensions | An extension installed in A is absent in B | Phase 2 (installed in one profile only) |
| Aeri records | A's history and bookmarks never appear in B | Phase 6/8 (no records yet) |
| Deletion | After deleting A and quitting, its directories are gone; B unchanged | unit test + manual check (spec) |
| Renderers | The same site in A and B runs in separate renderers | Chromium rule; S4 counts two renderers per profile |

**S4: cost per profile** (Observed). One window per profile, each showing
the same local page; memory is `phys_footprint` summed per process type;
profiles had been opened once before (warm).

| Profiles | Processes | Total MB | Browser | GPU | Page renderers | Extension renderers |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 9 | 490 | 104 | 157 | 3 × (122 MB) | 2 × (63 MB) |
| 2 | 12 | 775 | 151 | 269 | 5 × (215 MB) | 3 × (94 MB) |
| 5 | 21 | 1600 | 276 | 581 | 11 × (505 MB) | 6 × (188 MB) |
| 10 | 36 | 2958 | 491 | 1106 | 21 × (970 MB) | 11 × (335 MB) |

- Each extra profile with one window costs about 275 MB: about 105 MB of GPU
  memory for its window, 45 MB in the browser process, two renderers and one
  extension renderer (Aeri's tab-signals worker, about 30 MB). The network
  and storage services are shared (about 55 MB in all).
- Waking a profile takes 13–60 ms when it is the only one; profiles opened
  together initialise one after another, about 55 ms each (10 profiles: the
  last is ready after 0.8 s).
- After 9 of 10 profiles went dormant: 9 processes and 600 MB, against 490 MB
  for one profile alone.
- Not measured yet: switching Chromium's own history off. Its copy is
  deleted with the profile.
