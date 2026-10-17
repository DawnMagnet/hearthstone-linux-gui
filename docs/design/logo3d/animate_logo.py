# Animates the logo scene built by build_logo.py: a 10 second loop (240
# frames at 24 fps) in which the fire flickers, the lanterns and stars glow
# unevenly, Tux breathes, blinks, looks over his cards and lifts the middle
# one to study it more closely. Run inside Blender after build_logo.py.
import bpy, math, random
from mathutils import Vector

scn = bpy.data.scenes["HS_Logo"]
FPS, END = 24, 240
scn.render.fps = FPS
scn.frame_start, scn.frame_end = 1, END
O = scn.objects
random.seed(7)


def empty(name, loc, parent=None):
    e = bpy.data.objects.new(name, None)
    scn.collection.objects.link(e)
    e.empty_display_size = 0.2
    e.location = loc
    if parent:
        attach(e, parent)
    return e


def attach(child, parent):
    """Parents without moving the child."""
    bpy.context.view_layer.update()
    world = child.matrix_world.copy()
    child.parent = parent
    child.matrix_parent_inverse = parent.matrix_world.inverted()
    child.matrix_world = world


def curve(owner, path, index):
    """The F-curve animating owner.path[index] (works with slotted actions)."""
    ad = owner.animation_data
    fcurves = getattr(ad.action, "fcurves", None)
    if fcurves is None:
        from bpy_extras import anim_utils
        fcurves = anim_utils.action_get_channelbag_for_slot(ad.action, ad.action_slot).fcurves
    return next(fc for fc in fcurves if fc.data_path == path and fc.array_index == index)


def keys(obj, path, index, points, interpolation="BEZIER"):
    """points: [(frame, value), ...] on the vector property obj.path[index]."""
    for frame, value in points:
        getattr(obj, path)[index] = value
        obj.keyframe_insert(path, index=index, frame=frame)
    for kp in curve(obj, path, index).keyframe_points:
        kp.interpolation = interpolation
        if interpolation == "SINE":
            kp.easing = "EASE_IN_OUT"


def noise(owner, path, index, strength, scale):
    """Keeps the current value and lets a centred noise modifier wobble it.
    owner is an ID (object, light, node tree); index is -1 for a scalar."""
    owner.keyframe_insert(path, index=index, frame=1)
    m = curve(owner, path, max(index, 0)).modifiers.new("NOISE")
    m.strength = strength
    m.scale = scale
    m.phase = random.uniform(0, 100)
    return m


def socket_path(node, name):
    i = next(k for k, s in enumerate(node.inputs) if s.name == name)
    return f'nodes["{node.name}"].inputs[{i}].default_value'


# Fire: the flames stretch, widen and sway, and the light flickers with them.
# Kept small so the flames stay inside the fireplace opening.
for name, s_z, s_x, sway in (("Flame", 0.12, 0.06, 0.04), ("FlameCore", 0.16, 0.08, 0.05)):
    o = O[name]
    noise(o, "scale", 2, s_z, 3.0)
    noise(o, "scale", 0, s_x, 4.0)
    noise(o, "rotation_euler", 1, sway, 5.0)
noise(O["FireLight"].data, "energy", -1, 90, 2.5)
for sx in (-1, 1):
    noise(O[f"LampLight_{sx}"].data, "energy", -1, 14, 6.0)
nt = bpy.data.materials["HS_Ember"].node_tree
noise(nt, socket_path(next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), "Emission Strength"), -1, 1.5, 3.0)

# Stars twinkle on their own material, the moon keeps its glow.
star_mat = bpy.data.materials.get("HS_Star") or bpy.data.materials["HS_Moon"].copy()
star_mat.name = "HS_Star"
for k in range(2):
    O[f"Star_{k}"].data.materials[0] = star_mat
nt = star_mat.node_tree
noise(nt, socket_path(next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), "Emission Strength"), -1, 4.0, 4.0)

# Rig: the head turns and nods around the neck, eyes blink and the gaze
# moves over the cards.
head = O["Tux_Head"]
neck = empty("Rig_Head", (0, 0.8, head.location.z - 0.55))
for o in [o for o in O if o.name.startswith(("Tux_Eye", "Tux_Pupil", "Tux_Glint", "Tux_Beak", "Tux_Mouth"))] + [head]:
    attach(o, neck)
for s in (-1, 1):
    eye = O[f"Tux_Eye_{s}"]
    lid = empty(f"Rig_Blink_{s}", eye.matrix_world.translation.copy(), neck)
    gaze = empty(f"Rig_Gaze_{s}", eye.matrix_world.translation.copy(), lid)
    attach(eye, lid)
    attach(O[f"Tux_Pupil_{s}"], gaze)
    attach(O[f"Tux_Glint_{s}"], gaze)

