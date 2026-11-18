# Profile isolation

Status: Phase 0. Decision: [ADR 0004](../decisions/0004-profile-storage.md).
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
switching to it or opening a link in it makes it Active again.

Release lets Chromium's `ProfileManager` unload the profile. Whether memory
returns to baseline is **unmeasured** ([cef.md §5](../research/cef.md)). S4
measures it.

## Deletion

Deleting a profile needs confirmation. Then Aeri:

1. closes the profile's windows and browsers;
2. releases its request context;
3. deletes `Profiles/<id>/`;
4. deletes `Chromium/<id>/`;
5. deletes the profile's Keychain or Credential Manager items;
6. removes its row from `app.db`.

Chromium may keep its own history copy inside `Chromium/<id>/`. Step 4
removes it either way ([ADR 0004](../decisions/0004-profile-storage.md)).
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
- Chrome-style network services that send data to Google are turned off.
  The claim "nothing leaves the machine" is verified by a network audit in
  S4 (not yet run).

## Isolation test plan

Spike S4 (Phase 5) plus automated integration tests on local test sites
(handoff §11). No test depends on an external site.

**S4 measurements** with 1, 2, 5 and 10 request contexts, one page on the
same site in each:

- memory per process type (`CefTaskManager` and `ps`) and process counts;
- startup time per new profile;
- memory after a profile goes dormant;
- whether Chromium's own history can be switched off;
- a network audit of an idle browser with no page open.

**Integration tests** against a local test server on two profiles, A and B:

| Test | Pass condition |
| --- | --- |
| Cookies | A sets a cookie. B sends none. A keeps it after restart. |
| Local storage and IndexedDB | A writes values. B reads none. |
| HTTP cache | A caches a resource. B's request reaches the server. |
| Same account, two sessions | A and B stay signed in to one local site as different users. |
| Permissions | A grants a permission. B is asked again. |
| Extensions | An extension installed in A is absent in B. |
| Aeri records | A's history and bookmarks never appear in B. |
| Deletion | After deleting A, `Profiles/<A>/` and `Chromium/<A>/` do not exist and B is unchanged. |
| Renderers | The same site in A and B runs in separate renderer processes. |
| Dormancy | After A's last window closes, A has no running processes. |
