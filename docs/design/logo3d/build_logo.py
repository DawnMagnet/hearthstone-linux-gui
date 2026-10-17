# Builds the 3D launcher logo in a scene called HS_Logo: Tux stands behind a
# serving counter in a tavern, seen through a round wooden frame, and plays
# cards. The layout follows data/icons/io.github.hearthstone_linux_gui.svg;
# SVG coordinates are mapped to the scene with svg_xz(). Run inside Blender;
# it never touches other scenes.
import bpy, bmesh, math, re
from mathutils import Vector

scn = bpy.data.scenes.get("HS_Logo")
if scn is None:
    # Without a window (blender -b) the active scene cannot be switched, so
    # the default scene becomes HS_Logo; operators add objects to it.
    scn = bpy.data.scenes.new("HS_Logo") if bpy.context.window else bpy.context.scene
    scn.name = "HS_Logo"
if bpy.context.window:
    bpy.context.window.scene = scn
for o in list(scn.collection.all_objects):
    bpy.data.objects.remove(o, do_unlink=True)

U = 0.023          # scene units per SVG unit
RING_Z = 2.0       # SVG circle centre (128, 122) maps to x=0, z=RING_Z
TUX_DZ = 0.1       # Tux and his cards sit a little higher than in the SVG
COUNTER_Z = 0.2    # height of the counter top


def svg_xz(x, y):
    return ((x - 128) * U, RING_Z + (122 - y) * U)


def tux_xz(x, y):
    """SVG point inside Tux's group (scaled 0.86 around (128, 214))."""
    x, z = svg_xz(128 + (x - 128) * 0.86, 214 + (y - 214) * 0.86)
    return x, z + TUX_DZ


def path_points(d, steps=10):
    """Samples an absolute SVG path made of M, L, H, V, Q and z commands."""
    tokens = re.findall(r"[MLHVQz]|-?[\d.]+", d)
    pts, cur, i, cmd = [], (0.0, 0.0), 0, None
    while i < len(tokens):
        if tokens[i].isalpha():
            cmd = tokens[i]
            i += 1
            if cmd == "z":
                continue
        num = lambda k: float(tokens[i + k])
        if cmd in ("M", "L"):
            cur = (num(0), num(1))
            pts.append(cur)
            i += 2
        elif cmd == "H":
            cur = (num(0), cur[1])
            pts.append(cur)
            i += 1
        elif cmd == "V":
            cur = (cur[0], num(0))
            pts.append(cur)
            i += 1
        elif cmd == "Q":
            c, e = (num(0), num(1)), (num(2), num(3))
            for k in range(1, steps + 1):
                t = k / steps
                pts.append(tuple((1 - t) ** 2 * cur[j] + 2 * (1 - t) * t * c[j] + t * t * e[j] for j in (0, 1)))
            cur = e
            i += 4
    if len(pts) > 2 and (Vector(pts[0]) - Vector(pts[-1])).length < 1e-6:
        pts.pop()
    return pts


def mat(name, color, rough=0.4, metal=0.0, coat=0.0, emit=None, es=0.0):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    m.use_nodes = True
    b = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    b.inputs["Base Color"].default_value = (*color, 1)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    b.inputs["Coat Weight"].default_value = coat
    b.inputs["Emission Color"].default_value = (*(emit or (0, 0, 0)), 1)
    b.inputs["Emission Strength"].default_value = es
    return m


def clear_nodes(nt, *types):
    for n in [n for n in nt.nodes if n.type in types]:
        nt.nodes.remove(n)


