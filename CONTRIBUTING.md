# Contributing to Aeri

Bug reports, documentation fixes, and code contributions are welcome. Be respectful and keep feedback focused on the work.

## Open an issue

- Search [existing issues](https://github.com/kuya-egg/aeri-browser/issues) before opening a new one.
- For a bug, include the Aeri version or commit, macOS version, steps to reproduce, and what you expected versus what happened. For a site-specific issue, include a public URL or a minimal example if you can share it.
- For a larger feature or architecture change, open an issue first to discuss the scope. Small fixes can go straight to a pull request.
- Don’t post passwords, cookies, tokens, private URLs, or unsanitized logs and screenshots. For a suspected security or privacy vulnerability, don’t post the details publicly. Use the repository’s [Security tab](https://github.com/kuya-egg/aeri-browser/security) if private reporting is available; otherwise, open an issue asking for a private contact without describing the vulnerability.

## Prepare a pull request

1. Fork the repo, branch from `main`, and follow the [development guide](docs/development.md) to build Aeri on an Apple Silicon Mac.
2. Keep the PR focused on one change. Add a regression test for bug fixes where practical, and use local fixtures instead of live websites in automated tests.
3. Run the checks below for code changes. For UI changes, launch the bundle and include screenshots or a short recording. Describe what you tested and anything you couldn’t verify.
4. Link the related issue if there is one. Update the relevant docs or changelog for user-visible changes. Keep third-party license notices and attribute borrowed code and assets.

## Run the checks

These are the checks in [CI](.github/workflows/ci.yml):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask bundle
codesign --verify --deep --strict target/bundle/aeri.app
```

For profile or cookie-isolation changes, also run `cargo xtask isolation` from a logged-in macOS session after bundling. Documentation-only changes don’t need a full build; check the rendered Markdown, relative links, and commands you changed.

By submitting a contribution, you agree to license it under the project’s [Apache License 2.0](LICENSE).
