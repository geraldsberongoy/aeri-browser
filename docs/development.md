# Development

Status: Phase 1. Commands in this file were run on the owner's Mac (macOS
27.2, Xcode 27.2, Rust 1.100, Apple Silicon) unless marked otherwise.

## Prerequisites

| Need | Detail |
| --- | --- |
| Machine | Apple Silicon Mac. CEF has no universal macOS build, so Intel needs a separate x64 binary ([cef.md §1](research/cef.md)). Aeri targets arm64 first. |
| OS | macOS 14 or later. GPUI's idle-pausing display link needs it ([gpui.md Q9](research/gpui.md)). |
| Toolchain | Full Xcode, because GPUI compiles Metal shaders at build time. Xcode 26+ ships the Metal Toolchain as a separate component: `xcodebuild -downloadComponent MetalToolchain` (839 MB). Without it the `gpui_apple` build fails with "missing Metal Toolchain". Also CMake and Ninja (`brew install cmake ninja`); `cef-dll-sys` builds CEF's C++ wrapper with them. |
| Rust | Stable, edition 2024, installed with `rustup`. |
| Disk | About 2 GB free. The CEF framework is 341 MB on disk once unpacked ([cef.md §1](research/cef.md)). Debug builds, caches and bundles add to that. |
| Network | The first build downloads CEF. A first run downloads filter lists (Phase 4). |

No Apple Developer account is needed for local runs.

## CEF binaries

Aeri uses prebuilt CEF binaries from the Spotify CDN
(`cef-builds.spotifycdn.com`), stable channel: CEF 156.0.30 (Chromium 156),
through the `cef` crate pinned at `=156.6.0` ([cef.md §1, §2](research/cef.md)).

- With no `CEF_PATH`, the `cef-dll-sys` build script downloads CEF itself
  (cef-rs README). CI relies on this.
- Locally, export CEF once and point builds at it, so `cargo clean` does not
  download it again:

  ```sh
  git clone https://github.com/tauri-apps/cef-rs /tmp/cef-rs
  (cd /tmp/cef-rs && cargo run -p export-cef-dir -- --force "$HOME/.local/share/cef")
  export CEF_PATH="$HOME/.local/share/cef"
  ```

  Cargo's `[env]` cannot expand `$HOME`, so `CEF_PATH` is exported in the
  shell, not in `.cargo/config.toml`.
- Stock CEF lacks H.264/AAC and Widevine. This is expected until Phase 14
  ([ADR 0005](decisions/0005-media-codecs-and-drm.md)).

## Workspace layout

```
apps/aeri/          the binary: GPUI windows and views, AppKit glue (mac.rs)
apps/aeri-helper/   CEF subprocess binary
crates/aeri-core/   domain model, rules, persistence; no GPUI, no CEF
crates/aeri-cef/    CEF adapter for the engine seam
xtask/              `cargo xtask bundle`, `cargo xtask isolation`
docs/               architecture, decisions, research, roadmap
.monozukuri/        blueprint (requirements, stack, plan) and phase specs
```

Where code goes:

| You are writing | Put it in |
| --- | --- |
| A rule, state machine, record or SQL migration | `crates/aeri-core` |
| Anything that names a CEF type | `crates/aeri-cef` |
| A GPUI view, menu, shortcut or AppKit call | `apps/aeri` |
| A renderer-process hook | `apps/aeri-helper` |

See [architecture/overview.md](architecture/overview.md). GPUI is a git
dependency pinned to a `zed-industries/zed` commit
([02-tech-stack.md](../.monozukuri/blueprint/02-tech-stack.md)).

## Build, bundle, run

A CEF app on macOS must be an `.app` bundle with helper bundles. A plain
`cargo run` of the main binary exits with "not inside an app bundle".

```sh
export CEF_PATH="$HOME/.local/share/cef"
cargo test --workspace
cargo xtask bundle                    # → target/bundle/aeri.app, ad-hoc signed
open target/bundle/aeri.app
```