def brick_mat(name, c1, c2, mortar, scale, width, row, mortar_size, rough=0.85, offset_y=0.0):
    """Stone blocks or wall planks on the XZ plane (objects need applied transforms)."""
    m = mat(name, c1, rough)
    nt = m.node_tree
    clear_nodes(nt, "TEX_BRICK", "TEX_COORD", "MAPPING")
    b = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Rotation"].default_value = (math.radians(90), 0, 0)  # world z -> texture y
    mp.inputs["Location"].default_value = (0, offset_y, 0)
    br = nt.nodes.new("ShaderNodeTexBrick")
    br.inputs["Color1"].default_value = (*c1, 1)
    br.inputs["Color2"].default_value = (*c2, 1)
    br.inputs["Mortar"].default_value = (*mortar, 1)
    br.inputs["Scale"].default_value = scale
    br.inputs["Brick Width"].default_value = width
    br.inputs["Row Height"].default_value = row
    br.inputs["Mortar Size"].default_value = mortar_size
    nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
    nt.links.new(mp.outputs["Vector"], br.inputs["Vector"])
    nt.links.new(br.outputs["Color"], b.inputs["Base Color"])
    return m


def feather_mat(name, color, rough, sheen, sheen_tint, bump):
    """Matte plumage: soft sheen and a fine bump running downwards."""
    m = mat(name, color, rough)
    nt = m.node_tree
    clear_nodes(nt, "TEX_NOISE", "BUMP", "TEX_COORD", "MAPPING")
    b = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    b.inputs["Specular IOR Level"].default_value = 0.25
    b.inputs["Sheen Weight"].default_value = sheen
    b.inputs["Sheen Roughness"].default_value = 0.4
    b.inputs["Sheen Tint"].default_value = (*sheen_tint, 1)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (1, 1, 3.5)
    nz = nt.nodes.new("ShaderNodeTexNoise")
    nz.inputs["Scale"].default_value = 60
    nz.inputs["Detail"].default_value = 4
    bp = nt.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = bump
    bp.inputs["Distance"].default_value = 0.02
    nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
    nt.links.new(mp.outputs["Vector"], nz.inputs["Vector"])
    nt.links.new(nz.outputs["Fac"], bp.inputs["Height"])
    nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    return m


def select_only(o):
    bpy.context.view_layer.objects.active = o
    for other in scn.objects:
        other.select_set(other == o)


def apply_transform(o):
    select_only(o)
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)


def add(op, name, loc, scale=(1, 1, 1), rot=(0, 0, 0), material=None, smooth=False, **kw):
    op(location=(0, 0, 0), **kw)
    o = bpy.context.active_object
    o.name = name
    o.scale = scale
    o.location = loc
    o.rotation_euler = rot
    if material:
        o.data.materials.append(material)
    if smooth:
        bpy.ops.object.shade_smooth()
    return o


def bevel(o, w=0.03, seg=3):
    m = o.modifiers.new("Bevel", "BEVEL")
    m.width = w
    m.segments = seg
    m.limit_method = "ANGLE"


def box(name, loc, half, material, bev=0.0, rot=(0, 0, 0), applied=False):
    o = add(bpy.ops.mesh.primitive_cube_add, name, loc, half, rot, material)
    if applied:
        apply_transform(o)
    if bev:
        bevel(o, bev)
    return o


