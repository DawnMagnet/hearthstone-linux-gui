# Reports pairs of objects in the HS_Logo scene whose surfaces cross each
# other. Parts of one thing (Tux, one card, one lantern, the window, the
# mug, the fire) are grouped and not checked against each other; touching
# faces can show up as a few triangle pairs and are listed with their count.
import bpy, re
from mathutils.bvhtree import BVHTree

scn = bpy.data.scenes["HS_Logo"]
GROUPS = [
    (r"^Tux_|^Rig_", "tux"),
    (r"^Card_(\d)_", r"card\1"),
    (r"^TableCard", "table card"),
    (r"^Lantern[A-Za-z]*_(-?1)", r"lantern\1"),
    (r"^Mug", "mug"),
    (r"^Flame", "fire"),
    (r"^Fireplace$|^FireBack$|^Hearth$", "fireplace"),
    (r"^Window|^Mullion|^Moon$|^Star_", "window"),
]


def group(name):
    for pattern, g in GROUPS:
        m = re.match(pattern, name)
        if m:
            return m.expand(g)
    return name


def trees(frame):
    scn.frame_set(frame)
    dg = bpy.context.evaluated_depsgraph_get()
    out = {}
    for o in scn.objects:
        if o.type not in ("MESH", "CURVE") or o.hide_render:
            continue
        ev = o.evaluated_get(dg)
        me = ev.to_mesh()
        if me is None or not me.polygons:
            ev.to_mesh_clear()
            continue
        verts = [ev.matrix_world @ v.co for v in me.vertices]
        polys = [tuple(p.vertices) for p in me.polygons]
        out[o.name] = BVHTree.FromPolygons(verts, polys)
        ev.to_mesh_clear()
    return out


def check(frames=(1,)):
    found = {}
    for f in frames:
        t = trees(f)
        names = sorted(t)
        for i, a in enumerate(names):
            for b in names[i + 1:]:
                if group(a) == group(b):
                    continue
                n = len(t[a].overlap(t[b]))
                if n and n > found.get((a, b), (0, 0))[0]:
                    found[(a, b)] = (n, f)
    for (a, b), (n, f) in sorted(found.items(), key=lambda kv: -kv[1][0]):
        print(f"{a} x {b}: {n} triangle pairs (frame {f})")
    print("pairs:", len(found))


check(FRAMES if "FRAMES" in globals() else (1,))