`cargo xtask bundle` builds `aeri` and `aeri-helper`, then assembles the app,
the framework and the five helper bundles with cef-rs's `build_bundle`. It
copies Aeri's tab-signals extension into `Contents/Resources/tab-signals`
([ADR 0007](decisions/0007-tab-signals.md)), then ad-hoc signs the whole
bundle. It deletes the old bundle first: macOS
caches code signatures per file, and copying a new binary over a running or
recently run one gets it killed ("load code signature error 2").

Development switches:

| Variable or flag | Effect |
| --- | --- |
| `AERI_DATA_DIR=/tmp/aeri-data` | Use a throwaway data directory instead of `~/Library/Application Support/Aeri`. |
| `AERI_START_URL=https://example.com` | Open one window on this address per opened profile instead of restoring each profile's session. Without it, a profile with no session opens a new tab (`about:blank`, with the address bar shown). |
| `AERI_REMOTE_DEBUGGING_PORT=9334` | Expose the DevTools protocol for scripted checks. |
| `AERI_PROFILE=Work,Client` | Open (or create) these profiles, each in its own window, instead of the last-used one. For scripted checks; the Profiles menu is the Person's way. |
| `AERI_LOG=info,aeri=debug` | `tracing` filter (stdout; colours only on a terminal). |
| `--use-mock-keychain` | Chromium switch: skip the Keychain. Use it for scripted runs; see below. |
| `--autoplay-policy=no-user-gesture-required` | Chromium switch: let a script play sound without a click (tab-signals checks). |

Camera and microphone checks: macOS attributes the permission prompt to the
app that launched Aeri (your terminal), not to Aeri, even with Chromium's
fake-device switches. Run them from `open target/bundle/aeri.app` instead.

Run the bundle binary directly to see logs:

```sh
AERI_DATA_DIR=/tmp/aeri-data AERI_REMOTE_DEBUGGING_PORT=9334 \
  ./target/bundle/aeri.app/Contents/MacOS/aeri --use-mock-keychain
```

`kill <pid>` (SIGTERM) and Ctrl-C quit like the Quit menu: browsers close
and CEF shuts down, so cookies are written to disk, and each open profile's
session is saved. Aeri replaces Chromium's own handlers, which end the
process at once and lose recent cookies. Closing the last window does not
quit; the Dock icon opens a window again (that profile's session).

Keys (from a page or the address bar): ⌘L address bar, ⇧⌘P command
palette, ⌘T new tab, ⌘N new window (on a space no window shows), ⌘W close
tab, ⌘⇧T reopen the last closed tab, ⌃Tab / ⌃⇧Tab next and previous tab,
⌘1–⌘8 that tab (favourites first), ⌘9 the last tab, ⌘⌥← / ⌘⌥→ previous and
next space, ⌘D bookmark the page (again: remove it), ⌥⌘B bookmarks, ⌘Y
history, ⌘S show or hide the sidebar, ⌃⌘F full screen, ⌘Q quit. In the bar,
↑ / ↓ pick a suggestion. A profile's records (spaces, favourites, windows,
closed tabs, history, bookmarks) are in `Profiles/<id>/profile.db` under the
data directory; `sqlite3` reads it once Aeri has quit.

Running under `lldb` (launch or attach) raises a system "Developer Tools
Access" dialog and stalls until it is answered. Aeri also needs fds 3 and 4
free at start for its private DevTools pipe (ADR 0006); if a parent process
leaves them open, Aeri logs "no DevTools pipe" and extension buttons do
nothing.

### Keychain

Chromium encrypts cookies and saved passwords with a key it keeps in the
Keychain. Aeri uses its own item, "Aeri Safe Storage" (account "Aeri"),
through CEF's `keychain_service_name` settings. The default item, "Chromium
Safe Storage", is shared by every Chromium-based app. If an old build asks
for "Chromium Safe Storage", deny it.

