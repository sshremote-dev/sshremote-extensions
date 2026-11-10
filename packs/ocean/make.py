"""The Ocean style pack: a deep sea picture and its rising bubbles and drifting light."""
import json
import math
import random
import sys
from pathlib import Path

W, H, FR, LOOP = 1920, 1080, 24, 24 * 30
rnd = random.Random(7)


def v(k):
    return {"a": 0, "k": k}


def keys(points):
    return {"a": 1, "k": [{"t": t, "s": s, "o": {"x": [0.4], "y": [0]}, "i": {"x": [0.6], "y": [1]}} for t, s in points]}


def tr(p=(0, 0), o=100):
    return {"ty": "tr", "p": v(list(p)), "a": v([0, 0]), "s": v([100, 100]), "r": v(0), "o": v(o), "sk": v(0), "sa": v(0)}


def layer(name, shapes, ks=None):
    base = {"o": v(100), "r": v(0), "p": v([0, 0, 0]), "a": v([0, 0, 0]), "s": v([100, 100, 100])}
    base.update(ks or {})
    return {"ddd": 0, "ty": 4, "nm": name, "sr": 1, "ks": base, "ao": 0, "shapes": shapes, "ip": 0, "op": LOOP, "st": 0, "bm": 0}


def rect(x, y, w, h):
    return {"ty": "rc", "d": 1, "s": v([w, h]), "p": v([x + w / 2, y + h / 2]), "r": v(0)}


def ellipse(r):
    return {"ty": "el", "d": 1, "s": v([r * 2, r * 2]), "p": v([0, 0])}


def fill(rgb, o=100):
    return {"ty": "fl", "c": v([*rgb, 1]), "o": v(o), "r": 1}


def linear(a, b, stops, o=100):
    flat = [x for p, c in stops for x in (p, *c)]
    return {"ty": "gf", "o": v(o), "r": 1, "t": 1, "s": v(list(a)), "e": v(list(b)), "g": {"p": len(stops), "k": v(flat)}}


def radial(r, stops):
    flat = [x for p, c in stops for x in (p, *c)]
    alpha = [x for p, c in stops for x in (p, 1 if p == 0 else 0)]
    return {"ty": "gf", "o": v(100), "r": 1, "t": 2, "s": v([0, 0]), "e": v([r, 0]), "h": v(0), "a": v(0),
            "g": {"p": len(stops), "k": v(flat + alpha)}}


def doc(name, layers, op):
    return {"v": "5.7.4", "fr": FR, "ip": 0, "op": op, "w": W, "h": H, "nm": name, "ddd": 0, "assets": [], "layers": layers}


def still():
    sea = layer("sea", [{"ty": "gr", "it": [rect(0, 0, W, H), linear((0, 0), (0, H), [
        (0, (0.05, 0.32, 0.45)), (0.45, (0.02, 0.16, 0.30)), (1, (0.0, 0.04, 0.10))]), tr()]}])
    # Sand ridges at the bottom, darker further back.
    ridges = []
    for k, (y, c) in enumerate([(930, (0.06, 0.10, 0.14)), (990, (0.09, 0.13, 0.16))]):
        pts = [(x, y + 26 * math.sin(x / (210 + 60 * k) + k)) for x in range(0, W + 1, 120)]
        verts = pts + [(W, H), (0, H)]
        ridges.append({"ty": "gr", "it": [{"ty": "sh", "ks": v({"c": True, "v": [list(p) for p in verts],
                       "i": [[0, 0]] * len(verts), "o": [[0, 0]] * len(verts)})}, fill(c), tr()]})
    return doc("ocean_still", [layer("sand", ridges), sea], 1)


def motion():
    layers = []
    # Light shafts from the surface, swaying slowly.
    for k in range(5):
        x = 200 + k * 380 + rnd.uniform(-60, 60)
        shaft = [{"ty": "sh", "ks": v({"c": True, "v": [[-40, 0], [40, 0], [180, 900], [-180, 900]],
                  "i": [[0, 0]] * 4, "o": [[0, 0]] * 4})},
                 fill((0.6, 0.9, 1.0), 100), tr()]
        sway = keys([(0, [x, -20, 0]), (LOOP / 2, [x + 70, -20, 0]), (LOOP, [x, -20, 0])])
        fade = keys([(t, [8 + 10 * (0.5 + 0.5 * math.sin(t / LOOP * 2 * math.pi * 2 + k))]) for t in range(0, LOOP + 1, LOOP // 6)])
        layers.append(layer(f"light {k}", [{"ty": "gr", "it": shaft}], {"p": sway, "o": fade}))
    # Bubbles: rising, wobbling, each on its own cycle that divides the loop.
    for k in range(34):
        r = rnd.choice([5, 7, 9, 12, 15])
        x = rnd.uniform(60, W - 60)
        period = LOOP // rnd.choice([2, 3, 4])
        start = rnd.uniform(0, period)
        pts = []
        for c in range(LOOP // period + 2):
            t0 = start + (c - 1) * period
            pts += [(t0, [x, H + 30, 0]), (t0 + period * 0.999, [x + rnd.uniform(-40, 40), -30, 0])]
        pts = sorted((round(t, 2), s) for t, s in pts if -period <= t <= LOOP + period)
        bubble = [ellipse(r), {"ty": "st", "c": v([0.8, 0.95, 1, 1]), "o": v(70), "w": v(1.4), "lc": 2, "lj": 2},
                  fill((0.7, 0.9, 1.0), 12), tr()]
        p = {"a": 1, "k": [{"t": t, "s": s, "o": {"x": [0.5], "y": [0.5]}, "i": {"x": [0.5], "y": [0.5]}} for t, s in pts]}
        layers.append(layer(f"bubble {k}", [{"ty": "gr", "it": bubble}], {"p": p}))
    # Plankton glints: soft dots that come and go.
    for k in range(40):
        glow = [ellipse(10), radial(10, [(0, (0.6, 1.0, 0.95)), (1, (0.6, 1.0, 0.95))]), tr()]
        t = rnd.uniform(0, LOOP - 90)
        o = keys([(0, [0]), (t, [0]), (t + 40, [80]), (t + 90, [0]), (LOOP, [0])])
        layers.append(layer(f"glint {k}", [{"ty": "gr", "it": glow}],
                            {"p": v([rnd.uniform(0, W), rnd.uniform(150, 950), 0]), "o": o}))
    return doc("ocean_motion", layers, LOOP)


if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).parent)
    (out / "ocean_still.json").write_text(json.dumps(still(), separators=(",", ":")))
    (out / "ocean_motion.json").write_text(json.dumps(motion(), separators=(",", ":")))
