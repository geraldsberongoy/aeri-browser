<p align="center">
  <img src="apps/aeri/resources/AppIcon.icon/Assets/glyph.png" width="96" height="96" alt="Aeri ribbon">
</p>

<h1 align="center">Aeri</h1>

<p align="center">
  A native macOS browser built with Rust and GPUI, powered by Chromium through CEF.
</p>

<p align="center">
  Apple Silicon · Early development<br>
  <a href="#build-it-yourself">Build from source</a> · <a href="CONTRIBUTING.md">Contribute</a> · <a href="CHANGELOG.md">What’s new</a>
</p>

Aeri is currently built from source; there is no published installer yet.
Intel Macs and Windows are not supported by the current build.

## Your tabs, your way.

Put your tabs in a sidebar on the left or right, across the top, or in a compact icons-only column. Let the bar hide until your pointer reaches the edge, group work and personal browsing into **Spaces**, and keep favourites pinned above them.

## Two pages, one window.

Option-click any tab to pair it beside the active page in **Split View**. Both tabs stay live, drag the divider to give either one more room, or press `⌥⌘N` to start or separate a split without closing a thing.

## Peek before you commit.

Shift-click any link to preview it right over the page you’re on. Press `Esc` to dismiss it, `⌘Return` to keep it as a tab, or `⌥⌘Return` to open it beside its opener in Split View.

## Refuses to leave the picture.

Switch tabs or macOS Spaces while a video is playing and it glides out into a floating window above everything, then slips back into its tab when you return. Drag, pinch to resize, or flick with two fingers to dock it to a corner or side — or press `⇧⌘P` on any page.

## Extension support and separate profiles

Aeri offers **uBlock Origin Lite** during profile setup and supports extensions from the Chrome Web Store. Profiles keep cookies, history, extensions, and saved passwords separate.

## Also in there

- **Aeri Home.** Blank tabs open to a clean, native canvas with the Aeri ribbon, inline search, and quick access to Bookmarks, History, and Settings.
- **Small windows.** Links from other apps, sized pop-ups, and launcher addresses can open in a lightweight window with **Open in Aeri** (`⌘O`) to pull the live page into your main window without reloading.
- **Reading Mode.** `⇧⌘R` strips an article down to clean reading with light/dark controls and Markdown copy.
- **Recent tab previews.** Hold `Control-Tab` to visually cycle recent tabs; release to switch.
- **Floating address bar & command palette.** `⌘L` opens a centered address bar with history and bookmark suggestions; `⌥⌘P` searches commands, tabs, Spaces, and profiles.
- **Themes & per-site styles.** Light, dark, or automatic themes per Space, native macOS window materials, importable theme files, and a built-in CSS editor for any site.
- **Rests quietly.** Background tabs sleep when unused (never tabs playing audio or sharing your screen), with Low Power motion and reduced-animation options built in.
- **Native macOS craft.** Real macOS menu bar with live Profiles, smooth spring motion, high-contrast text options, and a clean drag-to-Applications DMG installer.

## Media compatibility

The standard build uses stock CEF and does not support H.264/AAC, so some
videos won’t play. DRM playback is a separate limitation: services that require
signed Widevine verification may refuse to play. See the
[media compatibility decision](docs/decisions/0005-media-codecs-and-drm.md).

---

## Build it yourself

Aeri is a Rust workspace pairing a GPUI macOS shell with Chromium Embedded Framework (CEF).

```text
apps/
  aeri/               Native macOS GPUI app, views, menus, shortcuts, AppKit glue
  aeri-helper/        CEF subprocess helper binary
crates/
  aeri-core/          Domain model, layout/theme rules, SQLite persistence (no GPUI, no CEF)
  aeri-cef/           CEF engine adapter
xtask/                Bundle, DMG installer, and isolation tooling
docs/                 Architecture, ADRs, research, and development notes
```

### Prerequisites

- **Apple Silicon Mac** running **macOS 14 Sonoma** or later
- **Full Xcode** with the **Metal Toolchain** installed (`xcodebuild -downloadComponent MetalToolchain` on Xcode 26 or newer)
- **Rust** stable (edition 2024)
- **CMake** and **Ninja** (`brew install cmake ninja`)

### Build & run

CEF apps on macOS must run from a signed `.app` bundle (plain `cargo run` will exit):

```bash
git clone https://github.com/kuya-egg/aeri-browser.git
cd aeri-browser
cargo xtask bundle                         # -> target/bundle/aeri.app (ad-hoc signed)
open target/bundle/aeri.app
```

The first build downloads the pinned CEF binaries automatically. You don’t need
to compile Chromium from source for the standard build.

To build the release macOS installer DMG:

```bash
cargo xtask bundle --release
cargo xtask dmg                            # -> target/release-bundle/Aeri.dmg
```

See the [development guide](docs/development.md) for CEF caching, local testing, and troubleshooting.

## Contributing

Bug reports, documentation fixes, and code contributions are welcome. Read
[CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull request.

## License

Aeri is licensed under [Apache License 2.0](LICENSE). Portions adapted from
Search retain their [MIT license notice](apps/aeri/resources/Search-LICENSE.txt).
