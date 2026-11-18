# Phase 5 — Profiles

- playbook: feature · risk: high · size: M
- Intent: the Person creates, renames, recolours, switches, deletes and
  opens profiles, each in its own windows. Two profiles stay signed in to
  one site as different accounts. A profile whose last window closes goes
  dormant and releases its request context.
- Proves: spike S4 (per-profile cost and isolation; network audit).
- Design: ADR 0004 (request context per profile, layout),
  `docs/architecture/profile-isolation.md`.

## Decisions taken in this phase (conservative defaults, owner may revisit)

1. **Aeri keeps running with no windows** (macOS convention, like Safari and
   Arc). Closing the last window makes its profile dormant; the Dock icon or
   the Profiles menu opens a window again. ⌘Q quits.
2. **Switching profiles uses native menus**: the strip shows the window's
   profile (accent-coloured monogram and name); clicking it opens a native
   menu like Safari's profile menu. The menu bar gets a Profiles menu with
   the same entries, so every command is reachable from the keyboard.
3. **Profile manager** is a GPUI window (never a web page): rename,
   recolour, open, delete, create.
4. **Avatar** is the name's first letter on the accent colour for now; a
   picture avatar is later polish.
5. **Deleting a profile** closes its windows and removes it from `app.db` at
   once, into a `deleted_profiles` list. Its directories are removed after
   CEF has shut down (at quit), and any left over are removed at the next
   launch before CEF starts. Chromium's profile is never deleted under it.
6. **`profile.db` is not created yet.** No per-profile Aeri records exist
   until Phase 6 (tabs, session, history). Deletion already removes
   `Profiles/<id>/`, so Phase 6 only adds the file.
7. **"Open in another profile"** needs a link and tab context menu, which
   comes with the sidebar (Phase 6); the rule (address only) is unchanged.
8. **The last-used profile** opens at launch (`profiles.last_used_at`, set
   when one of its windows becomes key).

## Definition of Done (feature, high)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | This file; stage 1 Identity rules; ADR 0004 |
| discover | Reuse-first search | CEF `RequestContext` per profile (already in `ProfileContext`), GPUI menus, `Window::prompt`, `Application::on_reopen` |
| discover | Design documented | Decisions above; profile-isolation.md updated |
| prove | Behaviour and failure-case tests | `store` unit tests (rename, blank names, delete refusing the last, deleted list, last used, accent suggestion); `cargo xtask isolation` end to end |
| prove | Security: isolation checks | Isolation test table below, against a local test site |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | Evidence ledger complete | Below |
| prove | Change budget respected | Below |
| prove | Docs updated | profile-isolation.md (S4 outcome), roadmap, development.md, project memory |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (store: rename, accent, delete, deleted list, last used) | ~150 |
| cef adapter (release, profile ids) | ~40 |
| shell (profiles map, windows per profile, dormancy, delete, menus) | ~350 |
| UI (strip badge, native menu, manager window) | ~450 |
| xtask isolation harness (local server, CDP) | ~450 |

Stop and re-plan at roughly twice that.

## Checkpoints

1. **Store.** Migration 3: `profiles.last_used_at`, `deleted_profiles`.
   `rename_profile`, `set_accent`, `delete_profile` (refuses the last one),
   `deleted_profiles`/`forget_deleted`, `mark_used`, `suggest_accent`.
2. **Many profiles in the shell.** Windows carry their profile; each open
   profile has its own `ProfileContext`, extension watch and pending
   windows. Startup opens the last-used profile (or each `AERI_PROFILE`
   name, comma-separated, for development and tests).
3. **Dormancy.** When a profile's last window is gone its context and watch
   are dropped. Check: its page and extension targets disappear and its
   renderers exit.
4. **UI.** Strip monogram + name; native profile menu; Profiles menu in the
   menu bar; profile manager window (create, rename, recolour, open,
   delete with a confirmation naming the profile).
5. **Deletion.** As decided above. Check: after quit, `Chromium/<id>/` and
   `Profiles/<id>/` are gone and the other profile is unchanged.
6. **S4 and isolation.** `cargo xtask isolation` runs the bundled app on two
   profiles against a local server; S4 numbers for 1, 2, 5 and 10 profiles;
   network audit of an idle browser.

## Isolation tests (local server, two profiles A and B)

| Test | Pass condition |
| --- | --- |
| Cookies | A sets a cookie; B sends none; A keeps it after restart |
| Local storage and IndexedDB | A writes; B reads nothing |
| HTTP cache | A caches a resource; B's request reaches the server |
| Two sessions | A and B stay signed in as different users, also after restart |
| Permissions | A granted; B still `prompt` |
| Renderers | Same site in A and B: separate renderer processes |
| Dormancy | After A's last window closes, A has no targets left |
| Deletion | After deleting A and quitting, its directories are gone; B unchanged |

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done |
| 4 | done; rename, recolour and delete with confirmation not yet driven live (owner became active) |
| 5 | done (start-up removal Observed; removal at quit by the same code path) |
| 6 | done; permissions check moved to Phase 9 |

Change budget: about 1,450 lines against 1,440 planned. The shell went over
(about 540 against 350: many-profile plumbing plus orderly quit on signals)
and the core under.

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Rename, recolour, last used, accent suggestion, deleting refuses the last profile, deleted data removed only by the sweep | Tested | `store` tests (24 in `aeri-core`) |
| Two profiles keep sign-ins, cookies, local storage, IndexedDB and HTTP cache apart, also after restart | Tested | `cargo xtask isolation`: 11/11 pass |
| A profile whose last window closes goes dormant: no targets within 0.3 s, files closed | Observed | Isolation run; `lsof` on the browser process, 103 → 0 files under `Chromium/<id>/` |
| SIGTERM lost cookies before this phase: Chromium's own handler exits at once | Observed | First isolation run: "quit requested by signal", exit 135 ms later, no "quitting", cookies gone after restart; fixed by replacing the handlers |
| S4 cost table (1/2/5/10 profiles), wake-up 13–60 ms, about 55 ms each when opened together | Observed | `footprint` per process type; `profile ready ms=` log lines |
| An idle browser contacts only `update.googleapis.com` | Observed | Net log, 3 min on `about:blank`, after `--disable-features=AimEnabled,PreconnectToSearch,SearchEnginePreconnect2,NetworkTimeServiceQuerying`; before: `www.google.com/async/folae` (AI-mode eligibility) and a `www.google.com` preconnect |
| DevTools cannot grant permissions in a profile's context | Observed | `Browser.grantPermissions`: "Failed to find browser context" |
| Strip badge shows avatar and name; native profile menu lists profiles with the current one checked; Profiles menu in the menu bar lists them | Observed | Screenshots; System Events menu listing |
| Creating a profile from the manager opens its window, cascaded, with the uBO Lite offer | Observed | Typed "Work" + Return; log "profile created", window "Work" at +28 pt |
| Launch opens the last-used profile | Observed | Relaunch opened "Work", later "Personal" after it was in front at quit |
| A deleted profile's directories go at the next launch; Chromium does not recreate them | Observed | Row moved to `deleted_profiles` with Aeri stopped; after launch both directories gone, list empty |
| A pop-up `NSMenu` on macOS 27 does not draw item images | Observed | Images set (16 × 16), never shown; menu rows are text only |
| While the profile menu is open the shell handles no messages | Observed | DevTools probe: no "Chrome-created tab" line until Escape |
| Rename, recolour, delete with confirmation, Dock reopen | Not run | Owner switched to another app mid-check; to verify |