def sphere(name, loc, scale, material, seg=48, rot=(0, 0, 0)):
    return add(bpy.ops.mesh.primitive_uv_sphere_add, name, loc, scale, rot, material=material,
               smooth=True, segments=seg, ring_count=seg // 2)


def prism(name, pts, y, depth, material, to_xz=svg_xz):
    """Extrudes an SVG outline along +y from y to y + depth."""
    bm = bmesh.new()
    verts = []
    for p in pts:
        x, z = to_xz(*p)
        verts.append(bm.verts.new((x, y, z)))
    f = bm.faces.new(verts)
    ext = bmesh.ops.extrude_face_region(bm, geom=[f])
    bmesh.ops.translate(bm, vec=(0, depth, 0),
                        verts=[v for v in ext["geom"] if isinstance(v, bmesh.types.BMVert)])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    scn.collection.objects.link(o)
    o.data.materials.append(material)
    return o


def boolean(o, cutter, operation):
    m = o.modifiers.new(operation.title(), "BOOLEAN")
    m.operation = operation
    m.object = cutter
    m.solver = "EXACT"


def origin_to_base(o):
    """Moves the origin of a mesh to the bottom centre of its bounds."""
    vs = [v.co for v in o.data.vertices]
    base = Vector(((min(v.x for v in vs) + max(v.x for v in vs)) / 2,
                   (min(v.y for v in vs) + max(v.y for v in vs)) / 2, min(v.z for v in vs)))
    for v in o.data.vertices:
        v.co -= base
    o.location += base
    return o


def smile(o, k):
    """Bends a mesh so that its ends curve upwards by k * x**2."""
    apply_transform(o)
    for v in o.data.vertices:
        v.co.z += k * v.co.x ** 2


C = lambda h: tuple(((int(h[i:i + 2], 16) / 255 + 0.055) / 1.055) ** 2.4 for i in (1, 3, 5))  # sRGB hex

M = dict(
    black=feather_mat("HS_TuxBlack", (0.008, 0.009, 0.012), 0.45, 0.2, (0.6, 0.62, 0.7), 0.12),
    white=feather_mat("HS_TuxWhite", (0.9, 0.9, 0.92), 0.7, 0.15, (1, 1, 1), 0.06),
    orange=mat("HS_Beak", C("#ffb52a"), 0.45, coat=0.1),
    orange_dark=mat("HS_BeakDark", C("#e07b0c"), 0.5),
    mouth=mat("HS_Mouth", (0.18, 0.03, 0.01), 0.6),
    pupil=mat("HS_Pupil", (0.006, 0.006, 0.008), 0.15, coat=1.0),
    eye=mat("HS_EyeWhite", (0.95, 0.95, 0.95), 0.25, coat=0.6),
    card=mat("HS_Card", C("#f8efdc"), 0.45, coat=0.3),
    gold=mat("HS_Gold", C("#c9932a"), 0.3, metal=0.85),
    gold_light=mat("HS_GoldLight", C("#f3c64f"), 0.3, metal=0.8),
    blue=mat("HS_Blue", C("#2f7fe8"), 0.35, coat=0.4),
    red=mat("HS_Red", C("#e5533a"), 0.35, coat=0.4),
    green=mat("HS_Green", C("#3aa35a"), 0.35, coat=0.4),
    cardback=mat("HS_CardBack", C("#2a6fd8"), 0.35, coat=0.4),
    wood=mat("HS_Wood", C("#a8602b"), 0.5, coat=0.2),
    woodtop=mat("HS_WoodTop", C("#d48c48"), 0.45, coat=0.25),
    wall=brick_mat("HS_Wall", C("#7a4220"), C("#6c3a1b"), C("#3a1d0b"), 1.0, 40.0, 54 * U,
                   0.012, 0.75, offset_y=-0.64),
    beam=mat("HS_Beam", C("#4a2810"), 0.65),
    iron=mat("HS_Iron", (0.03, 0.025, 0.02), 0.45, metal=0.7),
    stone=brick_mat("HS_Stone", C("#a39a8e"), C("#8a8276"), C("#4d463f"), 1.0, 28 * U, 27 * U, 0.03),
    soot=mat("HS_Soot", (0.012, 0.006, 0.004), 0.95),
    fire=mat("HS_Fire", C("#f2691b"), 0.5, emit=C("#f2691b"), es=4.0),
    fire_core=mat("HS_FireCore", C("#ffd23f"), 0.5, emit=C("#ffd23f"), es=5.0),
    ember=mat("HS_Ember", (0.8, 0.15, 0.02), 0.6, emit=(1.0, 0.25, 0.03), es=2.5),
    night=mat("HS_Night", C("#2d55a8"), 0.3, emit=C("#2d55a8"), es=1.2),
    moon=mat("HS_Moon", C("#fff4c2"), 0.3, emit=C("#fff4c2"), es=5),
    lamp=mat("HS_Lamp", C("#ffc84a"), 0.3, emit=C("#ffb830"), es=1.2),
    mug=mat("HS_Mug", C("#c9a26a"), 0.4, coat=0.3),
    foam=mat("HS_Foam", C("#fff6e0"), 0.6),
    frame=mat("HS_Frame", C("#b06a32"), 0.4, coat=0.3),
)

# Round frame and the back wall it shows. Nothing may pass through anything
# else: every object lies on, hangs in front of or stands behind another.
R = 2.3
WALL_Y = 2.2       # front face of the back wall; wall fittings end here
add(bpy.ops.mesh.primitive_torus_add, "Frame", (0, 2.0, RING_Z), (1, 1, 1), (math.radians(90), 0, 0),
    M["frame"], True, major_radius=R, minor_radius=0.16, major_segments=128, minor_segments=24)
wall = add(bpy.ops.mesh.primitive_cylinder_add, "Wall", (0, WALL_Y + 0.1, RING_Z), (R + 0.02, R + 0.02, 0.1),
           (math.radians(90), 0, 0), M["wall"], vertices=128)
apply_transform(wall)
# Wall fittings are cut just inside the frame's inner edge, so they end
# behind the frame instead of running into it.
clip = add(bpy.ops.mesh.primitive_cylinder_add, "Clip", (0, 2.0, RING_Z), (R - 0.2, R - 0.2, 1.0),
           (math.radians(90), 0, 0), vertices=128)
clip.hide_render = True
clip.display_type = "WIRE"


def on_wall(o):
    boolean(o, clip, "INTERSECT")
    return o


def cutter(name, pts, y, depth):
    o = prism(name, pts, y, depth, M["soot"])
    o.hide_render = True
    o.display_type = "WIRE"
    return o


# Ceiling beam on the wall. The lanterns hang from iron brackets fixed to
# the wall, clear of the frame, the window and the fireplace.
beam_bottom = svg_xz(0, 40)[1]
on_wall(box("Beam", (0, WALL_Y - 0.2, (svg_xz(0, 14)[1] + beam_bottom) / 2), (2.5, 0.2, 13 * U), M["beam"]))
LANTERN_Y = 1.85
for sx in (-1, 1):
    x, z_lamp = 1.3 * sx, 3.1
    arm_z = z_lamp + 0.36
    box(f"LanternPlate_{sx}", (x, WALL_Y - 0.015, arm_z), (0.05, 0.015, 0.09), M["iron"], 0.005)
    box(f"LanternArm_{sx}", (x, (WALL_Y - 0.03 + LANTERN_Y - 0.02) / 2, arm_z),
        (0.018, (WALL_Y - 0.03 - LANTERN_Y + 0.02) / 2, 0.018), M["iron"])
    add(bpy.ops.mesh.primitive_cylinder_add, f"LanternRod_{sx}", (x, LANTERN_Y, (arm_z - 0.018 + z_lamp + 0.24) / 2),
        (1, 1, 1), material=M["iron"], vertices=12, radius=0.015, depth=arm_z - 0.018 - z_lamp - 0.24)
    # Glass body with an iron cap, base plate and corner posts around it.
    box(f"Lantern_{sx}", (x, LANTERN_Y, z_lamp), (0.14, 0.07, 0.18), M["lamp"])
    box(f"LanternCap_{sx}", (x, LANTERN_Y, z_lamp + 0.21), (0.19, 0.1, 0.03), M["iron"], 0.01)
    box(f"LanternBase_{sx}", (x, LANTERN_Y, z_lamp - 0.205), (0.17, 0.09, 0.025), M["iron"], 0.01)
    for px in (-1, 1):
        for py in (-1, 1):
            box(f"LanternPost_{sx}_{px}_{py}", (x + px * 0.155, LANTERN_Y + py * 0.085, z_lamp),
                (0.015, 0.015, 0.18), M["iron"])

# Fireplace on the left: a stone arch against the wall with the fire in
# its opening.
stone = prism("Fireplace", path_points("M22 204 V112 Q22 84 52 84 Q82 84 82 112 V204z"), 1.75, 0.45, M["stone"])
opening_pts = path_points("M32 204 V128 Q32 106 52 106 Q72 106 72 128 V204z")
boolean(stone, cutter("FireOpening", opening_pts, 1.6, 0.7), "DIFFERENCE")
on_wall(stone)
on_wall(prism("FireBack", opening_pts, WALL_Y - 0.015, 0.01, M["soot"]))
fx, _ = svg_xz(52, 0)
# A raised stone hearth fills the bottom of the opening; two logs lie side
# by side on it and the fire burns on top of them, where Tux's flipper
# does not hide it. The flames fit inside the opening even at full flicker.
HEARTH_Z = 0.9
x0, x1 = svg_xz(32, 0)[0], svg_xz(72, 0)[0]
z0 = svg_xz(0, 204)[1]
on_wall(box("Hearth", ((x0 + x1) / 2, (1.75 + WALL_Y) / 2, (z0 + HEARTH_Z) / 2),
            ((x1 - x0) / 2, (WALL_Y - 1.75) / 2, (HEARTH_Z - z0) / 2), M["stone"], applied=True))
for k, y in enumerate((1.9, 2.06)):
    on_wall(add(bpy.ops.mesh.primitive_cylinder_add, f"Log_{k}", (fx, y, HEARTH_Z + 0.07), (1, 1, 1),
                (0, math.radians(90), 0), M["beam"], vertices=16, radius=0.07, depth=0.7))
for name, d, y, depth, m in [
        ("Flame", "M38 200 Q30 176 44 160 Q44 174 52 170 Q48 150 60 138 Q60 156 68 166 Q76 180 66 200z",
         1.94, 0.06, M["fire"]),
        ("FlameCore", "M44 200 Q40 184 50 174 Q52 184 56 180 Q56 168 62 176 Q68 188 60 200z",
         1.89, 0.04, M["fire_core"])]:
    o = on_wall(origin_to_base(prism(name, path_points(d), y, depth, m)))
    o.location.x += fx - o.location.x
    o.location.z = HEARTH_Z + 0.15  # just above the logs, also while swaying
    o.scale = (0.8, 1, 0.8)

# Window on the wall: a frame with an arched opening, the night sky behind
# the glazing bars, the moon and two stars on the sky.
win_pts = path_points("M188 144 V96 Q188 76 206 76 Q224 76 224 96 V144z")
wframe = prism("WindowFrame", path_points("M182 150 V96 Q182 70 206 70 Q230 70 230 96 V150z"),
               WALL_Y - 0.2, 0.2, M["beam"])
boolean(wframe, cutter("WindowOpening", win_pts, WALL_Y - 0.3, 0.4), "DIFFERENCE")
on_wall(wframe)
on_wall(prism("WindowPane", win_pts, WALL_Y - 0.03, 0.02, M["night"]))
mx, mz = svg_xz(206, 110)
on_wall(box("Mullion_V", (mx, WALL_Y - 0.12, mz), (2 * U, 0.04, 34 * U), M["beam"]))
mx, mz = svg_xz(206, 112)
on_wall(box("Mullion_H", (mx, WALL_Y - 0.12, mz), (18 * U, 0.04, 2 * U), M["beam"]))
x, z = svg_xz(214, 92)
on_wall(add(bpy.ops.mesh.primitive_cylinder_add, "Moon", (x, WALL_Y - 0.04, z), (1, 1, 1),
            (math.radians(90), 0, 0), M["moon"], vertices=32, radius=6 * U, depth=0.01))
for k, (sx, sy, r) in enumerate([(196, 104, 1.5), (200, 88, 1.2)]):
    x, z = svg_xz(sx, sy)
    sphere(f"Star_{k}", (x, WALL_Y - 0.04, z), (r * U, 0.005, r * U), M["moon"], 12)

# Shelf on the wall right under the window, with a mug of ale standing on it.
shelf_top = svg_xz(0, 150)[1]
x = svg_xz(208, 0)[0]
on_wall(box("Shelf", (x, WALL_Y - 0.225, shelf_top - 3.5 * U), (28 * U, 0.225, 3.5 * U), M["beam"], 0.01))
mug_x, mug_y = 1.72, WALL_Y - 0.38
on_wall(add(bpy.ops.mesh.primitive_cylinder_add, "Mug", (mug_x, mug_y, shelf_top + 0.19), (1, 1, 1),
            material=M["mug"], vertices=32, radius=0.15, depth=0.38))
on_wall(add(bpy.ops.mesh.primitive_cylinder_add, "MugFoam", (mug_x, mug_y, shelf_top + 0.4), (1, 1, 1),
            material=M["foam"], vertices=32, radius=0.155, depth=0.04))
on_wall(add(bpy.ops.mesh.primitive_torus_add, "MugHandle", (mug_x + 0.22, mug_y, shelf_top + 0.2), (1, 1, 1),
            (math.radians(90), 0, 0), M["mug"], True, major_radius=0.1, minor_radius=0.03))

# Tux stands behind the counter; only his upper body shows above it.
x, z = tux_xz(128, 150)
sphere("Tux_Body", (x, 0.9, z), (62 * 0.86 * U, 0.85, 70 * 0.86 * U), M["black"])
x, z = tux_xz(128, 160)
sphere("Tux_Belly", (x, 0.42, z), (41 * 0.86 * U, 0.55, 52 * 0.86 * U), M["white"])
x, z = tux_xz(128, 86)
head = sphere("Tux_Head", (x, 0.85, z), (46 * 0.86 * U, 0.8, 44 * 0.86 * U), M["black"])
for s in (-1, 1):
    ex, ez = tux_xz(128 + 16 * s, 80)
    sphere(f"Tux_Eye_{s}", (ex, 0.14, ez), (10 * 0.86 * U, 0.1, 13 * 0.86 * U), M["eye"], 48,
           (0, 0, math.radians(14 * s)))
    # Pupils look down at the cards; big glints keep the look bright.
    px, pz = tux_xz(128 + 13 * s, 85)
    sphere(f"Tux_Pupil_{s}", (px, 0.05, pz), (5.4 * 0.86 * U, 0.045, 6 * 0.86 * U), M["pupil"], 32,
           (0, 0, math.radians(14 * s)))
    gx, gz = tux_xz(128 + 13 * s + 1.8, 82.6)
    sphere(f"Tux_Glint_{s}", (gx, 0.0, gz), (1.9 * 0.86 * U, 0.015, 2.1 * 0.86 * U), M["eye"], 16)
# Closed beak whose corners turn up into a smile.
bx, bz = tux_xz(128, 103)
smile(sphere("Tux_BeakTop", (bx, -0.08, bz), (22 * 0.86 * U, 0.24, 6.5 * 0.86 * U), M["orange"]), 0.7)
bx, bz = tux_xz(128, 109)
smile(sphere("Tux_Mouth", (bx, -0.02, bz), (19 * 0.86 * U, 0.17, 4 * 0.86 * U), M["mouth"]), 1.2)
smile(sphere("Tux_BeakLow", (bx, -0.03, bz - 0.04), (17.5 * 0.86 * U, 0.17, 4.5 * 0.86 * U), M["orange_dark"]), 1.4)

# Hand of three cards held in front of Tux, 0.1 apart so that neither the
# cards nor their emblems touch; the lowest corners stay above the counter.
# Each emblem lies on its card where the card in front leaves it free.
CARD_DZ = 0.24
for i, (sx, sy, rot, sym) in enumerate([(98, 172, -24, "diamond"), (128, 164, 0, "ring"), (158, 172, 24, "star")]):
    x, z = tux_xz(sx, sy)
    a = math.radians(rot)
    right = Vector((math.cos(a), 0, -math.sin(a)))
    up = Vector((math.sin(a), 0, math.cos(a)))
    c = Vector((x, -0.45 - 0.1 * i, z + CARD_DZ))
    w, h = 21 * 1.15 * 0.86 * U, 30 * 1.15 * 0.86 * U
    box(f"Card_{i}_Rim", c, (w, 0.015, h), M["gold"], 0.06, (0, a, 0))
    box(f"Card_{i}_Face", c + Vector((0, -0.027, 0)), (w - 0.05, 0.012, h - 0.05), M["card"], 0.05, (0, a, 0))
    face_front = -0.039
    if sym == "diamond":
        p = c + right * -0.16 + up * 0.08 + Vector((0, face_front - 0.012, 0))
        box(f"Card_{i}_Sym", p, (0.14, 0.012, 0.14), M["blue"], 0.01, (0, a + math.radians(45), 0))
    elif sym == "ring":
        p = c + right * -0.1 + up * 0.13 + Vector((0, face_front - 0.026, 0))
        add(bpy.ops.mesh.primitive_torus_add, f"Card_{i}_Sym", p, (1, 1, 0.4), (math.radians(90), 0, 0),
            M["red"], True, major_radius=0.155, minor_radius=0.065)
    else:
        bm = bmesh.new()
        pts = [bm.verts.new(((0.27 if k % 2 == 0 else 0.11) * math.cos(math.pi / 2 + k * math.pi / 5), 0,
                             (0.27 if k % 2 == 0 else 0.11) * math.sin(math.pi / 2 + k * math.pi / 5)))
               for k in range(10)]
        f = bm.faces.new(pts)
        ext = bmesh.ops.extrude_face_region(bm, geom=[f])
        bmesh.ops.translate(bm, vec=(0, -0.025, 0),
                            verts=[v for v in ext["geom"] if isinstance(v, bmesh.types.BMVert)])
        me = bpy.data.meshes.new("Star")
        bm.to_mesh(me)
        bm.free()
        o = bpy.data.objects.new(f"Card_{i}_Sym", me)
        scn.collection.objects.link(o)
        o.location = c + up * 0.03 + Vector((0, face_front, 0))
        o.rotation_euler = (0, a, 0)
        o.data.materials.append(M["green"])

# Flippers wrap round the hand from the sides, as in the drawing: behind the
# cards at the shoulder, round the outside of the outer card's edge, and in
# front of the cards at the bottom, above the counter.
for s in (-1, 1):
    cu = bpy.data.curves.new(f"Flipper_{s}", "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = 1.0
    cu.bevel_resolution = 6
    cu.use_fill_caps = True
    sp = cu.splines.new("BEZIER")
    # Broad, flat paddles: a round tube squashed in depth by the object scale.
    flat = 0.4
    pts = [(-0.95, 0.35, 1.7, 0.2), (-1.45, -0.05, 1.15, 0.27), (-1.45, -0.75, 0.68, 0.26),
           (-1.0, -0.84, 0.52, 0.2), (-0.6, -0.84, 0.5, 0.1)]
    sp.bezier_points.add(len(pts) - 1)
    for p, (x, y, z, r) in zip(sp.bezier_points, pts):
        p.co = (x * s, y / flat, z)
        p.handle_left_type = p.handle_right_type = "AUTO"
        p.radius = r
    o = bpy.data.objects.new(f"Tux_Flipper_{s}", cu)
    o.scale.y = flat
    scn.collection.objects.link(o)
    o.data.materials.append(M["black"])

# Serving counter: a thin plank across the front whose middle bows towards
# the viewer; a played card lies on it and sticks out.
bm = bmesh.new()
half_w, bow, n = 2.5, 0.45, 48
xs = [-half_w + 2 * half_w * k / n for k in range(n + 1)]
front = [(x, -1.6 + bow * (x / half_w) ** 2) for x in xs]
back = [(x, 0.0 + bow * (x / half_w) ** 2) for x in reversed(xs)]  # in front of Tux
f = bm.faces.new([bm.verts.new((x, y, COUNTER_Z)) for x, y in front + back])
ext = bmesh.ops.extrude_face_region(bm, geom=[f])
bmesh.ops.translate(bm, vec=(0, 0, -0.26),
                    verts=[v for v in ext["geom"] if isinstance(v, bmesh.types.BMVert)])
bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
me = bpy.data.meshes.new("Counter")
bm.to_mesh(me)
bm.free()
counter = bpy.data.objects.new("Counter", me)
scn.collection.objects.link(counter)
counter.data.materials.append(M["wood"])
counter.data.materials.append(M["woodtop"])
for poly in counter.data.polygons:
    poly.material_index = 1 if poly.normal.z > 0.5 else 0
    poly.use_smooth = abs(poly.normal.z) < 0.5
bevel(counter, 0.04)
x, _ = svg_xz(188, 0)
card_back = Vector((x, -1.0, COUNTER_Z + 0.012))
rz = math.radians(-14)
box("TableCard_Rim", card_back, (0.5, 0.71, 0.012), M["gold_light"], 0.05, (0, 0, rz))
box("TableCard", card_back + Vector((0, 0, 0.024)), (0.43, 0.64, 0.012), M["cardback"], 0.04, (0, 0, rz))
box("TableCard_Sym", card_back + Vector((0, 0, 0.048)), (0.17, 0.17, 0.012), M["gold_light"], 0.0,
    (0, 0, rz + math.radians(45)))


def light(name, kind, loc, energy, color=(1, 1, 1), size=3.0, rot=(0, 0, 0), shadow=True):
    """Lamp and fire lights cast no shadows and no highlights (shadow=False)."""
    d = bpy.data.lights.new(name, kind)
    d.energy = energy
    d.color = color
    d.use_shadow = shadow
    if not shadow:
        d.specular_factor = 0.0
    if kind == "AREA":
        d.size = size
    else:
        d.shadow_soft_size = 0.2
    o = bpy.data.objects.new(name, d)
    scn.collection.objects.link(o)
    o.location = loc
    o.rotation_euler = rot
    return o


for sx in (-1, 1):
    light(f"LampLight_{sx}", "POINT", (1.3 * sx, LANTERN_Y - 0.2, 3.1), 90, (1, 0.75, 0.4), shadow=False)
light("FireLight", "POINT", (fx, 1.5, HEARTH_Z + 0.5), 140, (1, 0.45, 0.12), shadow=False)
light("WallGlow", "POINT", (0, 1.4, 2.4), 120, (1, 0.7, 0.4), shadow=False)

# Camera, lights, render settings
cam_data = bpy.data.cameras.get("HS_Cam") or bpy.data.cameras.new("HS_Cam")
# Far away with a long lens, so the wall keeps its size behind Tux, and
# 9 degrees from above, so the counter top and the card on it show.
cam_data.lens = 180
cam = bpy.data.objects.new("HS_Cam", cam_data)
scn.collection.objects.link(cam)
tilt, dist = math.radians(9), 30
cam.location = (0, -dist * math.cos(tilt), 1.92 + dist * math.sin(tilt))
cam.rotation_euler = (math.radians(90) - tilt, 0, 0)
scn.camera = cam
light("HS_Key", "AREA", (-4, -6, 6), 900, (1, 0.97, 0.92), 4, (math.radians(45), 0, math.radians(-35)))
light("HS_Fill", "AREA", (5, -5, 2), 300, (0.85, 0.9, 1.0), 4, (math.radians(70), 0, math.radians(45)))
light("HS_Top", "AREA", (0, -1, 6), 250, (1, 0.85, 0.6), 3, (math.radians(20), 0, 0))
try:
    scn.render.engine = "BLENDER_EEVEE"
except TypeError:
    scn.render.engine = "BLENDER_EEVEE_NEXT"
scn.render.film_transparent = True
scn.view_settings.view_transform = "Standard"
scn.view_settings.exposure = -0.3
# 1024 px, moderate samples, no raytracing: a larger raytraced render crashed Blender.
scn.render.resolution_x = scn.render.resolution_y = 1024
scn.eevee.taa_render_samples = 64
if hasattr(scn.eevee, "use_raytracing"):
    scn.eevee.use_raytracing = False
w = bpy.data.worlds.get("HS_World") or bpy.data.worlds.new("HS_World")
scn.world = w
w.use_nodes = True
bg = next(n for n in w.node_tree.nodes if n.type == "BACKGROUND")
bg.inputs["Color"].default_value = (0.05, 0.045, 0.04, 1)
bg.inputs["Strength"].default_value = 0.6
print("built", len(scn.collection.all_objects), "objects")
