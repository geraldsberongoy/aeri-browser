# ADR 0009: Aeri's basic password store is Chrome's password manager

- Status: accepted
- Date: 2026-11-20
- Deciders: owner (2026-11-20), principal engineer (agent)
- Spike: S6 (Phase 10)

## Context

Stage 1 asks for a per-profile password provider: the system's password
manager (deferred until Apple grants the browser entitlement, blueprint
owner decision 2), Aeri's basic store, a password-manager extension, or
none. It also says sign-ins are saved only after a submit, with save /
never / not now; filled only when the Person picks one, on the site it
was saved for; and handed over only after the system confirms it is them.
The basic store was to keep each sign-in as its own Keychain item.

S6 asked whether Chrome's own password manager works in Aeri's
Chrome-style tab windows without a visible toolbar, or whether Aeri needs
its own form detection and fill in the renderer helper. Observed in Aeri
(CEF 156):

- After a sign-in form is submitted and the page navigates, Chrome shows
  its "Save password?" bubble (Never / No Thanks / Save) at the page's top
  right. Save writes the sign-in to the profile's `Login Data` database,
  encrypted with a key Chrome keeps in the Keychain ("Aeri Safe Storage").
- Out of the box, a saved sign-in fills as the page loads.
- With Chrome's "use your screen lock" setting
  (`password_manager.biometric_authentication_filling`), nothing fills on
  load; a click on the field lists the saved sign-ins, and picking one
  shows macOS's "Aeri is trying to fill your password on <site>" (Touch ID
  or the Mac password) before anything is filled.
- Another profile sees none of these sign-ins (its own `Login Data`).
- `chrome://password-manager` works in a tab, per profile.

## Options

1. **Chrome's password manager as the basic store.** About 400 lines:
   provider choice, preferences, menus. Proven on real sites. Secrets sit
   in Chrome's encrypted database with only the key in the Keychain.
2. **Aeri's own store**: form detection and fill through `aeri-helper`,
   each sign-in a Keychain item. About 2,000+ lines, to be proven on
   ~20 real sign-in pages; more fragile on real sites.
3. **Extensions only**, no basic store for now.

## Decision

Option 1 (owner, 2026-11-20). Aeri's basic store is Chrome's password
manager of the profile:

- A profile's provider (`profile.db` `settings.password_provider`): Aeri
  (default), a password-manager extension, or none.
- Before any page of the profile opens, and whenever the provider changes,
  Aeri sets Chrome's preferences (`aeri-core::credentials`):
  `credentials_enable_service` on only for Aeri's store; screen lock before
  filling always on; Credential Management auto sign-in off; leak checks
  (which send credential hashes to Google) off.
- The Person views, edits and deletes sign-ins on Chrome's password
  manager page (**Manage Passwords…**), which asks for the screen lock to
  reveal one.

## Consequences

- Stage 1's "kept in the system's protected secret storage" becomes
  "encrypted by Chrome with a key kept there". The blueprint is revised.
- Turning Chrome's manager off for an extension or none stops saving, but
  sign-ins saved before stay offered when the Person clicks a field, until
  deleted on the password manager page (Observed). Stage 1's "Aeri offers
  nothing itself" holds only for new sign-ins.
- Chrome offers saved sign-ins on any page of the same site, including
  plain HTTP and same-site frames; stage 1's "secure connection,
  top-level page" is narrower (Inferred from Chrome's documented
  behaviour, not checked).
- A Mac without a password or Touch ID cannot confirm; Chrome's behaviour
  then is not known (Inferred risk).
- Importing and exporting passwords come with Chrome's page.
