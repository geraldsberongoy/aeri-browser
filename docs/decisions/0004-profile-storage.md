# ADR 0004: Profile isolation and storage layout

- Status: accepted
- Date: 2026-11-18
- Deciders: owner (kuya-egg); principal engineer (agent)

## Context

Profiles are isolated identities. Website storage, Aeri's own records and
sign-ins must never cross between profiles. The only exception is sign-ins
in the system password manager.

Many profiles run inside one app process tree. Deleting a profile removes
all of its data. Profile ids are stable and never derived from display
names.

CEF constraints (`docs/research/cef.md` §5):

- A request context's `cache_path` must equal, or be a direct child of,
  `root_cache_path`.
- Each disk context is a full Chrome `Profile`.
- Different profiles never share a renderer process.
- A process-singleton lock is held on the root path.

## Options considered

1. **One `CefRequestContext` per profile, with all Aeri data in SQLite.**
   - Layout: each request context at `Chromium/<profile-id>/`. Aeri data
     lives in `app.db` plus `Profiles/<profile-id>/profile.db`.
   - Pros: isolation follows Chromium's own profile boundary, and deletion
     removes two directories plus Keychain items.
   - Cons: we can't read Chromium's own history or bookmarks, so Aeri
     records its own (Chromium may duplicate history internally).
2. **The handoff's split: `history.db`, `bookmarks.db`, `session.db` per
   profile.**
   - Pros: separation by concern.
   - Cons: three files to migrate, back up and delete per profile, and no
     transactions across them (closing a tab writes both session and
     closed-tab data).
3. **Using the primary context (root path) for the first profile.**
   - Pros: it is CEF's default.
   - Cons: the first profile would be special. Deleting it means deleting
     the root.

## Decision

Option 1, and no user profile ever uses the root context.

Paths are under `~/Library/Application Support/Aeri/` on macOS and
`%LOCALAPPDATA%\Aeri\` on Windows:

```
app.db                        global: profiles, appearance, themes, shortcuts, list metadata
Profiles/<profile-id>/profile.db   spaces, tabs, trails, closed tabs, history,
                                   bookmarks, site settings, downloads, provider choice
Chromium/                     CEF root_cache_path (unused as a profile)
Chromium/<profile-id>/        that profile's Chromium storage and extensions
Blocking/                     filter lists and engine cache (global)
```

Rules:

- Profile ids are UUID v7.
- Secrets in Aeri's basic store are Keychain or Credential Manager items
  keyed by profile id, and never stored in SQLite.
- Migrations are numbered, keyed on `user_version`, and run when the
  database opens.

## Why this one

It has the fewest files and gives transactional updates within a profile.
Isolation equals Chromium's process and storage boundary, and deletion is
mechanical.

## Tradeoffs and consequences

- **Per-profile cost.** Each profile is a full Chrome `Profile`, so "each
  extra profile costs little" must be measured (spike S4). Dormant profiles
  release their request context.
- **Same site in several profiles.** It runs in separate renderers by
  design, which is the price of isolation.
- **Duplicate history.** Chromium may keep its own history inside
  `Chromium/<id>/`. S4 checks whether that can be switched off. It is
  deleted with the profile either way.
- **Second launch.** A second app launch must forward to the running
  instance (`OnAlreadyRunningAppRelaunch`).

## Outcome (Phase 5, S4)

- Per-profile cost is measured: about 275 MB per extra profile with one
  window, most of it the window's GPU memory and its renderers. Dormant
  profiles are unloaded: their processes exit and Chromium closes every file
  in `Chromium/<id>/`; about 11 MB each stays in the browser process.
  Numbers: [profile-isolation.md](../architecture/profile-isolation.md).
- Deletion is deferred, not immediate: the row leaves `app.db` at once and
  the directories are removed after CEF shuts down (at quit, or at the next
  launch), so Chromium never runs on a profile being deleted. Observed: a
  deleted profile's directories are gone after the next launch and Chromium
  does not recreate them.
- Second launch: Chromium's process singleton hands it to the running Aeri
  (Observed); the second process logs "cef::initialize failed" and exits.
- Duplicate history: not switched off yet; it is deleted with the profile.
