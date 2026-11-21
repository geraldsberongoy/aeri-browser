#!/usr/bin/env bash
# Renders apps/aeri/resources/aeri-intro.mp4:
#   1280x720 H.264 High yuv420p, faststart, 7.4 s, no audio track.
# Usage: xtask/intro/render.sh (from anywhere; needs Node.js, npx, FFmpeg and ffprobe).
#
# Picture: black-and-white contours, ribbon, letter-by-letter wordmark, and fade.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="$repo/apps/aeri/resources/aeri-intro.mp4"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

mkdir "$work/assets" "$work/vendor"
cp "$here/index.html" "$here/hyperframes.json" "$work/"
cp "$here/vendor/gsap.min.js" "$work/vendor/"
cp "$repo/apps/aeri/resources/AppIcon.icon/Assets/glyph.png" "$work/assets/glyph.png"
( cd "$work" && npx --yes hyperframes@0.9.75 render --output raw.mp4 --fps 30 )

ffmpeg -y -v error -i "$work/raw.mp4" -map 0:v:0 -an \
  -vf "scale=1280:720:flags=lanczos,format=yuv420p" \
  -c:v libx264 -profile:v high -level 4.0 -crf 18 -preset slow -pix_fmt yuv420p -r 30 \
  -movflags +faststart "$work/aeri-intro.mp4"
ffprobe -v error -show_entries stream=codec_type,codec_name,width,height,pix_fmt,r_frame_rate,duration -of default=nw=1 "$work/aeri-intro.mp4"
mv "$work/aeri-intro.mp4" "$out"
