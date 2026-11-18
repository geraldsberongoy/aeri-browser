# Content blocking and credentials: evidence for stack selection

Date: 2026-11-18. Author: BlockingPasswordsResearch. Scope: blueprint Stage 2 inputs.
Labels: **Observed** = verified in a primary source or a probe I ran. **Reported** = secondary source only. **Inferred** = my reasoning.
Probe machine: Apple M4 Pro, macOS 27.2.1 (Darwin 27.2.0, build 26B131). Throwaway probes live in `/tmp/research-bp/`. No repo file other than this one was touched.

## Summary of the findings that change the design

1. **Observed.** adblock-rust 0.13.6 is alive (released 2026-11-17), MPL-2.0, and parses uBO/ABP network and cosmetic syntax. It does **not** execute procedural cosmetic filters, does not do CNAME uncloaking, and rejects `$replace`, `$header`, `$ipaddress`, `$popup`, and regex `$removeparam`. The embedder supplies those pieces. Brave does so in browser-process C++ and a JS content script, both MPL-2.0.
2. **Observed.** Classic uBO (MV2) cannot run on current Chromium without patches. Chrome Web Store (CWS) no longer lists MV2 extensions. The Chromium `main` branch blocks MV2 unless a test-only static is set.
3. **Observed.** Apple's iCloud Passwords helper is gated by an AMFI parent launch constraint. A parent may launch it only if it is a listed browser (Chrome, Edge, Firefox, Arc, Brave, Vivaldi, Opera, Helium, Zen, ungoogled-chromium and others) **or holds the `com.apple.developer.web-browser.public-key-credential` entitlement**. I ran it from three non-listed parents on this Mac and the kernel killed all three. That entitlement is the same one passkeys need, so Aeri needs to request it anyway.
4. **Observed.** There is no public API for a non-Safari browser to read arbitrary sites' passwords from Apple Passwords. The only Apple-sanctioned bulk path is user-initiated credential exchange (macOS 26+).

---

## Q1. brave/adblock-rust

