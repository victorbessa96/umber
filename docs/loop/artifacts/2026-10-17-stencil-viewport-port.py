# Ladder rung 3 for the stencil-viewport kill: a Python port of
# viewport.rs's screen_quad / stencil_drag / wrap_angle /
# stencil_overlay_rgba (f64 here, f32 in Rust; every pinned value below is
# exact or tolerance-checked the same way the Rust tests are), the Rust
# tests' pinned cases, and one mutation per branch. Run: python3 <this>.
import math

M = None
PI, TAU = math.pi, 2 * math.pi
STEP, MIN_SCALE, ARM, MAXS = math.pi / 2, 0.05, 1.0, 512


def T(center, scale, rotation):
    return dict(center=list(center), scale=scale, rotation=rotation)


def with_(t, **kw):
    out = dict(t)
    out.update(kw)
    return out


def forward(t, dims, p):
    (cx, cy), s, r = t["center"], t["scale"], t["rotation"]
    if M == "Q1":
        r = -r  # rotation sign flipped
    hx, hy = dims[0] / 2, dims[1] / 2
    dx, dy = (p[0] - hx) * s, (p[1] - hy) * s
    c, sn = math.cos(r), math.sin(r)
    return [cx + c * dx - sn * dy, cy + sn * dx + c * dy]


def inverse(t, dims, q):
    (cx, cy), s, r = t["center"], t["scale"], t["rotation"]
    c, sn = math.cos(r), math.sin(r)
    dx, dy = q[0] - cx, q[1] - cy
    return [(c * dx + sn * dy) / s + dims[0] / 2, (c * dy - sn * dx) / s + dims[1] / 2]


def screen_quad(t, w, h):
    cs = [[0, 0], [w, 0], [w, h], [0, h]]
    if M == "Q2":
        cs = [[0, 0], [0, h], [w, h], [w, 0]]  # corner order reversed
    if M == "Q3":
        cs = [[0, 0], [w, 0], [w, w], [0, w]]  # height taken from width
    return [forward(t, [w, h], c) for c in cs]


def wrap(a):
    if M == "W1":
        return a  # no wrap at all
    if M != "W2" and -PI < a <= PI:  # W2: always through rem_euclid
        return a
    a = a % TAU
    return a - TAU if a > PI else a


def drag(start, chord, snap, p0, p1, img, view):
    d = [p1[0] - p0[0], p1[1] - p0[1]]
    if chord == "rot":
        def arm(p):
            return [p[0] - start["center"][0], p[1] - start["center"][1]]

        f, to = arm(p0), arm(p1)

        def short(v):
            return math.hypot(*v) < ARM

        r = start["rotation"]
        if M == "R3" or (not short(f) and not short(to)):  # R3: no arm guard
            delta = math.atan2(to[1], to[0]) - math.atan2(f[1], f[0])
            if M == "R2":
                delta = -delta  # arc sign flipped
            if M == "R1" and snap:
                delta = round(delta / STEP) * STEP  # snaps the DELTA
            r += delta
        if snap and M != "R1":
            r = round(r / STEP) * STEP
        return with_(start, rotation=wrap(r))
    if chord == "scale":
        half = max(img[0], 1) * 0.5
        dd = d[1] if M == "S1" else d[0]  # S1: vertical delta
        s = start["scale"] + dd / (img[0] if M == "S3" else half)  # S3: full width
        if M != "S2":
            s = max(s, MIN_SCALE)  # S2: no clamp
        return with_(start, scale=s)
    if chord == "move":
        def keep(v, mx):
            if M == "V1":
                return v  # V1: no view clamp
            return min(max(v, 0.0), mx) if (math.isfinite(mx) and mx > 0) else v

        c = start["center"]
        if M == "V2":
            d = [d[0], -d[1]]  # V2: y sign flipped
        return with_(start, center=[keep(c[0] + d[0], view[0]), keep(c[1] + d[1], view[1])])
    raise ValueError(chord)


