# Animated logo

[Deutsche Fassung](README.de.md)

Source of the launcher's animated logo (`data/logo/logo-loop.webm`) and of
the README animation (`docs/images/logo-animation.webp`): Tux stands behind
the counter of a tavern and plays cards, as in the drawn logo
(`data/icons/logo.png`), only modelled in 3D. It is an original work with no
Blizzard material: no logo, card frames, card art or fonts.

| File | Purpose |
| --- | --- |
| `build_logo.py` | Builds the scene `HS_Logo` (Tux, cards, counter, tavern in a round frame, lights, camera). The layout follows the SVG coordinates of the drawn logo. |
| `animate_logo.py` | Turns it into a 10-second loop (240 frames at 24 fps): the fire flickers, lanterns and stars glow unevenly, Tux breathes, blinks, looks over his cards and lifts the middle one. Sets the frame output to 640 px RGBA PNG. |
| `check_overlaps.py` | Lists objects whose surfaces cross. Every object may only lie on, hang in front of or stand behind another; parts of one thing are grouped. |
| `encode.sh` | Encodes the frames into the WebM (368 px, VP9 with alpha) and the WebP (320 px at 12 fps, kept small for the README). |

## Rebuilding

Made with Blender 5.4 (EEVEE); the scripts touch only the scene `HS_Logo`.

1. In Blender, run `build_logo.py` and then `animate_logo.py` (Scripting
   workspace, or `exec(open(path).read())` in the Python console).
2. Set the output path to a folder ending in `frame_####` (Output
   properties) and render the animation (Render > Render Animation).
3. Encode the frames: `docs/design/logo3d/encode.sh <frames folder>`.

Or in the background (rendering with EEVEE needs a display, for example
through `xvfb-run`):

```sh
xvfb-run -a blender -b --python docs/design/logo3d/build_logo.py \
  --python docs/design/logo3d/animate_logo.py \
  -S HS_Logo -o /tmp/logo-frames/frame_#### -a
docs/design/logo3d/encode.sh /tmp/logo-frames
```

To check for objects passing through each other, run `check_overlaps.py` in
the same session after the other two scripts; set `FRAMES` (a tuple of frame
numbers) first to check more than frame 1. Touching faces show up as a few
triangle pairs and are fine.
