# Aeri — project memory

## What it is
Open-source, native desktop web browser. Mac first (Apple Silicon), then
Windows. It is compatible with Chrome (sites and Chrome Web Store
extensions), page-first and polished like Search and Arc, customisable like
Zen, and lighter than Helium. Many isolated profiles can be open at once,
one window per profile, inside one running app.

Framed intent: `docs/handoff.md`. Approved requirements:
`.monozukuri/blueprint/01-business-logic.md`.

## Actors
Person (sole human, all privileges) · Website (untrusted) · Extension
(semi-trusted, per profile) · Web engine (external) · Operating system.

## Current phase
Blueprint, stage 2 (tech stack). Stage 1 was approved on 2026-11-18.

Research in progress, in `docs/research/`: `gpui.md`, `cef.md`,
`blocking-and-credentials.md`.

## Known risks carried into stage 2
- Chrome extensions vs a GPUI-drawn UI. On macOS, CEF browsers embedded in a
  parent view are reported to run in "Alloy style", which has no extension
  UI or Web Store install. Source: secondary report, verification in
  progress.
- Classic uBlock Origin (MV2) is reported unrunnable on current Chromium. The
  blocking requirement is stated by filter-list quality, not by that
  extension.
- No public macOS API for third-party browsers to read Apple Passwords
  (Observed in Search's source comments; verification in progress).