def overlay(w, h, cov):
    m = max(w, h)
    step = max(m // MAXS if M == "O1" else -(-m // MAXS), 1)  # O1: floor step
    ow, oh = -(-w // step), -(-h // step)
    out = []
    for oy in range(oh):
        for ox in range(ow):
            at = oy * step * w + ox * step
            if M == "O2":
                at = oy * step * h + ox * step  # O2: row stride by height
            out.append(255 if (at < len(cov) and cov[at]) else 0)
    return [ow, oh], out


def near(a, b, tol):
    return abs(a[0] - b[0]) <= tol and abs(a[1] - b[1]) <= tol


def quad_near(g, w, tol):
    return all(near(x, y, tol) for x, y in zip(g, w))


DS = T([100.0, 100.0], 1.0, 0.3)


def at(a):
    return [100 + 50 * math.cos(a), 100 + 50 * math.sin(a)]


def rot(snap, f, t):
    return drag(DS, "rot", snap, f, t, [64, 64], [800, 600])


def tests():
    f = []
    ident = T([0, 0], 1, 0)
    if screen_quad(ident, 4, 2) != [[-2, -1], [2, -1], [2, 1], [-2, 1]]:
        f.append("identity")
    t = T([10, 20], 1, PI / 2)
    if not quad_near(screen_quad(t, 4, 2), [[11, 18], [11, 22], [9, 22], [9, 18]], 1e-5):
        f.append("quarter")
    if screen_quad(with_(ident, scale=2), 4, 2) != [[-4, -2], [4, -2], [4, 2], [-4, 2]]:
        f.append("scale2")
    t = T([320, 240], 1.5, 0.7)
    q = screen_quad(t, 200, 100)
    corners = [[0, 0], [200, 0], [200, 100], [0, 100]]
    ok = all(near(inverse(t, [200, 100], s), im, 1e-3) for s, im in zip(q, corners))
    mean = [sum(p[0] for p in q) / 4, sum(p[1] for p in q) / 4]
    if not (ok and near(mean, t["center"], 1e-3)):
        f.append("invert")
    r = rot(False, [150, 100], [100, 150])
    ok = abs(r["rotation"] - (0.3 + PI / 2)) < 1e-6 and r["center"] == DS["center"] and r["scale"] == 1
    ok = ok and abs(rot(False, [100, 150], [150, 100])["rotation"] - (0.3 - PI / 2)) < 1e-6
    ok = ok and abs(rot(False, at(0.4), at(1.1))["rotation"] - 1.0) < 1e-5
    if not ok:
        f.append("rotate")
    ok = rot(True, at(0), at(1.2))["rotation"] == PI / 2
    ok = ok and rot(True, at(0), at(-0.2))["rotation"] == 0
    ok = ok and rot(True, at(0), at(-1.9))["rotation"] == -PI / 2
    ok = ok and abs(rot(False, at(0), at(1.2))["rotation"] - 1.5) < 1e-5
    if not ok:
        f.append("snap")
    c = DS["center"]
    if not (rot(False, [150, 120], c)["rotation"] == 0.3 and rot(False, c, [150, 120])["rotation"] == 0.3):
        f.append("hold")
    w = drag(with_(DS, rotation=3.0), "rot", False, at(0), at(0.5), [64, 64], [800, 600])
    if not abs(w["rotation"] - (3.5 - TAU)) < 1e-5:
        f.append("wrap")

    def sc(dx, dy, start=None):
        s0 = start or with_(DS, rotation=0.0)
        return drag(s0, "scale", False, [200, 100], [200 + dx, 100 + dy], [200, 50], [800, 600])["scale"]

    sw = [sc(k * 10, 0) for k in range(-8, 9)]
    ok = sc(50, 0) == 1.5 and sc(100, 0) == 2.0 and sc(-50, 0) == 0.5 and sc(0, 80) == 1.0
    ok = ok and all(b > a for a, b in zip(sw, sw[1:]))
    ok = ok and screen_quad(with_(DS, scale=sc(50, 0), rotation=0.0), 200, 50)[1][0] == 250
    if not ok:
        f.append("scale")
    if not (sc(-150, 0, DS) == MIN_SCALE and sc(-1000, 0, DS) == MIN_SCALE):
        f.append("clamp")

    def mv(to):
        return drag(DS, "move", False, [10, 10], to, [64, 64], [200, 150])["center"]

    if not (mv([40, -10]) == [130, 80] and mv([1000, 1000]) == [200, 150] and mv([-1000, -1000]) == [0, 0]):
        f.append("move")
    size, out = overlay(4, 4, [1 if i % 4 == i // 4 else 0 for i in range(16)])
    ok = size == [4, 4] and out == [255 if i % 4 == i // 4 else 0 for i in range(16)]
    size, out = overlay(1030, 1, [1 if x % 3 == 0 else 0 for x in range(1030)])
    ok = ok and size == [344, 1] and all(v == 255 for v in out)
    size, out = overlay(3, 600, [1 if (i // 3) % 2 == 0 else 0 for i in range(1800)])
    ok = ok and size == [2, 300] and out == [255] * 600
    if not ok:
        f.append("overlay")
    return f


print("baseline:", tests() or "PASS")
for m in ["Q1", "Q2", "Q3", "W1", "W2", "R1", "R2", "R3", "S1", "S2", "S3", "V1", "V2", "O1", "O2"]:
    M = m
    r = tests()
    M = None
    print(m, ("killed by " + ",".join(r)) if r else "SURVIVED")