Each rebuild changes the ad-hoc signature, so macOS may ask again whether
`aeri` may use "Aeri Safe Storage". While that prompt is open, page loads
stall. For scripted runs, pass `--use-mock-keychain` with a throwaway
`AERI_DATA_DIR`.

## Signing

- **Local runs:** `cargo xtask bundle` ad-hoc signs the bundle
  (`codesign --force --deep --sign -`); the hardened runtime is off.
  `codesign --verify --deep --strict target/bundle/aeri.app` passes.
  Chromium logs "Signature validation of current process failed (-67030)"
  at start-up on ad-hoc builds. It has no visible effect so far; recheck it
  when real signing lands.
- **Release:** hardened runtime, the JIT entitlement on the renderer and GPU
  helpers (Reported in [cef.md §8](research/cef.md)), notarisation
  (planned, Phase 15).
- Ad-hoc builds have no passkey entitlement. Platform passkeys and Apple
  Passwords do not work in them ([cef.md §9](research/cef.md),
  [blocking-and-credentials.md Q4](research/blocking-and-credentials.md)).
- Bundle identity matters: macOS ties notification permission to the bundle
  id `io.github.kuya-egg.aeri` ([cef.md §8](research/cef.md)).

## Logs

Nothing is sent anywhere.

- Aeri's own logs: `tracing` to stderr, filtered by `AERI_LOG` (default
  `info`). A rotating log file is not set up yet.
- Chromium's log: `<data dir>/chromium.log`.
- Errors: `thiserror` in libraries, `anyhow` at the app edge.

## Test strategy

From handoff §11 and the stack map.

| Level | Where | How |
| --- | --- | --- |
| Unit | `crates/aeri-core` | Profile lifecycle, tab and space state, session serialisation, navigation state, address classification, download-path cleaning, migrations. Each runs on an **in-memory SQLite**. No GPUI, no CEF. |
| Integration | `crates/aeri-cef` and `apps/aeri` | Chromium lifecycle, request-context isolation, navigation events, downloads, permissions, popups, crashes, session restore. Run against **local test pages** served from the repo. |
| End to end | `cargo xtask isolation` (profiles); more to come | Two profiles on a local site the task serves: sign-ins, cookies, local storage, IndexedDB, HTTP cache, restart, dormancy. Runs the bundled app; needs a logged-in macOS session. Not in CI yet. |

Rules:

- Automated tests never depend on an external website.
- Each stage-1 edge case becomes a test in the module that owns the rule
  ([03-logic-to-stack-map.md](../.monozukuri/blueprint/03-logic-to-stack-map.md)).
- Chromium-owned behaviour (popups, certificates, isolation) is tested at the
  integration level, not mocked.
- Spikes S1–S7 are experiments with written pass criteria, not tests. See
  the [roadmap](roadmap.md).

CI is `.github/workflows/ci.yml` on a macOS arm64 runner: fmt, clippy with
`-D warnings`, tests, bundle, signature check. It has not run on GitHub yet.
`cargo-deny` licence and advisory checks are not added yet.

## Repository conventions

| Thing | Location |
| --- | --- |
| Architecture docs | `docs/architecture/` |
| Decisions (ADRs, numbered `NNNN-title.md`) | `docs/decisions/` |
| Research and evidence | `docs/research/` |
| Phases | [`docs/roadmap.md`](roadmap.md) |
| Blueprint: requirements, stack, stack map, phase plan | `.monozukuri/blueprint/` |

- One convention for ADRs: `docs/decisions/`, not `.monozukuri/`.
- Research claims carry a label: Observed, Reported or Inferred. Keep it when
  you cite them.
- A decision that changes the architecture needs a new ADR or an update to an
  existing one. Do not silently change a source document.
- Add a crate only when an ownership boundary appears (handoff §9).
- Never commit secrets, signing keys or notarisation credentials.
