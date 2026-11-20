# Phase 10 — Passwords

- playbook: feature · risk: high (handles secrets) · size: M
- Intent: each profile has a password provider (Aeri's basic store, a
  password-manager extension, or none). Sign-ins are saved after a submit
  and filled only on the Person's pick, on the site they were saved for,
  after the system confirms it is them.
- Owner direction (2026-11-20): foundations and working functions first;
  plain look. Owner decision (2026-11-20, S6): Aeri's basic store is
  Chrome's password manager (ADR 0009).

## Spike S6 (2026-11-20)

Observed in Aeri (CEF 156), sign-in page `http://127.0.0.1:8767/login`
posting to a page that redirects:

- Chrome's "Save password?" bubble (Never / No Thanks / Save) appears after
  the submit, at the page's top right, with no visible toolbar. Save writes
  `Login Data` in the profile's Chromium directory.
- On the next visit the sign-in was filled as the page loaded.
- Chrome then offered "Protect passwords with your screen lock"; with
  `password_manager.biometric_authentication_filling` on, nothing fills on
  load; a click on the field lists the sign-in (and Manage Passwords…);
  picking it shows macOS's "Aeri is trying to fill your password on
  127.0.0.1:8767" (Touch ID or password). Not answered (harness rule).
- A second profile's page offered nothing; its `Login Data` held 0 rows
  while the first held 1.
- `chrome://password-manager/passwords` loads in a tab ("Password
  Manager").
- Chrome's switches for "fill only on pick" (`FillOnAccountSelect`) are not
  in this build; the screen-lock preference is the only lever.

## Decisions

1. **Provider per profile** (`aeri-core::credentials::PasswordProvider`),
   stored in `profile.db` (migration 6, `settings`): Aeri (default),
   Extension, Off. The system's password manager is not offered until
   the entitlement exists (blueprint owner decision 2).
2. **Chrome's preferences follow the provider**
   (`credentials::chrome_password_prefs`), set when the profile is ready —
   before any page opens — and on every change: Chrome's manager saves only
   for Aeri; screen lock before filling always; no Credential Management
   auto sign-in; no leak checks (they send credential hashes to Google).
   Set through `ProfileContext::set_bool_preference` (CEF's C API, see
   Phase 9).
3. **Choosing and managing**: the profile menu lists the three providers
   (the profile's checked) and **Manage Passwords…**; the palette has
   **Manage Passwords…**. Both open `chrome://password-manager/passwords` in
   a new tab of the profile, where sign-ins are viewed (after the screen
   lock), edited, deleted, imported and exported.
4. Save and fill UI is Chrome's own (bubble, field list, macOS prompt);
   Phase 12 decides whether any of it is restyled.

## Definition of Done (feature, high)

| Gate | Item | Evidence |
| --- | --- | --- |
| discover | Purpose, constraints, consumers understood | Stage 1 Credentials rules; blueprint owner decisions; this file |
| discover | Reality checked before design | S6 above |
| discover | Design documented; stage-1 change approved | ADR 0009; owner decision 2026-11-20 |
| prove | Behaviour tests | provider ↔ preferences rule; provider keys; provider default and persistence |
| prove | Existing tests pass | `cargo test --workspace` |
| prove | Security boundaries | Profiles see only their own sign-ins; nothing fills on load; filling asks the system; no leak checks |
| prove | End to end | Live: save, revisit, pick list, provider switch; `cargo xtask isolation` |
| prove | Evidence ledger complete | Below |
| prove | Docs updated | Roadmap, development doc, project memory, blueprint |

## Change budget

| Part | Budget (lines) |
| --- | --- |
| core (`credentials`, migration 6, settings, tests) | ~150 |
| cef (`set_bool_preference`) | ~20 |
| shell (provider apply, profile menu, palette, Manage Passwords) | ~150 |
| docs (ADR 0009, blueprint, spec) | — |

## Checkpoints

1. S6 spike and owner decision.
2. Core: provider, preference rule, settings table, tests.
3. Shell: apply at ready and on change; menu; palette.
4. Verification and docs.

## Status

| Checkpoint | State |
| --- | --- |
| 1 | done |
| 2 | done |
| 3 | done |
| 4 | done |

## Evidence ledger

| Claim | Label | Evidence |
| --- | --- | --- |
| Only Aeri's store turns Chrome's manager on; fill always waits; no auto sign-in, no leak checks | Tested | `credentials` tests |
| Provider defaults to Aeri and persists | Tested | `profile_db::settings` test; `cargo test --workspace` 59 + 3 + 2 passed |
| The preferences are applied before pages open | Observed | Fresh profile log "password provider applied … provider=aeri" before the first page; its `Preferences`: `credentials_enable_service` per provider, `biometric_authentication_filling: true`, `credentials_enable_autosignin: false`, `password_manager_leak_detection: false` |
| Save after submit | Observed | Fresh profile: submit `bob` → "Save password?" bubble → Save → `Login Data` holds origin `http://127.0.0.1:8767/login`, user `bob` |
| Nothing fills on load; a click lists the sign-in | Observed | Revisit: both fields empty; click on the user field: list with "bob" and Manage Passwords… |
| Picking asks the system before filling | Observed | Spike: macOS "Aeri is trying to fill your password on 127.0.0.1:8767"; fields stayed empty (prompt not answered) |
| Profiles see only their own sign-ins | Observed | Spike: profile B's page offered nothing; B's `Login Data` 0 rows, A's 1 |
| Provider switch stops saving | Observed | Profile menu → Use a Password-Manager Extension: log "password provider chosen … extension"; submitting `carol` showed no bubble and saved nothing; `profile.db` `settings` holds `password_provider` = `extension` |
| Saved sign-ins stay offered after switching away | Observed | With the extension provider, a click on the field still listed "bob" |
| Manage Passwords opens Chrome's page | Observed | Profile menu → Manage Passwords… opened `chrome://password-manager/passwords` in a new tab |
| Profiles stay isolated | Tested | `cargo xtask isolation`: 13 checks, 0 failed |
| Change size within budget (~320) | Observed | ~300 lines: new files 210 (`credentials.rs`, `profile_db/settings.rs`, `shell/passwords.rs`, tests included), +88 / −26 in existing code |

## Residual risks

- Switching to an extension or none does not hide sign-ins saved before;
  they are offered on a field click until deleted (Manage Passwords…).
- Chrome offers sign-ins on plain HTTP pages and same-site frames of the
  saved site (Inferred); stage 1 asks for secure, top-level pages.
- Without Touch ID or a Mac password Chrome's fill confirmation is
  untested (Inferred).
- Secrets live in Chrome's encrypted `Login Data`, key in the Keychain
  (ADR 0009), not as Keychain items.