# Cards: the whole hand sways a little; the middle card rises in its own
# layer (it never tips into the cards beside it) and wiggles while Tux
# studies it.
hand = empty("Rig_Hand", (0, -0.3, 0.5))
for o in [o for o in O if o.name.startswith("Card_") and o.name != "Card_Table"]:
    attach(o, hand)
mid = empty("Rig_MiddleCard", O["Card_1_Rim"].matrix_world.translation - Vector((0, 0, 0.6)), hand)
for o in [O["Card_1_Rim"], O["Card_1_Face"], O["Card_1_Sym"]]:
    attach(o, mid)

# Breathing: three slow breaths, which also lift the head and hand slightly.
body = [O["Tux_Body"], O["Tux_Belly"]]
breath = [(f, 0.0 if (f - 1) % 80 == 0 else 1.0) for f in range(1, END + 1, 40)] + [(END, 0.0)]
for o in body:
    base = o.scale[2]
    keys(o, "scale", 2, [(f, base * (1 + 0.012 * v)) for f, v in breath], "SINE")
    base_x = o.scale[0]
    keys(o, "scale", 0, [(f, base_x * (1 + 0.006 * v)) for f, v in breath], "SINE")

# Timeline (frame: what Tux does)
#   1- 30  looks at the middle card
#  36- 66  looks at the left card, head tilts left
#  76-104  looks at the right card, head tilts right; blinks at 92
# 116-170  leans in and lifts the middle card to study it closely
# 150      blinks
# 176-200  content little nod
# 214-240  back to the start pose (seamless loop)
nz = neck.location.z
keys(neck, "location", 2, [(1, nz), (41, nz + 0.012), (81, nz), (121, nz + 0.012), (161, nz),
                           (201, nz + 0.012), (240, nz)], "SINE")
keys(neck, "rotation_euler", 1, [(1, 0), (30, 0), (44, math.radians(-5)), (66, math.radians(-5)),
                                 (82, math.radians(5)), (104, math.radians(5)), (118, 0),
                                 (170, 0), (214, 0), (240, 0)])
keys(neck, "rotation_euler", 0, [(1, 0), (110, 0), (126, math.radians(7)), (166, math.radians(7)),
                                 (178, 0), (184, math.radians(4)), (190, 0), (196, math.radians(3)),
                                 (204, 0), (240, 0)])
for s in (-1, 1):
    g = O[f"Rig_Gaze_{s}"]
    gx, gz = g.location.x, g.location.z
    keys(g, "location", 0, [(f, gx + v) for f, v in [(1, 0), (30, 0), (40, -0.055), (66, -0.055),
                                                     (78, 0.055), (104, 0.055), (116, 0), (240, 0)]])
    keys(g, "location", 2, [(f, gz + v) for f, v in [(1, 0), (110, 0), (124, -0.03), (168, -0.03),
                                                     (180, 0), (240, 0)]])
    lid = O[f"Rig_Blink_{s}"]
    blink = []
    for f in (92, 150, 222):
        blink += [(f - 3, 1.0), (f, 0.08), (f + 3, 1.0)]
    keys(lid, "scale", 2, [(1, 1.0)] + blink + [(240, 1.0)])

keys(hand, "rotation_euler", 1, [(1, 0), (44, math.radians(-2)), (66, math.radians(-2)),
                                 (82, math.radians(2)), (104, math.radians(2)), (118, 0), (240, 0)])
keys(hand, "location", 2, [(1, 0.5), (110, 0.5), (126, 0.55), (166, 0.55), (180, 0.5), (240, 0.5)])
keys(mid, "location", 2, [(1, mid.location.z), (112, mid.location.z), (128, mid.location.z + 0.12),
                          (164, mid.location.z + 0.12), (180, mid.location.z), (240, mid.location.z)])
keys(mid, "rotation_euler", 1, [(1, 0), (130, 0), (140, math.radians(-3)), (150, math.radians(3)),
                                (160, 0), (240, 0)])

# Flippers follow the hand a little.
for s in (-1, 1):
    f = O[f"Tux_Flipper_{s}"]
    z = f.location.z
    keys(f, "location", 2, [(1, z), (110, z), (126, z + 0.05), (166, z + 0.05), (180, z), (240, z)])

# Frames for the logo animation: 640 px RGBA PNGs on a transparent
# background, without raytracing (EEVEE).
scn.render.resolution_x = scn.render.resolution_y = 640
scn.eevee.taa_render_samples = 32
if hasattr(scn.eevee, "use_raytracing"):
    scn.eevee.use_raytracing = False
scn.render.image_settings.file_format = "PNG"
scn.render.image_settings.color_mode = "RGBA"

scn.frame_set(1)
print("animated", END, "frames")
