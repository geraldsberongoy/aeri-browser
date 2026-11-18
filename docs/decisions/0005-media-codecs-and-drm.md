# ADR 0005: Media codecs and DRM

- Status: accepted
- Date: 2026-11-18
- Deciders: owner (kuya-egg); principal engineer (agent)

## Context

Chrome-compatible media needs H.264/AAC playback and Widevine DRM. Evidence
is in `docs/research/codecs-drm.md` and `docs/research/cef.md` §1.

**What the stock CEF build does today:**
- It cannot play H.264 or AAC.
- Widevine does download into it through the component updater, and test
  content played.
- That Widevine copy is not VMP-signed. castLabs reports `PLATFORM_TAMPERED`,
  and Netflix and Disney+ typically refuse such clients.

**Options outside the stock build:**
- CEF's maintainer does not publish codec builds.
- Helium, Brave and ungoogled-chromium all build with
  `proprietary_codecs=true ffmpeg_branding=Chrome`.
- Vivaldi uses OS decoders through a large patch set.

**Licensing:**
- Via LA AVC: free up to 100,000 units per year.
- Via LA AAC: $0.98 per unit from the first unit.
- Official Widevine signing needs a Google agreement and a paid castLabs
  audit.

## Options considered

1. **Stock CEF only.**
   - Pros: no build pipeline.
   - Cons: H.264/AAC sites fail, which is a visible compatibility gap.
2. **Self-built CEF with FFmpeg proprietary codecs (Helium's route).**
   - Pros: H.264/AAC everywhere.
   - Cons:
     - 2–6 h builds, 150–200 GB of disk, 16–32 GB of RAM, repeated monthly.
     - Needs a dedicated Mac or paid large runners.
     - Patent exposure.
3. **Self-built CEF with OS decoders (Vivaldi's route).**
   - Pros: the OS vendor carries the patent licence.
   - Cons:
     - A large Chromium patch set to maintain.
     - AAC is not covered by Chromium's platform path.
     - CEF upstream has declined it for 3+ years.
4. **Widevine with VMP signing.**
   - Pros: Netflix-grade playback.
   - Cons: needs a Google agreement and a castLabs audit, so realistically
     needs a company.

## Decision

- **Early phases:** stock CEF.
- **Before the first public release:** a dedicated phase moves to a
  self-built CEF (option 2) with `proprietary_codecs=true` and
  `ffmpeg_branding=Chrome`, with HEVC left at the platform default. The build
  machine is chosen then, and licensing gets legal advice then.
- **Widevine:**
  - Ship it as the stock component updater delivers it.
  - Detect DRM refusals on known services and offer "Open in system browser".
  - VMP signing is deferred until there is a company.

## Why this one

It reaches Helium-level media compatibility, which is the bar the owner set,
without slowing the early phases that prove the architecture. It also avoids
a long-lived patch set like option 3.

## Tradeoffs and consequences

- **Owning a Chromium build.** From the codec phase on, Aeri owns a CEF build
  pipeline: monthly rebuilds, a build machine and longer security-update
  turnaround. Pinning `CEF_API_VERSION` keeps code churn low.
- **Patent licensing.** Exposure for H.264/AAC is the same as Helium's
  unless licences are taken out. Legal advice is required before the public
  release.
- **DRM services.** Netflix and Disney+ will refuse playback until VMP
  signing exists. The fallback keeps the Person moving.
- **Rendering independence.** ADR 0002 does not depend on this choice; the
  window model is the same with stock or self-built CEF.