### 1.1 Version, licence, maintenance
- **Observed.** Cargo package `adblock` 0.13.6, licence `MPL-2.0`, edition 2024. Tag `v0.13.6`, commit `1108c61472dd5a29ae56388ee936048c4fe5a69f`, dated 2026-11-17. Source: https://github.com/brave/adblock-rust (`Cargo.toml`, `CHANGELOG.md`).
- **Observed.** crates.io shows ~1.45M total downloads, ~389k recent, and releases on 2026-07-09, 07-17, 07-19, 08-20, 10-28 (twice) and 11-17. Source: https://crates.io/api/v1/crates/adblock. The changelog shows that cadence as steady and recent: `$method` (0.13.0), `$to=` (0.13.6), a new DAT format v7 (0.13.4).
- **Observed.** The repo is Brave's own: the README says "the engine powering Brave's native adblocker". GitHub shows a push on 2026-11-17, ~2.9k stars.
- **Observed.** Cargo features: `single-thread` (default), `css-validation`, `content-blocking` (Apple content-blocker translation), `embedded-domain-resolver` (default), `resource-assembler` (parse uBO's resource files), `full-regex-handling`. With `single-thread` off, `Engine` becomes `Send + Sync`. The README still recommends single-thread access.
- **Observed.** The published crate excludes `data/` (see `exclude` in `Cargo.toml`). The repo's `data/` directory holds GPL/CC list snapshots that are not shipped in the crate.

### 1.2 What MPL-2.0 means inside an Apache-2.0 app
- **Observed** (Mozilla MPL FAQ, https://www.mozilla.org/en-US/MPL/2.0/FAQ/, Q8, Q10, Q13):
  - Distributing a binary that contains unmodified MPL code requires telling recipients where to get the MPL source.
  - You may license the other parts of the "Larger Work" as you like, so the combined binary can be Apache-2.0 for Aeri's own files.
  - Modifications to MPL files must stay MPL and be published. New, separate files that contain no MPL code are not Modifications.
- **Inferred.** Practical rule for Aeri:
  - Use `adblock` as a normal Cargo dependency, unmodified.
  - List it in `THIRD_PARTY_NOTICES` with a source link (crates.io plus tag).
  - Keep any fork of `adblock` and any copied Brave file (for example the procedural-filter JS, see 1.5) as separate MPL-2.0 files with their headers.
  - No obligation arises for Aeri's Apache-2.0 files.
- **Inferred.** This is engineering reading, not legal advice. A lawyer check before first release is cheap insurance.

### 1.3 Filter-syntax support (what the code actually does)
Method: read `src/filters/abstract_network.rs`, `network.rs`, `cosmetic.rs`, `blocker.rs`, `cosmetic_filter_cache.rs` at the commit above. Then ran a throwaway probe (`/tmp/research-bp/probe`) that calls the public `adblock::lists::parse_filter` on the repo's own list snapshots. The probe counts one line at a time. Its "unsupported" bucket also contains comment and header lines, so read only the specific error rows.

| Capability | Result | Evidence |
|---|---|---|
| Network rules (`\|\|`, `\|`, `*`, `^`, regex, `$domain/from`, `$to`, `$method`, types, `$3p/1p`, `$important`, `$badfilter`) | **Observed** supported | `abstract_network.rs` option table; 0.13.0 and 0.13.6 changelog |
| `$redirect=`, `$redirect-rule=` | **Observed** supported. `BlockerResult.redirect` carries the resource body. Probe: 41 brave-list rules with an empty `redirect-rule` value rejected | `blocker.rs` lines 20-60 |
| `$removeparam=` | **Observed** partial. Literal names `[A-Za-z0-9_-]+` only. Regex `removeparam=/…/` is rejected (14 hits in the brave list). Bare `removeparam` rejected (12). Exceptions rejected (2). Result field `rewritten_url` | `network.rs` line 24 (`VALID_PARAM`); probe |
| `$csp=` | **Observed** supported through `Engine::get_csp_directives`. Rejected when combined with a content type (12 hits) | `engine.rs` line 287; probe |
| Cosmetic: element hiding, specific and generic, exceptions | **Observed** supported. `hide_selectors`, `hidden_class_id_selectors` for dynamic classes and ids | `cosmetic_filter_cache.rs` |
| Procedural cosmetic (`:has-text`, `:matches-css*`, `:matches-attr`, `:matches-path`, `:min-text-length`, `:upward`, `:xpath`, `:remove()`, `:remove-attr()`, `:remove-class()`) | **Observed** parsed into `procedural_actions` (JSON strings) in `UrlSpecificResources`. The library does **not** evaluate them. The embedder must run them in the page | `cosmetic.rs` lines 52-92, 1162-1180; `cosmetic_filter_cache.rs` line 36 |
| Scriptlets `##+js(...)` and redirect resources | **Observed** supported. Resources are injected through `use_resources`. `resource-assembler` can parse uBO's `redirect-resources.js` and `scriptlets.js`. 17 `+js` rules in uBO's `filters.txt` and 18 in the brave list failed arg parsing in my probe, a small tail | `lib.rs`, `src/resources/`; probe |
| HTML filtering `##^script:has-text(...)` | **Observed** rejected ("html filtering is unsupported": 59 in uBO's list, 163 in the brave list) | probe |
| `$popup`, `$elemhide`, `$specifichide`, `$genericblock`, `$header`, `$permissions`, `$replace`, `$ipaddress`, `$urlskip`, `$strict1p/3p`, `$denyallow` | **Observed** not in the option parser. Unknown options return `UnrecognisedOption` and the rule is dropped. Probe: 2976 of 52k EasyList lines failed this way (for example `$popup,subdocument`), and 3913 of the brave list (for example `$xhr,1p,replace=/…/`) | `abstract_network.rs` line 311; probe |
| CNAME uncloaking | **Observed** absent from the library (`grep -ri cname src` returns nothing). Brave implements it in browser code (1.4) | `brave-core/browser/net/brave_ad_block_tp_network_delegate_helper.cc` |

Reading of the probe numbers (**Inferred**): the "unsupported" rows are mostly comment and header lines, so I count only the named error rows as real rejects. On that basis about 96% of non-comment EasyList lines (2,977 rejects of ~77k) and about 98% of the uBO `filters.txt` snapshot (173 rejects of ~11.4k) parse. Most EasyList losses are `$popup`-style options that Brave handles outside the engine. The probe cannot prove blocking parity with uBO. The repo ships `tests/ublock-coverage.rs` and `data/ublock-matches.tsv` for that comparison. I did not run it.

### 1.4 How Brave integrates it
All **Observed** unless noted, from https://github.com/brave/brave-core (master, 2026-11-18). Files under `components/brave_shields/core/browser/adblock/` and `browser/net/`:
- **Network layer.** `ad_block_engine.cc` and `ad_block_engine_wrapper.cc` wrap the Rust `Engine`. `brave_ad_block_tp_network_delegate_helper.cc` calls `engine_wrapper->ShouldStartRequest(...)` for every request and applies `rewritten_url` (removeparam) and `mock_data_url` (redirect). `brave_ad_block_csp_network_delegate_helper.cc` applies `$csp`.
- **CNAME.** The same helper resolves the canonical name with `network::mojom::ResolveHostClient` and re-checks the request against the engine with the CNAME host substituted. The comment says "CNAME uncloaked unless using aggressive blocking mode". A UMA histogram times the resolution.
- **Cosmetic.** The engine returns `hide_selectors`, `procedural_actions` and `script_injections` per URL. A renderer content script (`components/cosmetic_filters/resources/data/content_cosmetic.ts`) applies them. `procedural_filters.ts` (MPL-2.0 header) implements `has-text`, `matches-css*`, `min-text-length`, `upward`, `xpath` and so on in JS. It feeds new classes and ids back to `hidden_class_id_selectors`.
- **List supply.** Providers exist for component-updater lists (`ad_block_component_filters_provider`), custom user filters, user subscriptions (`ad_block_subscription_download_manager.cc`), default and custom resources, and a serialized-engine cache (`ad_block_dat_cache_manager.cc`). Several engine instances run in parallel (default plus additional); `blocker.rs` documents that `important` exists to coordinate between them.
- **Catalog.** `brave/adblock-resources` (MPL-2.0 in its `package.json`) holds `filter_lists/list_catalog.json` with 62 entries. Its README says the `permission_mask` bit 0 marks "trusted resources from the uBlock Origin project".

### 1.5 Measured matching cost and memory
My own run of the repo's Criterion benches (`cargo bench` at commit `1108c61`, release profile, Apple M4 Pro, 2026-11-18). The benches use `data/brave/brave-main-list.txt` (6.9 MB text, 186k lines) and `data/requests.json` (242,945 recorded browser requests):

| Bench | Result |
|---|---|
| `rule-match-browserlike/brave-list`, 242,945 requests | 929.8 / **937.8** / 947.5 ms (low / mid / high). That is **≈3.9 µs per request** (my division) |
| `rule-match-first-request/brave-list` (engine fresh from text, one request) | 565 / **575** / 583 µs |
| Heap retained after loading the list plus resources (`memory-usage-final`) | **9.23 MB** |
| Heap retained after 1,000 requests | 3.28 MB (the benchmark reports it separately; I do not interpret what it measures beyond that) |
| Peak heap while building the engine from text (`memory-usage-max`) | **30.8 MB** |
| Allocations to build | 292,832 |

- **Inferred.** Peak 30.8 MB versus ~9 MB resident shows why Brave caches the serialised engine (`serialize` and `deserialize`, DAT v7 in `engine.rs`). Aeri can build at update time off the UI thread and load the DAT file at launch. The changelog (0.13.4) says flatbuffer verification is skipped for DAT written and read by the same library version.
- **Inferred.** One shared immutable engine for all profiles costs ~9 MB once, not per profile, if `single-thread` is off and the engine is `Sync`. Per-profile state would be the per-site "blocking off" set and custom filters. Unverified with CEF; see Unknowns.
- **Reported.** The uBO maintainer says uBO's own JS engine was measured faster than "a well-known Rust-based filtering engine" by low single-digit µs (https://github.com/uBlockOrigin/uBOL-home/wiki/Frequently-asked-questions-(FAQ)). It names no engine or numbers.
- **Observed.** In CEF the hook point is `CefResourceRequestHandler::OnBeforeResourceLoad`: IO thread, can cancel (`RV_CANCEL`) or rewrite the URL, and `browser` and `frame` may be NULL for service-worker or `CefURLRequest` traffic. `CefRequest::GetResourceType()` exists. Source: https://github.com/chromiumembedded/cef `include/cef_resource_request_handler.h`, `cef_request.h` (master 2026-11-18). A 4 µs match fits that path with room to spare.

### 1.6 Which lists, and the licences
- **uBO lists.** **Observed:** `uBlockOrigin/uAssets` is GPL-3.0 (GitHub API; `LICENSE` is GPLv3 text). `filters.txt` headers point to that LICENSE. `gorhill/uBlock` and `uBOL-home` are also GPL-3.0.
- **EasyList / EasyPrivacy.** **Observed:** dual GPLv3-or-later and CC BY-SA 3.0, attribution "The EasyList authors (https://easylist.to/)". https://easylist.to/pages/licence.html. The page says other subscriptions hosted by third parties may carry other terms.
- **Brave's lists.** **Observed:** `brave/adblock-lists` is MPL-2.0 (GitHub API). Brave's `brave-main-list.txt` is an aggregate that includes uBO-derived lines (same header strings in the file).
- **Loadable lists.** **Observed:** the parser accepts ABP/uBO "Standard" and "Hosts" formats. In my probe the uBO `filters.txt` snapshot gave 3,775 network and 7,454 cosmetic rules, EasyList 48,804 and 24,773, and the brave list 121,567 and 43,459. EasyPrivacy, Peter Lowe and the uBO badware list are plain ABP syntax. I only probed the three lists named. The rest is **Inferred**.
- **Runtime download vs bundling.** **Inferred**, not legal advice:
  - If Aeri fetches lists from upstream URLs at run time (uAssets and easylist.to), Aeri's installer carries no GPL or CC BY-SA data. Redistribution questions go away. Attribution still belongs in the UI (list name, homepage).
  - If Aeri bundles a seed snapshot or mirrors lists on its own server, Aeri is distributing GPL-3.0 and CC BY-SA content. For plain-text lists the "source" is the file itself, so the burden is licence text and notices, not source offers. This stays a legal-review item.
  - First-run problem: requirement "on by default" plus runtime-only download means no blocking until the first fetch finishes. A bundled seed snapshot (or a first-launch fetch that blocks the first navigation briefly) is the product choice. Flag to Main.
  - uBO scriptlets (`resources`) are GPL-3.0 JavaScript executed in pages. Aeri downloading them at run time keeps them out of the Apache-2.0 distribution. Brave's resource pipeline (`adblock-resources` plus `resource-assembler`) shows the pattern. Whether injecting GPL scriptlets into pages makes the browser a combined work is a legal question I cannot settle (**Unknown**).

---

## Q2. uBlock Origin Lite and classic uBO

### 2.1 uBOL capabilities
- **Observed.** Current build 2026.1116.1650 (changelog; `chromium/manifest.json`). GPL-3.0. `minimum_chrome_version` 122. Manifest permissions: `activeTab, alarms, declarativeNetRequest, offscreen, scripting, storage, unlimitedStorage, userScripts`, host permission `<all_urls>`, 56 static rulesets, service-worker background, zero manifest content scripts. Source: https://github.com/uBlockOrigin/uBOL-home at `9e14668` (2026-11-16).
- **Observed.** Fully declarative design. The README says no permanent process, and the service worker wakes only for the popup and options pages. Default rulesets: uBO built-ins, EasyList, EasyPrivacy, Peter Lowe's list.
- **Observed.** Build log (`chromium/log.txt`): `ublock-filters` 10,899 filters in, 7,323 DNR rules out; `easylist` 52,288 in, 3,866 out; `easyprivacy` 56,664 in, 9,253 out; `ublock-badware` 9,997 in, 502 out; `adguard-spyware-url` 2,752 in, 2,701 out. Filters not turned into DNR rules are cosmetic or scriptlet rules, delivered as pre-compiled content scripts under `chromium/rulesets/scripting/`. The log also lists many "salvaged rule by ignoring unsupported `domain=`" entries (entity values such as `xhamster.*`).
- **Observed** (uBOL FAQ and changelog):
  - Filter lists refresh only when the extension updates. uBOL never contacts a remote server by itself.
  - The June 2026 release added custom filters and subscriptions to external lists, "in a way that respects reliability and efficiency".
  - Generic cosmetic filtering is off by default. It needs "Complete" mode.
  - Element picker and zapper are included.
  - Chromium 145 added `topDomains`/`excludedTopDomains` conditions, so some dynamic filtering is "partially" possible.

### 2.2 What uBOL cannot do (maintainer's own list)
**Observed**, uBOL FAQ, https://github.com/uBlockOrigin/uBOL-home/wiki/Frequently-asked-questions-(FAQ):
- `strict1p`, `strict3p`; entity-based `domain=`; `redirect-rule=` (DNR has no redirect-if-blocked); `click2load`; regex `removeparam=`; exceptions for modifier options; `replace=`; `ipaddress=`; `urlskip=` (partial); many regex filters DNR rejects.
- No response-header or response-body conditions ("no large media" switch impossible).
- CNAME uncloaking: "no DNR implementation supports this".
- "In general uBOL will be less effective against anti-content-blocker websites or minimizing breakage because many filters can't be converted."
- You cannot run another content blocker next to uBOL ("No… same reasons you should not use another content blocker with uBO").

### 2.3 DNR rule limits
**Observed**, https://developer.chrome.com/docs/extensions/reference/api/declarativeNetRequest (read 2026-11-18):
- At most 100 static rulesets per extension, 50 enabled at once.
- 30,000 guaranteed static rules per extension. The rest draws from a shared global pool. The uBOL FAQ states that pool is "330K" in Chromium (**Observed** from the FAQ text, not from Chromium source).
- Dynamic rules: 5,000 unsafe, 30,000 safe. Session rules: 5,000. Regex rules: 1,000 of each type, each under 2 KB compiled.
- Static rules of a disabled extension stop counting against the global limit (Chrome 128).
- uBOL's FAQ says its default ruleset is ~17k rules and stays near 30k with every annoyance list on.

### 2.4 Classic uBO (MV2) on current Chromium
- **Observed.** Chrome timeline (https://developer.chrome.com/docs/extensions/develop/migrate/mv2-deprecation-timeline): MV2 disabled for all users in Chrome 138 (2025-07-24); the enterprise policy removed in 139; "all remaining MV2 extensions removed from the Chrome Web Store" on 2026-08-31, so classic uBO cannot be installed or updated from CWS.
- **Observed.** Chromium source, `extensions/common/extension_features.cc`:
  - Tag `150.0.7871.115`: `kExtensionManifestV2Unsupported` enabled by default; `kAllowLegacyMV2Extensions` exists, disabled by default.
  - Tag `151.0.7922.0`: no MV2 feature mentions remain.
  - `main` (VERSION 160.0.8169.0): `extensions/browser/manifest_v2_handler.cc` has `ShouldDisableLegacyExtensions()` returning true unless `g_allow_mv2_for_testing` is set. Install and enable are blocked for affected MV2 extensions.
- **Observed.** The Bezier note makes the same claims about 150 and 151. I verified them independently against the tags above.
- **Conclusion (Observed + Inferred).** Classic uBO would need a Chromium patch, and Aeri forks nothing. Not an option on CEF 150+.

---

## Q3. Engine-level adblock-rust vs uBOL as an extension

| Axis | Engine-level (adblock-rust in the browser process) | uBOL (MV3 extension) |
|---|---|---|
| Network blocking | Whole ABP/uBO static syntax in 1.3. Callback sees every request on the IO thread. Can use CNAME, `$csp`, literal `$removeparam`, redirects | Chromium C++ DNR. Subject to rule limits and the maintainer's list in 2.2. No CNAME |
| Cosmetic | Engine returns selectors and procedural ops. Aeri must write the injector and the procedural runtime (or reuse Brave's MPL JS) | Pre-compiled declarative content scripts; procedural, scriptlets and picker included; works without Aeri code |
| Scriptlets | Aeri injects them; needs GPL-3 scriptlet resources downloaded or bundled | Shipped in package |
| List freshness | Aeri controls it: background refresh, on its own schedule. Meets "lists refresh in the background" | Only via extension updates; custom list subscriptions added June 2026 |
| User control | Aeri UI: per-site off, add/remove lists. This is what the requirements ask for | uBOL popup and options UI. Requires hosting extension UI, a CEF Chrome-style question (see Unknowns) |
| Per extra profile | **Inferred**: one shared engine, small per-profile state | **Inferred**: one extension instance, service worker and compiled rulesets per profile |
| Works if extensions are broken or disabled | Yes (independent of the extension system) | No |
| Cost to build | Medium: FFI, DAT cache, injector, procedural JS, CNAME resolver | Near zero beyond extension support, which is required anyway |
| Coexistence | If the user installs uBOL too, double filtering. uBOL's FAQ says to run only one blocker | n/a |

Measurements that exist (all limited):
- **Reported** (DebugBear, https://www.debugbear.com/blog/chrome-extensions-website-performance, 2024, updated 2025-12): on two ad-heavy news articles, classic uBO cut CPU time from 57 s to just under 4 s and page weight from 41 MB to under 3 MB. The test predates uBOL and does not isolate blocker overhead.
- **Reported** (peer-reviewed status unknown; arXiv 2503.01000v2, https://arxiv.org/abs/2503.01000): on ~1,000 sites, MV3 vs MV2 instances of uBlock Origin showed no statistically significant difference in blocking ads or trackers. Screenshots showed leftover cosmetic placeholders in 21-22% of sampled pages under MV3 versus 0% under MV2. The study is about uBO vs uBOL, not about engine-level blocking.
- **Not found.** No primary-source benchmark of adblock-rust in-browser vs uBOL RAM/CPU. The maintainer himself says only "benchmarks with proper methodology" can answer. The Brave bench in 1.5 gives only the engine's cost in isolation.
- **Inferred.** Net: engine-level wins on user control, freshness, CNAME and per-profile memory. uBOL wins on out-of-the-box cosmetic and scriptlet parity and costs nothing to build. They solve different parts, so Aeri's built-in blocker should be the engine, and uBOL stays the extension users may choose (disable the engine for a profile that installs it).

---

## Q4. macOS: website passwords from Apple Passwords / iCloud Keychain

### 4.1 AuthenticationServices: what each API allows
All **Observed** from Apple docs (fetched 2026-11-18 from developer.apple.com `.md` mirrors):

| API | Availability | What it gives a non-Safari browser |
|---|---|---|
| `ASAuthorizationPasswordProvider` / `ASAuthorizationPasswordRequest` | macOS 10.15+ | "Keychain credential sharing" for an **app**, which works through the app's associated domains. Apple's associated-domains docs say the app's ID must be listed in the site's `apple-app-site-association` for service `webcredentials`, and the app needs the entitlement. **Inferred:** a browser is not listed by arbitrary sites, so it cannot enumerate other sites' passwords |
| `ASCredentialProviderViewController`, `ASCredentialIdentityStore` | macOS 11+ | The **provider** side. A password manager implements this so the OS can fill from it. Aeri's own store could be offered to other apps this way. It does not read Apple's store |
| `ASAuthorizationWebBrowserPublicKeyCredentialManager` plus `…PlatformPublicKeyCredential` | macOS 13.3+ | Passkeys only (Q6). Not passwords |
| `ASCredentialExportManager` / `ASCredentialImportManager` | macOS 26.0+ | User-initiated transfer between credential apps (passwords and passkeys) with an out-of-process system UI. Aeri could be an **importer** if it declares a credential provider extension with `SupportsCredentialExchange`. Bulk, one-time, user-chosen. **Unknown** whether Apple's Passwords app acts as exporter |
| `ASCredentialUpdater` | macOS 26.0+ | Sites and apps signal credential changes to providers. Not a read path |
| WebKit AutoFill | n/a | **Observed** via Search's code comment: "WebKit's AutoFill is Safari's". A CEF page is not a WKWebView |

- **Observed.** The Search comment ("There is no public way for a browser to ask the Mac's Passwords app for a site's accounts") matches my reading of the docs. Source: https://github.com/driceroland/Search `Sources/Search/ApplePasswords.swift` at `c5870a6` (2026-11-18).

### 4.2 How Chrome integrates (verified on this Mac)
- **Observed** (this machine, macOS 27.2.1):
  - `/Library/Google/Chrome/NativeMessagingHosts/com.apple.passwordmanager.json` is a symlink into `/System/Cryptexes/App/Library/Google/Chrome/NativeMessagingHosts/`.
  - The manifest: `name: com.apple.passwordmanager`, `type: stdio`, `path:` `/System/Cryptexes/App/System/Library/CoreServices/PasswordManagerBrowserExtensionHelper.app/Contents/MacOS/PasswordManagerBrowserExtensionHelper`, `allowed_origins`: `chrome-extension://pejdijmoenmkgeppbflobdenhhabjlaj/` and `chrome-extension://mfbcdcnpokpoajjciilocoachedjkima/`. I did not identify the second extension.
  - `/Library/Application Support/Mozilla/NativeMessagingHosts/com.apple.passwordmanager.json` is the Firefox twin with `allowed_extensions`.
- **Observed.** The helper binary (bundle id `com.apple.PasswordManagerBrowserExtensionHelper`, version 27.2, `LSUIElement`, signed by Apple 2026-11-06, library-validation, no team ID) hard-codes both `chrome-extension://…` origins. `strings` on it also shows the commands the extension speaks (`ExtensionCommandGetLoginNames4URL`, `GetPassword4LoginName`, `SetPassword4LoginName_URL`, `NewAccount4URL`, …).
- **Observed.** CWS lists "iCloud Passwords" (id `pejdijmoenmkgeppbflobdenhhabjlaj`, ~7M users, rating 2.59/5): "compatible with macOS Sonoma or later, and Windows versions supported by the iCloud for Windows app". https://chromewebstore.google.com/detail/icloud-passwords/pejdijmoenmkgeppbflobdenhhabjlaj. Apple's Windows guide confirms the same extension for Chrome, Edge and Firefox with a six-digit pairing code: https://support.apple.com/guide/icloud-windows/autofill-passwords-in-a-web-browser-icw76039ec0f/icloud.

### 4.3 Where a Chromium-based browser looks for the manifest
- **Observed** (Chromium `main`, `chrome/common/chrome_paths.cc`, `DIR_NATIVE_MESSAGING`, https://chromium.googlesource.com/chromium/src/+/refs/heads/main/chrome/common/chrome_paths.cc):
  - macOS Chrome-branded: `/Library/Google/Chrome/NativeMessagingHosts`.
  - Chrome for Testing: `/Library/Google/ChromeForTesting/NativeMessagingHosts`.
  - Every other build (unbranded, which is what CEF is): `/Library/Application Support/Chromium/NativeMessagingHosts`.
  - Per-user: `<user-data-dir>/NativeMessagingHosts` (`DIR_USER_NATIVE_MESSAGING`).
- **Inferred.** An unbranded CEF-based Aeri will **not** see Apple's manifest under `/Library/Google/Chrome`. Aeri could install its own manifest (same `name`, same two `allowed_origins`, `path` to Apple's helper) into its user-data directory or `/Library/Application Support/Chromium/NativeMessagingHosts`. That uses only documented Chromium lookup. It still needs the launch gate in 4.4.
- **Observed** (Windows, `chrome/browser/extensions/api/messaging/launch_context_win.cc`): Chromium reads `SOFTWARE\Chromium\NativeMessagingHosts` only when `CHROMIUM_BRANDING`, then **falls back to** `SOFTWARE\Google\Chrome\NativeMessagingHosts`. So on Windows the iCloud for Windows host registration is found without Aeri doing anything. Whether the Windows host additionally checks its caller is **Unknown**.

### 4.4 The launch gate (new finding; Search's claims verified and refined)
Search's code comments say the helper "only answers Apple's extension ids" and that giving the extension origin is "the unofficial part". I tested both claims.

**Probe 1 (ran, <1 min).** Launched the helper directly with Apple's extension origin as `argv[1]` and stdin closed. Parents tried: the omp process (Developer-ID signed), `/bin/bash` and `/bin/sh` (platform binaries), and python 3.14 (unsigned). Every run exited 137 (SIGKILL). The unified log shows, for each:
`kernel: (AppleMobileFileIntegrity) AMFI: Launch Constraint Violation (enforcing), error info: c[4]p[2]m[1]e[0], (Constraint not matched) launching proc … PasswordManagerBrowserExtensionHelper, launch type 0, failure proc …`, and the failure proc was the parent in each case.

**Probe 2 (`strings` on the helper).** The binary embeds a `LaunchConstraintVerifier` and an error text: "The native messaging host for the iCloud Passwords extension does not have required parent launch constraints that prevent it from being run by unauthorized processes." It also embeds an `$or` of constraint terms:
- an entitlement query for `com.apple.developer.web-browser.public-key-credential`, or
- (signing-identifier, team-identifier) pairs for: Chrome (+beta, canary, dev; team `EQHXZ8M8AV`), Edge (+Beta/Canary/Dev; `UBF8T346G9`), Firefox (+Developer Edition, Nightly; `43AQ936H96`), Arc and Dia (`company.thebrowser.*`; `S6N382Y83G`), Brave (+beta, nightly; `KL8N8XSYF4`), BrowserOS (`8YMKWU47S5`), Vivaldi (`4XF3XNRN6Y`), Helium (`net.imput.helium`; `S4Q33XPHB4`), Opera family (`A2P9LX4JPN`), Kagi Orion (`TFVG979488`), ungoogled-chromium (`B9A88FL5XJ`), Zen (`9V5K9TP787`), Wavebox (`4259LE8SU5`), Quark (names as they appear in the binary).

**Probe 3.** `codesign -d --entitlements` on `/Applications/Search.app` (v1.0.14, team `7BYKA895MC`): it holds `com.apple.developer.web-browser.public-key-credential = true`. `7BYKA895MC` does not appear in the helper's strings.

**Findings.**
- **Observed.** The helper is not callable by arbitrary processes on macOS 27.2. The gate is enforced by the kernel (AMFI), not by the helper's argument check.
- **Inferred** (consistent with all three probes, but I did not launch from Search or Chrome): a third-party browser passes the gate by holding the Apple-granted browser-passkey entitlement (4 and Q6). Search appears to rely on exactly this. Chrome, Edge and Helium are on the explicit list.
- **Observed.** The origin check is separate and hard-coded to Apple's two extension ids. So Aeri has two routes with the gate satisfied:
  1. **Official-shaped.** Install Apple's extension from CWS into the profile (genuine origin) and register the manifest (4.3). Needs CEF extension support with native messaging, for example in Chrome-style windows (**Unknown**, spike).
  2. **Search's route.** Spawn the helper from Aeri's own process with Apple's extension origin as `argv[1]` and speak its SRP-plus-AES-GCM protocol (Search implements it in `ApplePasswordsProtocol.swift`, tested against vectors from Apple's extension, per its comments). Search itself calls this unofficial. It leans on an undocumented protocol Apple can change (the helper's `secretSessionVersion` check exists in Search for this reason).
- **Observed.** Search runs the helper from `/System/Cryptexes/App/...` or `/System/Library/CoreServices/...` only after verifying the Apple signature (`identifier "…" and anchor apple`). That part of its design is sound and cheap to copy.
- **Reported/Unknown.** Apple has published no developer documentation for the helper protocol. Whether the entitlement grants anything beyond the launch is not documented.

### 4.5 Can another Chromium browser "officially" register it?
- **Observed.** Registration is a manifest file in a documented Chromium directory (4.3), so any browser can drop the file. The `allowed_origins` list is Apple's, so Aeri can only use it by installing Apple's extension from CWS or by impersonating the origin.
- **Observed.** The kernel gate shows Apple deliberately maintains a list of supported browsers (including Helium, Arc, Brave, Vivaldi, Zen, Opera, ungoogled-chromium) and a general entitlement path for the rest. That is the closest thing to "official" that exists. No public Apple page documents it. Treat it as **Inferred** intent.

---

## Q5. Windows: website password managers for third-party browsers

- **Observed.** Windows Credential Manager (`CredRead`/`CredWrite`, wincred.h) holds per-logon-session generic and domain credentials. It is a store for apps, not a browser autofill provider. `CredRead` returns credentials from "the user's credential set… associated with the logon session of the current token". https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw. Windows has **no** API that exposes Edge's passwords to other browsers.
- **Observed.** Microsoft Password Manager lives in Edge: "Microsoft Password Manager in Edge is only available if you sign in with a personal account profile." https://support.microsoft.com/en-us/accounts-billing/manage/view-or-edit-your-passwords-in-microsoft-password-manager. Passkeys saved there appear in Windows Settings > Accounts > Passkeys. Passwords do not appear.
- **Reported** (https://www.bleepingcomputer.com/news/security/microsoft-ends-authenticator-password-autofill-moves-users-to-edge, 2025-05-03; AP News): Microsoft Authenticator ended password autofill (stated deadline 2025-08-01) and moved password management to Edge. A user comment on the Windows IT Pro blog (Nov 2025) says the same. So Authenticator is not an option.
- **Observed.** iCloud for Windows plus the CWS extension works in Chrome, Edge and Firefox (4.2). With 4.3's registry fallback, a Chromium-based browser that runs CWS extensions on Windows should see the host. **Inferred**, not tested (no Windows machine in this run).
- **Realistic Windows plan (Inferred):** (a) Aeri's own basic store (Q7) as the default; (b) users install Bitwarden, 1Password or iCloud Passwords as CWS extensions, which is the same path the requirements already mandate. There is no OS-native website-password provider for third-party browsers on Windows.

---

## Q6. Passkeys

### 6.1 macOS
- **Observed.** Browsers use `ASAuthorizationWebBrowserPublicKeyCredentialManager` (macOS 13.3+) plus `ASAuthorizationController` with `ASAuthorizationPlatformPublicKeyCredentialProvider` requests. Apple's article "Authenticating people by using passkeys in browser apps": the app asks for authorization (`notDetermined`/`authorized`/`denied`), lists credentials per relying party, and runs the assertion. https://developer.apple.com/documentation/authenticationservices/authenticating-people-by-using-passkeys-in-browser-apps. Third-party credential providers' passkeys flow through the same API since macOS 14 (https://www.imperialviolet.org/2023/10/18/icloudkeychain.html).
- **Observed.** The gate is the entitlement `com.apple.developer.web-browser.public-key-credential` (macOS 13.3+). Apple's page: the **Account Holder of an organisation's** developer account must file the request form at https://developer.apple.com/contact/request/macos-browsers-passkeys/. Apple reviews against criteria: HTTP and HTTPS schemes in `Info.plist`, a URL field, search or bookmarks on launch, loads the destination directly and renders the expected content. Safe-browsing warnings and native auth UI are allowed. https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.web-browser.public-key-credential. A forum thread (https://developer.apple.com/forums/thread/829169) says the capability does not appear on macOS App IDs in the portal. I could not read the thread past its bot check, so treat as **Reported**.
- **Observed** (Chromium `main`): `device/fido/mac/icloud_keychain_sys.mm` checks `SecTaskCopyValueForEntitlement(…"com.apple.developer.web-browser.public-key-credential")` and only offers iCloud Keychain if the process has it. `device/fido/mac/authenticator_config.h` shows the older Touch ID authenticator needs an embedder-supplied `keychain_access_group` plus a per-profile metadata secret ("Chrome returns different secrets for each user profile").
- **Observed.** The unsigned/unentitled outcome is real: ungoogled-chromium's passkey bug (https://github.com/ungoogled-software/ungoogled-chromium/issues/2606, closed 2024-09) was diagnosed by a commenter as the missing entitlement and "principally unfixable by us" by a maintainer. It has since been added to Apple's helper list (4.4).
- **Observed.** The user also sees a system permission prompt on first use (Passkeys Access for Web Browsers; reset with `tccutil reset WebBrowserPublicKeyCredential`). https://www.imperialviolet.org/2023/10/18/icloudkeychain.html.
- **Inferred.** Aeri needs: an organisation developer account with an Account Holder, an approved entitlement request (lead time unknown), a provisioning profile that carries the managed capability, and a hardened-runtime, notarised build. Development builds cannot use iCloud Keychain passkeys without it. Whether CEF's Chrome-bootstrap WebAuthn path exercises this Chromium code in both Chrome-style and Alloy-style browsers is **Unknown** (spike).

### 6.2 Windows
- **Observed.** Chromium loads `webauthn.dll` dynamically (`device/fido/win/webauthn_api.cc`: `LoadLibraryExA("webauthn.dll", …, LOAD_LIBRARY_SEARCH_SYSTEM32)`) and binds `WebAuthNAuthenticatorMakeCredential/GetAssertion`, `WebAuthNIsUserVerifyingPlatformAuthenticatorAvailable` and the platform-credential-list functions. Calls take an `HWND`. No special entitlement appears in that file. Public headers: https://github.com/microsoft/webauthn (MIT).
- **Observed.** Windows 11 now supports **plugin passkey managers**. Microsoft Password Manager (Edge) is integrated as a plugin, GA with the November 2025 security update, and 1Password and Bitwarden were first third-party plugins. Authentication uses Windows Hello. "Users can save, manage, and use passkeys across browsers and native apps." https://techcommunity.microsoft.com/blog/windows-itpro-blog/windows-11-expands-passkey-manager-support/4467572. The plugin API needs Windows 11 24H2 build ≥26100.6725 or 25H2 ≥26200.6725 (https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/passkeymanager).
- **Inferred.** Any browser that calls `webauthn.dll` with a valid `HWND` gets Windows Hello and plugin passkeys with no gate. This is simpler than macOS. Aeri-on-Windows inherits whatever the embedded Chromium implements.

---

## Q7. Secret storage for Aeri's own basic password store

### 7.1 Crates (crates.io API and repos, 2026-11-18)
| Crate | Version | Licence | Maintenance |
|---|---|---|---|
| `security-framework` | 3.7.0 (2026-02-20) | MIT OR Apache-2.0 | kornelski/rust-security-framework; last push 2026-08-14; ~118M recent downloads. Safe wrapper for Security.framework including `SecAccessControl` and `set_access_group` |
| `security-framework-sys` | 2.17.0 | MIT OR Apache-2.0 | same repo |
| `keyring` | 4.2.0 (2026-08-29) | MIT OR Apache-2.0 | open-source-cooperative/keyring-rs; the README says the crate is now a **thin v1-compatible wrapper**. The API is `keyring-core` 1.0.1 and each OS store is a **separate crate** |
| `apple-native-keyring-store` | 1.0.2 (2026-08-06) | MIT OR Apache-2.0 | `keychain` feature = legacy/file-based keychain (for apps **not** code-signed with a provisioning profile); `protected` feature = data-protection keychain (needs a provisioning profile; adds iCloud sync and biometrics) |
| `windows-native-keyring-store` | 1.1.0 (2026-05-24) | MIT OR Apache-2.0 | Windows Credential Manager. README warns Credential Manager loses updates under concurrent writes and uses a SID-keyed named mutex |
| `windows` | 0.62.2 (2025-10-06) | MIT OR Apache-2.0 | microsoft/windows-rs; last push 2026-11-18; ~115M recent downloads |
| `windows-sys` | 0.61.2 (2025-10-06) | MIT OR Apache-2.0 | same repo; ~523M recent downloads |

All permissively licensed and compatible with Apache-2.0. All **Observed** (crates.io API, repo READMEs).

### 7.2 Access control, per platform
- **macOS (Observed**, TN3137, https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains**).** Two keychain implementations:
  - File-based (login keychain): ACL model (`SecAccess`). The default target of `SecItem` unless `kSecUseDataProtectionKeychain` or `kSecAttrSynchronizable` is set.
  - Data-protection keychain: keychain **access groups** plus optional `SecAccessControl` (biometrics, Secure Enclave). Groups come from code-signing entitlements, which "must be authorized by a provisioning profile"; the program needs an app-like bundle to embed it. Apple recommends it as the default for new code.
  - Apple's guidance: "the file-based keychain is on the road to deprecation". Starting in macOS 26.4 a file-based keychain may need a protected entropy file in `/var/db/SystemKeys` and is no longer a standalone item.
- **Inferred.** Aeri ships as a signed app bundle (it needs a bundle for CEF helpers anyway), so it can use the data-protection keychain with a team-prefixed access group. That binds items to Aeri's team-signed code instead of "any process the user approves". It reuses the provisioning-profile work the passkey entitlement already requires. Fall-back: the file-based keychain through `apple-native-keyring-store` `keychain` (works unsigned, weaker ACL story, shown prompts on signature change; **Inferred**, not tested).
- **Per-profile isolation (Inferred).** Use one service name per profile id, for example `app.aeri.profile.<uuid>`, and delete by service when a profile is deleted (requirement: deletion removes saved sign-ins). `keyring` entries are (service, user) pairs. Search for credentials by service is supported in the Windows store (feature `search`).
- **Windows (Observed).**
  - Credential Manager credentials belong to the user's logon session (`CredRead` doc). Any process running as that user can read them; there is **no per-application ACL**.
  - DPAPI `CryptProtectData`: "Typically, only a user with the same logon credential… can decrypt". The optional entropy blob must match at decrypt, which is a weak per-app secret, not an access control. https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata.
  - Reading a password in Aeri's UI should be gated by Windows Hello consent. That is a separate API (`UserConsentVerifier`), not evaluated here.
- **Inferred.** On Windows, store the secret in Credential Manager or a DPAPI-encrypted blob and accept that same-user malware can read it, exactly as it can for Chrome's `Login Data`. Aeri must not claim stronger isolation. Chromium's own app-bound encryption (Chrome 127+) is Google-service specific and not evaluated.
- **Inferred.** The "view after the system confirms it is them" requirement maps to `LocalAuthentication` (`LAContext`) on macOS and `UserConsentVerifier` on Windows. Neither is covered by the crates above; use `objc2-local-authentication` or the `windows` crate's WinRT bindings (not evaluated).

### 7.3 Which crate to pick (Inferred)
`keyring-core` + `apple-native-keyring-store` (`protected`) + `windows-native-keyring-store` gives one API and two stores, all MIT OR Apache-2.0, all maintained in 2026. If the access-group and biometric features turn out awkward through that abstraction, drop to `security-framework` directly on macOS and `windows` on Windows. `windows-sys` is raw FFI with no safety layer and only worth it if binary size matters.

---

## Implications for Aeri

1. **Blocking.** Build the built-in blocker on adblock-rust in the browser process, fed by runtime-downloaded lists, and cache the serialised engine. Budget for the glue Brave wrote: request hook, CNAME resolver, `$csp`, cosmetic and scriptlet injection, and a procedural runtime (reuse MPL JS from Brave, or write it). Expect to lose `$popup`, `$replace`, `$header`, `$ipaddress`, `$denyallow` and regex `$removeparam` unless Aeri adds them.
2. **Extensions.** Treat uBOL as a user-installable CWS extension, not as the built-in blocker. Classic uBO is out. If Aeri also runs the engine, document "do not run two blockers" and consider auto-disabling the engine in a profile where uBOL is enabled.
3. **Licence hygiene.** Keep `adblock` unmodified and list it in notices. Keep lists and scriptlets out of the installer where possible, but decide the first-run seed question (GPL-3.0 and CC BY-SA data in the installer or a first-fetch gate). Get legal review on GPL scriptlet injection.
4. **macOS credentials need an organisation Apple Developer account and the browser-passkey entitlement.** It unlocks iCloud Keychain passkeys *and* satisfies the launch gate for Apple's Passwords helper. Start the request early; the review time is unknown.
5. **Apple Passwords on Mac** is reachable only through the helper (extension route or Search's route). The Search route is undocumented and may break. Plan the default as Aeri's own store plus Keychain, with Apple Passwords as an optional "Connect" feature behind a setting, as Search does.
6. **Windows**: no OS website-password manager exists for third-party browsers. Default to Aeri's store; passkeys come from Windows Hello and plugin managers through `webauthn.dll`; password-manager extensions fill the rest.
7. **Secret store.** Data-protection keychain with a per-profile service name on macOS; Credential Manager or DPAPI on Windows. Say plainly in the UI and docs that same-user processes can read the Windows store.
8. **Per-profile cost.** The engine route keeps extra profiles near-free (shared engine). The extension route costs a service worker and rulesets per profile. This favours the engine for the "each extra profile costs little" requirement (Inferred).

## Unknowns that need a spike

1. **Does Apple's helper launch from Aeri with the entitlement, and does it answer?**
   Experiment (≈1 day, needs the Apple entitlement; before approval, test with a sacrificial dev app whose team has it, or with Search's binary as a reference): build a 50-line Rust or Swift CLI wrapped in an app bundle with the entitlement and a provisioning profile, `Process.run` the helper with Apple's origin, send `{"cmd":14}` (hello) over Chrome's 4-byte little-endian framing, and read the reply. Check the unified log for `AMFI: Launch Constraint Violation`. Repeat with a bundle without the entitlement as a control.
2. **Does the Apple extension + native messaging work in a CEF Chrome-style window with a user-data-dir manifest?**
   Experiment (≈half day, depends on CefResearch's findings): use the cefsimple sample, `--load-extension` a CWS CRX of `pejdijmoenmkgeppbflobdenhhabjlaj`, drop the manifest in `<root_cache_path>/NativeMessagingHosts/`, open the extension popup, pair with the six-digit code, fill a login.
3. **Is the Apple extension on CWS installable in CEF at all?** CWS install in an unbranded Chromium requires a CRX signature path (the requirement already says the package is verified). Test with the cefsimple sample and report whether "Add to Chrome" works or only `--load-extension`.
4. **Engine in the CEF browser process.** Experiment (≈1 day): link `adblock` 0.13.6 (static lib via a small C ABI), call it from `OnBeforeResourceLoad` on a cefsimple page that loads 50 known ad URLs, measure µs per call under real load, and confirm `browser == NULL` service-worker requests are handled. Also confirm `Engine` can be shared across `CefRequestContext`s with `single-thread` disabled.
5. **Cosmetic injection in CEF.** Experiment (≈1 day): can a render-process extension or `CefRenderProcessHandler` inject CSS and the procedural runtime at document start in an Alloy-style browser (macOS NSView embedding) and in a Chrome-style one? The Bézier note already lists "content scripts in Alloy" as unknown. Measure flash of unblocked content.
6. **uBOL in an Alloy-style browser** (inherited from CefResearch). Experiment: `--load-extension` uBOL, check static DNR blocking works without Chrome UI, and check how its popup and "Allow user scripts" toggle can be reached when Aeri draws the UI with GPUI.
7. **Blocking parity.** Experiment (≈half day): run `tests/ublock-coverage.rs` (already in the repo) against current uBO lists and the 1,000-site crawl from the arXiv paper's method, comparing ad and tracker counts for adblock-rust (+Aeri glue) vs uBOL vs classic uBO in a Chrome profile.
8. **Memory per profile.** Experiment (≈half day, after 4): launch 1, 3 and 5 profiles of a CEF sample with the engine in one process vs uBOL loaded in each; record resident memory of browser, network and extension processes with `footprint`.
9. **Windows host check.** Experiment (≈half day, needs a Windows VM): install iCloud for Windows and the CWS extension, run an unbranded Chromium build, check whether pairing and fill work, and whether the host validates its caller.
10. **Entitlement lead time and provisioning.** Experiment (non-technical): an Account Holder submits the Apple request form; record turnaround and whether the managed capability appears in the portal for macOS (the forum report says it may not).
11. **Keychain ACL behaviour.** Experiment (≈half day): store a secret with the `protected` store from a signed build, then try to read it from a different-team signed binary and from the same team with a different bundle id. Same for the `keychain` store from an ad-hoc build.
12. **Legal.** Ask counsel: (a) bundling a list snapshot; (b) GPL-3.0 scriptlets injected into pages; (c) MPL-2.0 notices in an Apache-2.0 binary.

## Sources

Dates are fetch dates (2026-11-18) unless noted. Repos were read at the named commit or tag.

**adblock-rust, Brave**
- https://github.com/brave/adblock-rust, tag `v0.13.6`, commit `1108c61472dd5a29ae56388ee936048c4fe5a69f` (2026-11-17): `Cargo.toml`, `README.md`, `CHANGELOG.md`, `src/**`, `benches/**`, `data/**`.
- https://crates.io/api/v1/crates/adblock (versions, licence, downloads).
- https://github.com/brave/brave-core (master 2026-11-18): `components/brave_shields/core/browser/adblock/*`, `browser/net/brave_ad_block_tp_network_delegate_helper.cc`, `components/cosmetic_filters/resources/data/procedural_filters.ts`.
- https://github.com/brave/adblock-resources (README, `package.json`, `filter_lists/list_catalog.json`); https://github.com/brave/adblock-lists.
- https://www.mozilla.org/en-US/MPL/2.0/FAQ/ (Q8, Q10, Q13).
- https://easylist.to/pages/licence.html; https://github.com/uBlockOrigin/uAssets (`LICENSE`, GitHub API licence field).
- https://github.com/chromiumembedded/cef, `include/cef_resource_request_handler.h`, `include/cef_request.h` (master).
- Probes: `/tmp/research-bp/adblock-rust` (`cargo bench --bench bench_memory`, `--bench bench_matching -- brave-list`) and `/tmp/research-bp/probe` (parse counts).

**uBO Lite, MV2**
- https://github.com/uBlockOrigin/uBOL-home at `9e14668` (2026-11-16): `README.md`, `CHANGELOG.md`, `chromium/manifest.json`, `chromium/log.txt`; wiki FAQ https://github.com/uBlockOrigin/uBOL-home/wiki/Frequently-asked-questions-(FAQ).
- https://developer.chrome.com/docs/extensions/reference/api/declarativeNetRequest.
- https://developer.chrome.com/docs/extensions/develop/migrate/mv2-deprecation-timeline.
- Chromium source: https://chromium.googlesource.com/chromium/src/+/refs/tags/150.0.7871.115/extensions/common/extension_features.cc, `refs/tags/151.0.7922.0` same file, and `refs/heads/main` (VERSION 160.0.8169.0) `extensions/browser/manifest_v2_handler.cc`.
- https://www.debugbear.com/blog/chrome-extensions-website-performance (Reported).
- https://arxiv.org/abs/2503.01000, Lukic and Papadopoulos, 2025-03-02, v2 (Reported).
- https://github.com/nvergez/bezier/blob/research/cef-macos/docs/research/cef-macos.md (secondary; MV2 claims re-verified above).

**Credentials, macOS**
- Apple docs: ASAuthorizationPasswordProvider/Request, ASCredentialProviderViewController, ASCredentialIdentityStore, ASCredentialImportManager/ExportManager (macOS 26.0), ASAuthorizationWebBrowserPublicKeyCredentialManager (macOS 13.3), https://developer.apple.com/documentation/authenticationservices/authenticating-people-by-using-passkeys-in-browser-apps.
- https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.web-browser.public-key-credential.
- https://developer.apple.com/documentation/xcode/supporting-associated-domains and `…/entitlements/com.apple.developer.associated-domains`.
- https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains (revision 2026-09-24).
- https://developer.apple.com/forums/thread/829169 (search snippet only; page blocked, Reported).
- https://github.com/driceroland/Search at `c5870a6` (2026-11-18): `Sources/Search/ApplePasswords.swift`, `ApplePasswordsProtocol.swift`, `ExtensionNative.swift`.
- Local probes on macOS 27.2.1: `/Library/Google/Chrome/NativeMessagingHosts/com.apple.passwordmanager.json`, `/Library/Application Support/Mozilla/NativeMessagingHosts/com.apple.passwordmanager.json`, `codesign -d` and `strings` on the helper bundle (version 27.2), `codesign -d --entitlements` on `/Applications/Search.app`, `log show` AMFI entries (2026-11-18 10:20 local).
- https://chromewebstore.google.com/detail/icloud-passwords/pejdijmoenmkgeppbflobdenhhabjlaj.
- https://www.imperialviolet.org/2023/10/18/icloudkeychain.html (Adam Langley, 2023-10-18).
- https://github.com/ungoogled-software/ungoogled-chromium/issues/2606.
- Chromium `main`: `chrome/common/chrome_paths.cc`, `device/fido/mac/icloud_keychain_sys.mm`, `device/fido/mac/authenticator_config.h`, `device/fido/win/webauthn_api.cc`, `chrome/browser/extensions/api/messaging/launch_context_win.cc`.

**Credentials, Windows**
- https://support.apple.com/guide/icloud-windows/autofill-passwords-in-a-web-browser-icw76039ec0f/icloud.
- https://support.microsoft.com/en-us/accounts-billing/manage/view-or-edit-your-passwords-in-microsoft-password-manager.
- https://www.bleepingcomputer.com/news/security/microsoft-ends-authenticator-password-autofill-moves-users-to-edge (2025-05-03, Reported); https://apnews.com/article/microsoft-authenticator-passwords-passkey-4cf0d3e4488f10702e15d916dc3604fd (Reported, search snippet).
- https://techcommunity.microsoft.com/blog/windows-itpro-blog/windows-11-expands-passkey-manager-support/4467572 (2025-11-11).
- https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/passkeymanager (updated 2026-09-04); https://github.com/microsoft/webauthn.
- https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw; https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata.

**Rust crates**
- https://crates.io/api/v1/crates/{keyring,security-framework,security-framework-sys,windows,windows-sys,apple-native-keyring-store,windows-native-keyring-store,keyring-core}.
- https://github.com/open-source-cooperative/keyring-rs (commit `03f7683`, 2026-11-06), `apple-native-keyring-store`, `windows-native-keyring-store`; https://github.com/kornelski/rust-security-framework; https://github.com/microsoft/windows-rs.

## Recommendation table

| Need | Best option | Fallback | Evidence |
|---|---|---|---|
| Built-in ad and tracker blocking, on by default, uBO-quality lists | adblock-rust 0.13.x in the browser process, lists downloaded at run time, serialised engine cache | uBO Lite shipped as a preinstalled CWS extension | Q1 1.3-1.5, Q3; engine 3.9 µs per request, 9.2 MB resident (Observed, my bench) |
| Procedural cosmetics, scriptlets, CNAME | Reuse Brave's MPL-2.0 JS and resolver design; Aeri writes the injector | Skip procedural filters in v1 and accept lower parity | 1.3, 1.4 (Observed) |
| Classic uBO | Not available | uBO Lite | 2.4: Chromium 150 flags, 151 removal, `main` blocks MV2 (Observed) |
| Extension-based blocker | uBO Lite from CWS | Engine only | 2.1-2.3 (Observed); rule limits and non-portable options |
| List and scriptlet licensing | Fetch lists and resources from upstream at run time; attribution in UI; legal review | Bundle a small seed snapshot after review | 1.6 (Observed licences, Inferred consequences) |
| macOS website passwords (default) | Aeri store in the data-protection Keychain, per-profile service, `security-framework` or `apple-native-keyring-store` (`protected`) | Same store in the file-based keychain | Q7 TN3137 (Observed) |
| macOS: Apple Passwords / iCloud Keychain | Apple's CWS extension plus native-messaging manifest in Aeri's user-data dir, with the browser-passkey entitlement | Search-style direct helper launch (undocumented, may break) | 4.2-4.5 (Observed manifests, launch gate probes); no public API (4.1) |
| macOS passkeys | `ASAuthorizationWebBrowserPublicKeyCredentialManager` via Chromium's iCloud Keychain code; needs entitlement | Cross-device passkeys through the system sheet, or none | Apple entitlement page (Observed); Chromium `icloud_keychain_sys.mm` (Observed) |
| Windows website passwords | Aeri store in Credential Manager or DPAPI; password-manager extensions | iCloud Passwords extension plus iCloud for Windows | Q5 (Observed docs; Authenticator retirement Reported) |
| Windows passkeys | Chromium's `webauthn.dll` path; Windows Hello and plugin managers | Security keys only | 6.2 (Observed) |
| Viewing a saved password after OS confirmation | `LAContext` (macOS), `UserConsentVerifier` (Windows) | Re-prompt for the OS login password | 7.2 (Inferred; not evaluated) |
| Rust secret-storage crate | `keyring-core` plus `apple-native-keyring-store` plus `windows-native-keyring-store` | `security-framework` and `windows` directly | 7.1 (Observed versions and licences) |
