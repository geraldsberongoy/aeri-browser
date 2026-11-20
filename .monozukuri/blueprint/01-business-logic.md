# Business logic — Aeri

Status: **approved** 2026-11-18 (owner). Open questions below keep their
stated defaults.

Source of framed intent: `docs/handoff.md` (acts as the nemawashi brief) plus
the Person-owner's stage-1 answers (2026-11-18).
Labels: **Decided** = stated by the owner; **Assumed** = default chosen here,
confirm or correct at approval.

## Overview

Aeri is a desktop web browser for people who keep several separate browsing
lives — work, personal, a client, a side project — open at the same time and
want each one to stay separate, organised and cheap to keep running. It loads
ordinary websites exactly as Chrome does, keeps each browsing identity's
sign-ins, history and saved data apart, and groups open pages into named
spaces that survive restarts. It works entirely on the Person's own machine:
no account, no cloud service, no activity collection.

What it must feel like (Decided, owner's words distilled):

- **Compatible like Chrome** — sites behave as they do in Chrome, and Chrome
  Web Store extensions install and work. (Decided: extensions are required for
  the first usable release)
- **Polished like Search and Arc** — quiet, minimal, nothing in the way.
- **Faster and lighter than Helium** — measurably less memory and processor
  use for the same pages, and several profiles open at once without the
  machine bogging down.
- **Customisable like Zen** — the Person shapes layout, look and shortcuts.
- **Page-first like Zen, Search and Arc** — the page fills the window edge to
  edge; the browser's own interface stays out of the way until called.
- **No hidden compromises** — lightness comes from not doing work nobody sees,
  never from breaking sites or weakening safety.

Reference products (for behaviour and feel, not to copy assets): Search
(driceroland/Search), Zen (zen-browser/desktop), Helium (imputnet/helium), Arc,
Chrome.

In scope for the first releases: one person on one computer; Mac first, then
Windows with the same behaviour. "Multiple instances" means many profiles and
windows open at once inside one running Aeri, each extra profile adding
little cost (Decided).

Planned for after the first releases (Decided): split view, link peek,
floating video, hide-page-elements-for-good.

Out of scope (Decided): AI assistants, accounts, sync of any kind, mobile, a
custom page renderer or engine fork, Aeri's own extension marketplace, VPN,
ads, automation agents, reading mode. A full password manager (vaults,
sharing, generators, breach alerts) is out; basic save-and-fill is in.

## Actors and roles

- **Person** — the single human using the browser. May create, rename and
  delete profiles and spaces; open, arrange and close tabs; customise the
  browser; grant or refuse site permissions; accept or refuse risky actions;
  save and use passwords; clear their data. Holds every privilege; there are
  no other human roles.
- **Website** — any page loaded in a tab. Untrusted. May request permissions,
  open new windows, start downloads, offer sign-in forms, and ask to navigate.
  May never read or change Aeri's own state, another profile's data, saved
  passwords it was not explicitly given, or anything the Person has not
  granted.
- **Extension** — third-party code the Person installs into a profile from the
  Chrome Web Store. Semi-trusted: may do exactly what the Person approved at
  install (read or change certain sites, block requests, show a popup, store
  its own data) and nothing more. Never reaches another profile, saved
  sign-ins, or Aeri's own settings.
- **Web engine** — the external component that fetches, runs and draws
  websites. Aeri tells it what to load, which profile's storage to use, and
  when a page may rest; it reports progress, titles, icons, errors, crashes,
  resource use and requests back. Aeri does not decide how pages render.
- **Operating system** — supplies windows, menus, file locations, protected
  secret storage, notifications, memory-pressure signals, power state and
  appearance (light/dark, reduced motion). Aeri adapts to it; it is not a
  decision-maker.

## Domains

- **Identity** — profiles: who the Person is being in a window, and the wall
  between identities.
- **Workspace** — spaces, tabs, favourites, pinning and ordering inside a
  profile.
- **Browsing** — navigating a tab, its back/forward trail, loading, failure
  and crash.
- **Trust** — site permissions, certificate warnings, downloads, popups,
  content blocking, and keeping websites away from privileged functions.
- **Credentials** — saving and offering sign-ins per profile.
- **Extensions** — installing, permitting, running, updating and removing
  Chrome extensions per profile.
- **Library** — history and bookmarks.
- **Continuity** — remembering everything above across quit, restart and
  crash.
- **Resource stewardship** — keeping memory, processor and energy use low:
  what runs, what rests, what is never started, and showing the Person what
  costs what.
- **Appearance & customisation** — layout, themes, shortcuts and their
  sharing.
- **Shell** — windows, the address/search bar, the command palette, menus and
  shortcuts: how the Person reaches every other domain.

## Entities and relationships

- **Profile** — one browsing identity. Stable identifier (never its name),
  display name, avatar/icon, accent colour, default search provider, its own
  website storage (sign-ins, cookies, cache, site data), and its own settings.
- **Window** — an on-screen frame belonging to exactly one profile for its
  whole life, showing one of that profile's spaces at a time.
- **Space** — a named, coloured, iconed working area inside a profile. Holds
  an ordered list of pinned tabs, an ordered list of regular tabs, and which
  tab is active.
- **Favourite** — a profile-wide tab shown at the top of every space of that
  profile; one live page shared by all those spaces.
- **Tab** — one open page slot. Current address, title, icon, back/forward
  trail, pinned flag, load state, audio flag, keep-awake flag.
- **Navigation entry** — one step in a tab's back/forward trail: address,
  title, time.
- **Closed-tab record** — enough of a closed tab (address, trail, origin
  space or favourites, position) to reopen it.
- **History entry** — a visited address, title and visit time.
- **Bookmark** — a saved address with title, inside an ordered folder tree.
- **Saved sign-in** — a username and secret for one website, with when it was
  saved and last used. Lives with the profile's chosen password provider.
- **Password provider** — where a profile's sign-ins come from and go to: the
  system's own password manager, Aeri's basic store, a password-manager
  extension, or none. One per profile.
- **Site permission** — a decision (ask / allow / block) for one website and
  one capability (camera, microphone, location, notifications, popups, …).
- **Certificate exception** — the Person's explicit choice to proceed past one
  certificate warning for one website.
- **Site setting** — per-website choices in a profile beyond permissions:
  keep awake. (Content blocking per site is uBlock Origin Lite's own; ADR 0008.)
- **Installed extension** — one extension in one profile: identity, version,
  approved permissions, enabled flag, pinned-to-toolbar flag, its own stored
  data.
- **Filter list** — dropped with the built-in blocker (ADR 0008): uBlock
  Origin Lite brings its own lists.
- **Download** — a file being or having been saved: source, destination,
  progress, outcome.
- **Session** — the remembered arrangement of a profile's windows, spaces,
  favourites and tabs.
- **Appearance** — layout and look choices: tab position (side/top), sidebar
  side (left/right), compact mode, theme, density, motion. Global defaults
  with per-profile accent and per-space colour.
- **Theme** — a named, shareable set of colours, typography and shape choices.
- **Shortcut map** — the Person's binding of keys to commands.

Relationships:

- Profile owns Spaces (one-to-many, at least one) and Favourites
  (one-to-many, ordered, may be zero).
- Profile has Windows (one-to-many, may be zero while the profile is dormant).
- Window shows one Space of its own Profile at any instant.
- Space owns Tabs (one-to-many, may be zero).
- Tab owns Navigation entries (one-to-many, ordered, with a current position).
- Profile owns History entries, Bookmarks, Saved sign-ins, Site permissions,
  Site settings, Certificate exceptions, Downloads, Closed-tab records,
  Installed extensions, and its Session (each one-to-many, never shared).
- Tab may reference an opener Tab (the page that opened it); the opener is
  always in the same Profile.
- Appearance references one Theme; Themes, Filter lists and the Shortcut map
  are global to the app (not profile data).

## Business rules

Identity
- A profile is identified by a generated identifier that never changes;
  renaming changes only the display name. Two profiles may share a display
  name; the name may not be empty or only whitespace.
- A window belongs to one profile for its whole life. "Switch profile" brings
  forward a window of the chosen profile, opening one on its last-active
  space if none is open. (Decided)
- Nothing a website stores under one profile is visible to a website loaded
  under another profile: sign-ins, cookies, cache, site data, permissions,
  site settings, certificate exceptions.
- Nothing Aeri itself records for one profile (history, bookmarks, saved
  sign-ins, closed tabs, downloads list, session, settings) appears in
  another profile.
- Every window shows which profile it belongs to (colour and avatar) at all
  times.
- Deleting a profile requires explicit confirmation naming the profile, and
  removes all of that profile's data, including its saved sign-ins. It cannot
  be undone.
- "Open in another profile" carries only the address. No sign-in, cookie or
  page state moves with it.

Workspace
- A tab belongs to exactly one space or to the profile's favourites; a space
  belongs to exactly one profile.
- A tab may move between spaces and favourites of the same profile. It may
  never move to another profile (that is "open in another profile" instead).
- In a space, favourites are shown first, then pinned tabs, then regular tabs.
  Order within each group is the Person's order and survives restarts.
- A favourite is one page: showing it from another space of the same profile
  shows the same page, not a copy.
- A space may be empty. Deleting a space that still has tabs requires
  confirmation; its tabs become closed-tab records.
- Duplicating a tab copies its address and back/forward trail into a new tab
  directly after it in the same space; it does not copy page state such as
  unsaved form input.

Browsing
- Text typed into the address bar that reads as an address is opened; any
  other text is sent to the profile's search provider.
- Suggestions while typing come only from the profile's own history,
  bookmarks and open tabs; nothing typed leaves the machine before the Person
  presses Return. (Assumed — matches Search)
- The address bar never runs page script (a typed or pasted script address is
  treated as text, not executed).
- Aeri's own internal pages (settings, profile manager, error pages) can be
  opened by the Person but never by a website.
- When a page fails to load, the tab stays open and shows what went wrong and
  a way to retry; the failing address stays in the address bar.
- When a page's process crashes, only that page is affected; the tab shows a
  crashed state with a reload action, and other tabs keep working.

Trust
- A website gets a capability only after the Person allows it for that site,
  within that profile. Undecided requests show a prompt attached to the tab
  that asked; closing or navigating that tab dismisses the prompt without
  granting.
- A certificate problem always stops the page and shows a warning. Aeri never
  proceeds automatically. Proceeding needs a deliberate Person action, applies
  to that one website in that one profile, and lasts only until the browser
  quits. (Decided)
- A website may open a new window only in response to a Person's action
  (click, key press); otherwise the attempt is blocked and the Person is told.
  Allowed popups open as tabs in the opener's space.
- Advertising and tracking requests are stopped before they are made by
  uBlock Origin Lite. Amended 2026-11-18 (ADR 0008, owner): Aeri has no
  built-in blocker; it opens uBO Lite's store page once at a profile's first
  launch, so blocking starts with one click instead of being on by default.
  Per-site switching off and filter lists are uBO Lite's.
- A downloaded file's name is cleaned of path separators and characters the
  file system forbids, and never lands outside the chosen download folder. A
  name that already exists gets a numbered suffix instead of overwriting.
- Websites never reach Aeri's own functions: they cannot read tabs, profiles,
  history, bookmarks, saved sign-ins or settings, nor trigger Aeri commands.

Extensions (Decided: required for the first usable release)
- The Person installs an extension into one profile, from its Chrome Web Store
  page or link. Before install, Aeri shows what the extension will be able to
  do; nothing is installed without the Person's confirmation.
- An extension is checked to be the genuine published package before it is
  installed or updated; a package that fails the check is refused.
- An extension gets only the permissions approved at install. An update that
  asks for more is held, disabled, until the Person approves the new ones.
- An extension and its data belong to one profile; installing the same
  extension in another profile is a separate install with separate data.
- Extensions never see saved sign-ins, other profiles, or Aeri's settings,
  and cannot change Aeri's interface beyond their own button, popup, and the
  places the extension system defines.
- The Person can disable, re-enable, pin, unpin and remove any extension;
  removing deletes its data in that profile.
- An extension that crashes or misbehaves is stopped and reported; pages keep
  working.
- An extension's resource cost is visible alongside tabs in the resource view.

Credentials (Decided: system password manager first, changeable later)
- Each profile has one password provider. The default is the system's own
  password manager where the system offers one to browsers; where it does
  not, the default is Aeri's basic store. The Person can change a profile's
  provider at any time: system, Aeri's basic store, an installed
  password-manager extension, or none.
- Sign-ins in the system's password manager belong to the Person's system
  account, not to a profile: every profile using the system provider sees the
  same accounts. Aeri says so when that provider is chosen. This is the one
  deliberate exception to profile isolation, and it covers sign-ins only.
- Sign-ins in Aeri's basic store belong to exactly one profile and are kept
  by that profile's Chrome password manager, encrypted with a key held in
  the system's protected secret storage; never in Aeri's own records
  (revised 2026-11-20 by the owner, ADR 0009: each sign-in was to be its
  own system secret).
- When the provider is an extension, Aeri offers nothing itself, to avoid two
  competing offers.
- Aeri offers to save a sign-in only after it has been submitted on a page;
  the Person chooses save, never, or not now.
- A saved sign-in is offered only on the website it was saved for (same site,
  secure connection, top-level page) and is filled only when the Person picks
  it — never automatically.
- The system may ask the Person to confirm it is them (fingerprint, password)
  before a secret is handed over; Aeri never caches a secret beyond filling
  it.
- Passkeys use the system's own passkey support where the system offers it.
- The Person can view (after the system confirms it is them), edit and delete
  sign-ins in Aeri's basic store, on Chrome's password manager page; system
  and extension sign-ins are managed in those tools.

Library
- A history entry is recorded for each completed top-level visit in a profile.
- Clearing browsing data acts on one profile, for chosen categories (history,
  site data and sign-ins, cache, downloads list, permissions, saved
  passwords) over a chosen time range.

Continuity
- On normal quit, the session of every open profile is remembered: windows,
  which space each shows, favourites, every space's tabs, their order, pins,
  active tab and back/forward trails.
- After an unexpected exit, the next launch offers the last remembered session.
- Quitting finishes only after every page has been shut down; nothing Aeri
  started keeps running after it exits.

Resource stewardship (Decided: lightweight is a primary goal)
- Work nobody can see is not done: a restored or unshown tab shows its title
  and icon but loads no page until first shown.
- An open profile with no shown pages costs close to nothing beyond the app
  itself; opening a second, third or fifth profile does not repeat the cost
  of the first.
- Pages not on screen are slowed down: their timers, animations and
  background work run at reduced rate unless they are playing sound, in a
  call, or marked keep-awake.
- A tab not shown for a while may be put to rest (its page released, its
  place, title, icon and trail kept). It is never put to rest while playing
  sound or video, capturing camera/microphone/screen, holding unsent form
  input, or marked keep-awake by the Person. Coming back reloads it. The
  idle period and on/off are the Person's choice.
- Under memory pressure from the operating system, Aeri puts eligible tabs to
  rest, least-recently shown first, before the system has to.
- The Person can see what each tab and profile is costing (memory, processor)
  and put a tab to rest or close it from there.
- Aeri's own interface does no work when nothing on it changes.
- A profile that is dormant runs no extensions and no pages.
- Lightness is proven by measurement against Helium and Chrome on the same
  machine and workloads (same pages, profiles, extensions); every release
  keeps or improves those results.

Appearance & customisation (Decided: Zen-level customisability)
- By default the page fills the window edge to edge; the sidebar and address
  bar slide in when the pointer reaches the edge or a shortcut is pressed,
  and slide away after. In the system's full-screen mode, all browser
  interface hides until called. (Decided: page-first experience)
- The Person chooses tab position (side or top), sidebar side (left or right),
  whether the sidebar stays shown or hides, light/dark/system, theme,
  density, and accent per profile and colour per space.
- Every command reachable from a menu or the palette can be bound to a
  shortcut; conflicting bindings are refused with the conflict named.
- Themes can be exported and imported as files; an imported theme can change
  only look, never behaviour, and cannot reach the network or any data.
- Reduced motion from the system is honoured regardless of theme.

Privacy
- Nothing about browsing activity leaves the machine because of Aeri itself.
  Any diagnostic sharing is off until the Person turns it on.

## Primary flows

1. First launch: Person opens Aeri -> one profile ("Personal") with one empty
   space is created -> a window for it shows that space with the address bar
   focused.
2. Browse: Person focuses the address bar -> types an address or search ->
   the active tab navigates -> title, icon and loading state update -> the
   visit is added to the profile's history.
3. New tab: Person creates a tab -> it appears after the active tab in the
   current space with the address bar focused.
4. Close and reopen: Person closes a tab -> a closed-tab record is kept ->
   "reopen closed tab" restores it to its original place (or the current
   space if that place is gone), with its back/forward trail.
5. Organise: Person pins, unpins, favourites, reorders or moves a tab to
   another space -> the new arrangement is shown and remembered.
6. Spaces: Person creates, renames, recolours, switches or deletes a space
   within the window's profile -> the window shows the chosen space;
   switching remembers the active tab of the space being left.
7. Profiles: Person creates a profile (name, colour, avatar) -> it starts with
   one empty space and no data -> a window for it opens.
8. Switch profile: Person picks another profile -> its existing window comes
   forward, or a new window opens on its last-active space.
9. Separate sign-ins: Person signs in to the same website in two profiles with
   different accounts -> each profile stays signed in to its own account,
   including after restart, and both windows can be open side by side.
10. Open in other profile: Person chooses "open in <profile>" on a link or tab
    -> a new tab opens in that profile's last-active space, in its window
    (opening one if none is open), with only the address.
11. Save a sign-in: Person submits a sign-in form -> Aeri offers to save to the
    profile's password provider -> next visit, focusing the field offers the
    matching accounts -> Person picks one -> the system confirms it is them if
    it requires -> it is filled.
12. Permission: a website asks for a capability -> prompt on its tab -> Person
    allows or blocks (once or remembered for the site) -> the decision applies
    to that site in that profile only.
13. Certificate warning: navigation hits a certificate problem -> warning page
    -> Person goes back (default) or deliberately proceeds for that site until
    quit.
14. Download: a website or the Person starts a download -> file saved with a
    clean, non-colliding name in the download folder -> progress and outcome
    shown in the profile's downloads list.
15. Command palette: Person opens the palette -> types -> picks among open
    tabs, history, bookmarks, spaces, profiles, and commands -> the choice is
    carried out.
16. Rest and return: a tab sits unseen past the idle period -> it is put to
    rest -> Person shows it again -> it reloads in place with its trail.
17. See the cost: Person opens the resource view -> sees each profile's and
    tab's memory and processor use -> rests or closes the heavy ones.
18. Install an extension: Person opens an extension's store page or pastes
    its link -> Aeri shows what it will be able to do -> Person confirms ->
    it is installed into the window's profile and its button appears.
19. Extension update: a newer version is published -> Aeri fetches and checks
    it -> same permissions: applied quietly; more permissions: held disabled
    until the Person approves.
20. Focus on the page: Person works with the interface hidden -> moves the
    pointer to the edge or presses the shortcut -> sidebar and address bar
    appear -> they hide again when the Person returns to the page.
21. Customise: Person changes layout, theme, or a shortcut -> every window
    reflects it at once -> it is remembered; a theme can be exported to a
    file and imported elsewhere.
22. Quit and restore: Person quits -> sessions remembered -> next launch shows
    the same windows, spaces, favourites and tabs, with only the shown tabs
    loading.
23. Delete profile: Person asks to delete -> confirmation naming the profile
    -> its windows close and all its data, including saved sign-ins and
    extensions, is removed.

## State transitions

Tab (what the page is doing):
- Unloaded --shown for the first time / reload--> Loading
- Loading --page finished--> Loaded
- Loading --load failed--> Failed
- Loading/Loaded --page process crashed--> Crashed
- Loaded --navigate / reload--> Loading
- Failed --retry / navigate--> Loading
- Crashed --reload--> Loading
- Loaded --idle period passed or memory pressure, and eligible--> Resting
- Resting --shown / reload--> Loading
- any --close--> Closed (terminal; a closed-tab record remains)

Tab (where it is seen), independent of the above:
- Background --becomes active tab of a shown space--> Foreground
- Foreground --another tab/space shown, or window hidden--> Background
- Background pages run slowed unless sound, call, capture or keep-awake.

Download:
- Pending --started--> In progress --finished--> Complete
- In progress --failed--> Failed --retry--> In progress
- Pending/In progress --cancelled by Person--> Cancelled

Site permission (per site, per capability, per profile):
- Ask --Person allows (remembered)--> Allow
- Ask --Person blocks (remembered)--> Block
- Allow/Block --Person resets--> Ask

Saved sign-in:
- (none) --Person saves after submit--> Saved
- Saved --Person edits--> Saved
- Saved --Person deletes / profile deleted / data cleared--> Removed
- (none) --Person chooses "never" for the site--> Never-for-site

Installed extension (per profile):
- (none) --Person confirms install, package checks out--> Enabled
- Enabled --Person disables--> Disabled --Person enables--> Enabled
- Enabled --update needs more permissions--> Held --Person approves-->
  Enabled (or --Person declines--> Disabled)
- Enabled --crashes or misbehaves--> Stopped --Person restarts / next
  launch--> Enabled
- any --Person removes / profile deleted--> Removed (data deleted)

Profile:
- Active (has an open window) --last window closed--> Dormant (data kept,
  nothing running)
- Dormant --opened / switched to / link opened in it--> Active
- Active/Dormant --deletion confirmed--> Deleted (terminal)

## Invariants

- At least one profile exists. At least one space exists in every profile.
- Every tab is in exactly one space or in its profile's favourites; every
  space and favourite belongs to exactly one profile.
- Every window belongs to exactly one profile for its whole life and shows
  exactly one of that profile's spaces; a space is shown in at most one
  window at a time (choosing it elsewhere brings its window forward).
- A space's active tab, if set, is one of that space's tabs or one of its
  profile's favourites.
- Within a space, favourites precede pinned tabs, which precede regular tabs.
- Website-visible storage and Aeri-recorded data of one profile are never
  readable under another profile. Sign-ins are isolated per profile except
  when two profiles both chose the system password manager.
- Secrets in Aeri's basic store exist only in the system's protected secret
  storage.
- A profile's identifier never changes and is never reused after deletion.
- No certificate warning is ever passed without a recorded Person action.
- A tab that is playing sound, capturing, holding unsent form input or marked
  keep-awake is never put to rest.
- A dormant profile has no running pages and no running extensions.
- An extension holds only permissions the Person approved.

## Edge cases

- Deleting the only profile -> refused; Person must create another first.
- Deleting the only space in a profile -> refused.
- Deleting the profile whose window is frontmost -> confirmation; its windows
  close; if no windows remain, a window opens on another profile.
- Closing the last tab in a space -> the space stays, shown empty with the
  address bar focused; the window does not close.
- Closing a profile's last window -> the profile becomes dormant; its pages
  stop; its session is remembered for next open.
- Two profiles with the same display name -> allowed; avatar and colour tell
  them apart, and Aeri suggests a different colour on creation.
- Renaming a profile while it has open windows -> the new name shows
  everywhere immediately; nothing about its stored data changes.
- A favourite shown in two windows of the same profile at once -> the live
  page appears in the frontmost window; the other shows its title and icon
  with "showing in another window".
- A popup without Person action -> blocked, with a notice offering to allow
  it once or always for that site.
- A website opens dozens of windows rapidly after one click -> only the first
  is allowed for that action.
- Reopening a closed tab whose space was deleted -> it opens in the current
  space.
- Restoring 100 tabs across 5 profiles -> titles and icons appear at once;
  only the shown tab of each window loads, so restart stays quick and light.
- A page hangs and never finishes loading -> the Person can stop it; the tab
  stays usable.
- A resting tab had a half-filled form the browser could not detect -> on
  return, the page reloads; to minimise this, resting is never automatic
  before the idle period and never for tabs the Person typed into since last
  load.
- A web app that must stay live (chat, music, dashboard) -> the Person marks
  it keep-awake; it is never rested or slowed beyond what the system forces.
- Content blocking breaks a site -> the Person turns uBlock Origin Lite off
  for that site from its popup, and the page reloads unblocked.
- An extension whose features need browser parts Aeri does not have -> Aeri
  tells the Person at install which features will not work, before they
  confirm.
- An extension package that fails the genuineness check -> refused, with the
  reason; nothing is installed.
- Removing an extension while its popup is open -> the popup closes; its data
  in that profile is deleted.
- Crash of the shared part of the web engine (not one page) -> every tab
  shows a crashed state; the session is preserved; reload recovers.
- Typing text that could be both a word and a local host name (e.g. "intranet")
  -> searched by default, with an offer to open it as an address.
- A download with a name like "../../.profile" -> saved as a cleaned name
  inside the download folder.
- A download to a folder without space or permission -> marked Failed with
  the reason; nothing partial is left under the final name.
- A sign-in page on an insecure connection -> no saved sign-in is offered, and
  no offer to save is made.
- A site served from a look-alike address -> sign-ins saved for the real site
  are not offered.
- The system password manager cannot be reached, or the Person refuses the
  system's confirmation -> nothing is filled; the Person can type the sign-in
  or switch provider.
- Switching a profile's provider -> existing sign-ins stay where they are;
  Aeri does not move secrets between providers without an explicit export.
- Opening a link in another profile that has no open window -> a window opens
  for that profile on its last-active space.
- Two windows of different profiles open the same website -> each sees only
  its own profile's sign-in.
- Importing a theme file that is malformed or tries to include anything but
  look choices -> refused with the reason; the current theme stays.
- Binding a shortcut already used by another command -> refused, naming the
  other command, with an offer to swap.
- A streaming site refuses protected playback in Aeri -> Aeri offers to open
  that page in the system's default browser, passing only the address.
  (Decided with the codecs plan, 2026-11-18)
- System switches between light and dark, or turns on reduced motion, while
  Aeri runs -> Aeri follows immediately.

## Open questions

1. **Default search provider** for a new profile (Assumed): per profile, a
   neutral default the Person can change.
2. **Private / temporary browsing** (Assumed): a later profile feature, after
   profiles ship.
3. **Extension coverage bar** (Assumed): the first usable release must run a
   named shortlist well — uBlock Origin Lite or equivalent, a password manager
   (1Password or Bitwarden), and a dark-mode or user-style extension — rather
   than promise every Chrome extension. Confirm or name the must-run list.
