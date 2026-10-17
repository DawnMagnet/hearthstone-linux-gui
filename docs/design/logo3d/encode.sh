#!/bin/sh
# Encodes the rendered frames (frame_0001.png ... frame_0240.png, 640 px RGBA)
# into the launcher's looping logo and the README animation. The crop is
# the area all frames use, so the logo fills the picture like the drawn one.
#
#   docs/design/logo3d/encode.sh <frames directory>
set -eu
frames="${1:?usage: encode.sh <frames directory>}"
root="$(cd "$(dirname "$0")/../../.." && pwd)"
crop="crop=558:558:41:20"

ffmpeg -y -framerate 24 -i "$frames/frame_%04d.png" \
  -vf "$crop,scale=368:368:flags=lanczos" \
  -c:v libvpx-vp9 -pix_fmt yuva420p -b:v 0 -crf 30 -auto-alt-ref 0 -row-mt 1 \
  "$root/data/logo/logo-loop.webm"

ffmpeg -y -framerate 24 -i "$frames/frame_%04d.png" \
  -vf "$crop,scale=320:320:flags=lanczos" \
  -c:v libwebp_anim -lossless 0 -q:v 70 -compression_level 6 -loop 0 -pix_fmt yuva420p \
  "$root/docs/images/logo-animation.webp"
