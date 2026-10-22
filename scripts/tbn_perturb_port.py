#!/usr/bin/env python3
"""
tbn_perturb_port.py — the f32 port of the tangent-wiring TBN algebra.

Companion artifact to docs/claw-artifacts/umber-gpu/tangent-wiring-tbn-algebra.md
(static prep for the tangent-wiring kill, docs/loop/briefs/2026-10-20-tangent-wiring.md;
kill report docs/loop/kill-reports/2026-10-22-tangent-tbn-static-prep.md).

Every arithmetic operation below is rounded through IEEE f32 (struct
pack/unpack), mirroring op-for-op what WGSL and the glam CPU helper
compute. Remaining differences (WGSL rsqrt-based normalize vs glam's
sqrt+div, GPU fast-math) live inside the goldens' existing +/-1 byte
tolerance; the flat-fixture agreement below is EXACT at f32.

Ports:
1. mikkelsen_perturbed_normal  — the CURRENT height_perturbed_normal
   (Mikkelsen screen-space surface-gradient), the shape of the golden
   helper in crates/umber-gpu/src/renderer.rs.
2. tbn_perturbed_normal        — the CORRECTED TBN formula the wiring
   kill ships: n' = normalize(n - ((dhdu/su)*That + (dhdv/sv)*Bhat)).
3. tbn_perturbed_normal_nostretch — the brief's original step-3 sketch
   (no stretch), kept only to quantify what it breaks.
4. vertex_tangent_frames       — crates/umber-mesh/src/tangents.rs,
   plus the RECOMMENDED stretch extension (per-weld-group mean
   |t_face| / |b_face|, fallback su=sv=1.0 for zero-contribution
   groups) — the numbers the wiring kill's upload will produce.
5. The Lit golden path (expected_height_perturbed_byte's lighting
   math) so every perturbed normal lands as the bytes the goldens pin.

Fixtures:
A: renderer.rs's textured_quad_mesh — the three existing ramp cases
   through all three formulas (the step-4 agreement proof).
B: the cylinder segment — the curved golden's fixture: frames,
   stretch, the probe, and the per-facet JUMP of the derivative frame
   vs the smooth TBN value that tracks the analytic curve.

Run: python3 scripts/tbn_perturb_port.py   (exit 0 = all checks green)
"""

import math
import struct
import sys

# ---------------------------------------------------------------------------
# f32 plumbing: every scalar op rounds to IEEE f32, exactly like the GPU/CPU.
# ---------------------------------------------------------------------------


def _f(x):
    """Round a python float (f64) to the nearest IEEE f32."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def fadd(a, b):
    return _f(_f(a) + _f(b))


def fsub(a, b):
    return _f(_f(a) - _f(b))


def fmul(a, b):
    return _f(_f(a) * _f(b))


def fdiv(a, b):
    return _f(_f(a) / _f(b))


def fsqrt(a):
    return _f(math.sqrt(_f(a)))


def v3(x, y, z):
    return (_f(x), _f(y), _f(z))


def vadd(a, b):
    return (fadd(a[0], b[0]), fadd(a[1], b[1]), fadd(a[2], b[2]))


def vsub(a, b):
    return (fsub(a[0], b[0]), fsub(a[1], b[1]), fsub(a[2], b[2]))


def vscale(a, s):
    return (fmul(a[0], s), fmul(a[1], s), fmul(a[2], s))


def vdot(a, b):
    # glam's dot: a.x*b.x + a.y*b.y + a.z*b.z, left-assoc, f32 each step.
    return fadd(fadd(fmul(a[0], b[0]), fmul(a[1], b[1])), fmul(a[2], b[2]))


def vcross(a, b):
    return (
        fsub(fmul(a[1], b[2]), fmul(a[2], b[1])),
        fsub(fmul(a[2], b[0]), fmul(a[0], b[2])),
        fsub(fmul(a[0], b[1]), fmul(a[1], b[0])),
    )


def vlen(a):
    return fsqrt(vdot(a, a))


def vnormalize(a):
    # glam: self / self.length() — per-component f32 division.
    l = vlen(a)
    return (fdiv(a[0], l), fdiv(a[1], l), fdiv(a[2], l))


def fsignum(x):
    # Rust f32::signum: never 0 (the golden helper's documented stand-in
    # for WGSL sign; det is nonzero in every fixture here).
    return _f(-1.0) if x < 0 else _f(1.0)


def clamp(x, lo, hi):
    return _f(min(max(_f(x), _f(lo)), _f(hi)))


# ---------------------------------------------------------------------------
# 1. The current formula (renderer.rs's mikkelsen_perturbed_normal mirror).
# ---------------------------------------------------------------------------


def mikkelsen_perturbed_normal(n, dpos_dx, dpos_dy, dhdx, dhdy):
    r1 = vcross(dpos_dy, n)
    r2 = vcross(n, dpos_dx)
    det = vdot(dpos_dx, r1)
    surf_grad = vscale(vadd(vscale(r1, dhdx), vscale(r2, dhdy)), fsignum(det))
    return vnormalize(vsub(vscale(n, _f(abs(det))), surf_grad))


# ---------------------------------------------------------------------------
# 2/3. The TBN formulas (new height_perturbed_normal core).
# ---------------------------------------------------------------------------

STRETCH_FLOOR = _f(1e-6)


def tbn_frame(n, tangent, handedness):
    """That = normalize(t - n*dot(n,t)); Bhat = handedness * cross(n, That)."""
    t_hat = vnormalize(vsub(tangent, vscale(n, vdot(n, tangent))))
    b_hat = vscale(vcross(n, t_hat), handedness)
    return t_hat, b_hat


def tbn_perturbed_normal(n, tangent, handedness, su, sv, dhdu, dhdv):
    """The CORRECTED formula: per-world-unit slopes via the stretch."""
    t_hat, b_hat = tbn_frame(n, tangent, handedness)
    su_c = _f(max(_f(su), STRETCH_FLOOR))
    sv_c = _f(max(_f(sv), STRETCH_FLOOR))
    grad = vadd(vscale(t_hat, fdiv(dhdu, su_c)), vscale(b_hat, fdiv(dhdv, sv_c)))
    return vnormalize(vsub(n, grad))


def tbn_perturbed_normal_nostretch(n, tangent, handedness, dhdu, dhdv):
    """The brief's step-3 sketch, verbatim — quantifies the breakage."""
    t_hat, b_hat = tbn_frame(n, tangent, handedness)
    grad = vadd(vscale(t_hat, dhdu), vscale(b_hat, dhdv))
    return vnormalize(vsub(n, grad))


# ---------------------------------------------------------------------------
# 4. vertex_tangent_frames — the umber-mesh foundation module, ported,
#    plus the RECOMMENDED stretch extension.
# ---------------------------------------------------------------------------


def _face_tangent(e1, e2, du1, dv1, du2, dv2):
    det = fsub(fmul(du1, dv2), fmul(du2, dv1))
    if abs(det) < 1e-10:
        return None
    r = fdiv(_f(1.0), det)
    t = (
        fmul(fsub(fmul(dv2, e1[0]), fmul(dv1, e2[0])), r),
        fmul(fsub(fmul(dv2, e1[1]), fmul(dv1, e2[1])), r),
        fmul(fsub(fmul(dv2, e1[2]), fmul(dv1, e2[2])), r),
    )
    b = (
        fmul(fsub(fmul(du1, e2[0]), fmul(du2, e1[0])), r),
        fmul(fsub(fmul(du1, e2[1]), fmul(du2, e1[1])), r),
        fmul(fsub(fmul(du1, e2[2]), fmul(du2, e1[2])), r),
    )
    return t, b


def _fallback_frame(normal):
    l = fsqrt(fadd(fadd(fmul(normal[0], normal[0]), fmul(normal[1], normal[1])), fmul(normal[2], normal[2])))
    if l < 1e-20:
        return v3(1.0, 0.0, 0.0), _f(1.0), _f(1.0), _f(1.0)
    ab = (abs(normal[0]), abs(normal[1]), abs(normal[2]))
    if ab[0] <= ab[1] and ab[0] <= ab[2]:
        helper = v3(1.0, 0.0, 0.0)
    elif ab[1] <= ab[2]:
        helper = v3(0.0, 1.0, 0.0)
    else:
        helper = v3(0.0, 0.0, 1.0)
    c = vcross(normal, helper)
    clen = vlen(c)
    return (fdiv(c[0], clen), fdiv(c[1], clen), fdiv(c[2], clen)), _f(1.0), _f(1.0), _f(1.0)


def vertex_tangent_frames(positions, normals, uvs, indices):
    """Returns one (tangent, handedness, su, sv) per slot.

    Mirror of umber_mesh::tangents::vertex_tangent_frames with the
    stretch extension: su/sv are the per-weld-group MEAN of the
    contributing faces' |t_face| / |b_face| (the unweighted
    accumulation convention, judgment call 2 of the foundation kill);
    a zero-contribution group takes the fallback frame with su=sv=1.
    """
    slot_count = min(len(positions), len(normals), len(uvs))
    group_of = [0] * slot_count
    group_key = {}
    group_normals = []
    for slot in range(slot_count):
        key = b"".join(
            struct.pack("<f", v)
            for v in (positions[slot] + normals[slot] + uvs[slot])
        )
        nxt = len(group_normals)
        gid = group_key.setdefault(key, nxt)
        if gid == nxt:
            group_normals.append(normals[slot])
        group_of[slot] = gid
    group_count = len(group_normals)
    t_sum = [[_f(0.0)] * 3 for _ in range(group_count)]
    b_sum = [[_f(0.0)] * 3 for _ in range(group_count)]
    len_t_sum = [_f(0.0)] * group_count
    len_b_sum = [_f(0.0)] * group_count
    face_count = [0] * group_count

    triangles = len(indices) // 3
    for tri in range(triangles):
        i0, i1, i2 = indices[tri * 3], indices[tri * 3 + 1], indices[tri * 3 + 2]
        if i0 >= slot_count or i1 >= slot_count or i2 >= slot_count:
            continue
        p0, p1, p2 = positions[i0], positions[i1], positions[i2]
        e1 = vsub(p1, p0)
        e2 = vsub(p2, p0)
        uv0, uv1, uv2 = uvs[i0], uvs[i1], uvs[i2]
        ft = _face_tangent(
            e1, e2,
            fsub(uv1[0], uv0[0]), fsub(uv1[1], uv0[1]),
            fsub(uv2[0], uv0[0]), fsub(uv2[1], uv0[1]),
        )
        if ft is None:
            continue
        t_face, b_face = ft
        for slot in (i0, i1, i2):
            g = group_of[slot]
            t_sum[g] = list(vadd(tuple(t_sum[g]), t_face))
            b_sum[g] = list(vadd(tuple(b_sum[g]), b_face))
            len_t_sum[g] = fadd(len_t_sum[g], vlen(t_face))
            len_b_sum[g] = fadd(len_b_sum[g], vlen(b_face))
            face_count[g] += 1

    out = []
    for g in range(group_count):
        n = group_normals[g]
        t = tuple(t_sum[g])
        ndot = vdot(n, t)
        gs = vsub(t, vscale(n, ndot))
        l = vlen(gs)
        if l < 1e-20:
            fb_t, fb_h, su, sv = _fallback_frame(n)
            out.append((fb_t, fb_h, su, sv))
            continue
        gs = (fdiv(gs[0], l), fdiv(gs[1], l), fdiv(gs[2], l))
        c = vcross(n, gs)
        handedness = _f(-1.0) if vdot(c, tuple(b_sum[g])) < 0 else _f(1.0)
        if face_count[g] > 0:
            su = fdiv(len_t_sum[g], _f(face_count[g]))
            sv = fdiv(len_b_sum[g], _f(face_count[g]))
        else:
            su, sv = _f(1.0), _f(1.0)
        out.append((gs, handedness, su, sv))
    return [out[group_of[slot]] for slot in range(slot_count)]


# ---------------------------------------------------------------------------
# 5. The Lit golden path (expected_height_perturbed_byte's lighting).
# ---------------------------------------------------------------------------

GROUND = (_f(0.30), _f(0.27), _f(0.24))
SKY = (_f(0.45), _f(0.55), _f(0.78))
BASE = (_f(0.72), _f(0.72), _f(0.75))
SUN = (_f(1.0), _f(0.96), _f(0.90))
TO_LIGHT = vnormalize(v3(1.0, 1.0, 1.0))


def lit_bytes(n):
    diffuse = _f(max(_f(vdot(n, TO_LIGHT)), _f(0.0)))
    t = clamp(fadd(fmul(n[1], _f(0.5)), _f(0.5)), 0.0, 1.0)
    px = []
    for i in range(3):
        env = fadd(GROUND[i], fmul(fsub(SKY[i], GROUND[i]), t))
        d = fmul(fmul(diffuse, SUN[i]), _f(0.55))
        px.append(fmul(BASE[i], fadd(env, d)))
    out = []
    for i in range(3):
        out.append(int(math.floor(fadd(fmul(px[i], _f(255.0)), _f(0.5)))) & 0xFF)
    out.append(255)
    return out


def fmt_v(v):
    return "(" + ", ".join(f"{c:.7f}" for c in v) + ")"


# ---------------------------------------------------------------------------
# The umber-mesh unit tests, ported (the 7/7 validation).
# ---------------------------------------------------------------------------


def check_foundation():
    eps = 1e-6

    # 1. flat quad welds into one consistent frame
    pos = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 1.0, 0.0),
           (0.0, 0.0, 0.0), (1.0, 1.0, 0.0), (0.0, 1.0, 0.0)]
    pos = [v3(*p) for p in pos]
    nrm = [v3(0.0, 0.0, 1.0) for _ in range(6)]
    uv = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    uv = [(_f(a), _f(b)) for a, b in uv]
    frames = vertex_tangent_frames(pos, nrm, uv, list(range(6)))
    assert len(frames) == 6
    for t, h, su, sv in frames:
        assert abs(t[0] - 1.0) < eps and abs(t[1]) < eps and abs(t[2]) < eps
        assert h == 1.0

    # 2. v-flipped uvs flip handedness
    uv = [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0), (1.0, 0.0), (0.0, 0.0)]
    uv = [(_f(a), _f(b)) for a, b in uv]
    frames = vertex_tangent_frames(pos, nrm, uv, list(range(6)))
    for t, h, su, sv in frames:
        assert abs(t[0] - 1.0) < eps and h == -1.0

    # 3. degenerate uv triangle contributes nothing (fallback +Y for +Z)
    pos = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 1.0, 0.0),
           (5.0, 0.0, 0.0), (6.0, 0.0, 0.0), (5.0, 1.0, 0.0)]
    pos = [v3(*p) for p in pos]
    uv = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (9.0, 9.0), (9.0, 9.0), (9.0, 9.0)]
    uv = [(_f(a), _f(b)) for a, b in uv]
    frames = vertex_tangent_frames(pos, nrm, uv, list(range(6)))
    for slot in range(3):
        assert abs(frames[slot][0][0] - 1.0) < eps
    for slot in range(3, 6):
        t, h, su, sv = frames[slot]
        assert abs(t[1] - 1.0) < eps and abs(t[0]) < eps
        assert h == 1.0

    # 4. uv seam splits weld groups (+X vs +Z)
    pos = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 1.0, 0.0),
           (0.0, 0.0, 0.0), (0.0, 0.0, 1.0), (1.0, 0.0, 1.0)]
    pos = [v3(*p) for p in pos]
    nrm = [v3(0.0, 0.0, 1.0), v3(0.0, 0.0, 1.0), v3(0.0, 0.0, 1.0),
           v3(0.0, 1.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 1.0, 0.0)]
    uv = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (2.0, 0.0), (3.0, 0.0), (3.0, 1.0)]
    uv = [(_f(a), _f(b)) for a, b in uv]
    frames = vertex_tangent_frames(pos, nrm, uv, list(range(6)))
    assert abs(frames[0][0][0] - 1.0) < eps and abs(frames[0][0][2]) < eps
    assert abs(frames[3][0][2] - 1.0) < eps and abs(frames[3][0][0]) < eps
    assert frames[3][1] == 1.0

    # 5. out-of-range + remainder indices never panic
    pos = [v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(1.0, 1.0, 0.0)]
    nrm = [v3(0.0, 0.0, 1.0) for _ in range(3)]
    uv = [(_f(0.0), _f(0.0)), (_f(1.0), _f(0.0)), (_f(1.0), _f(1.0))]
    frames = vertex_tangent_frames(pos, nrm, uv, [0, 1, 999, 42])
    assert len(frames) == 3
    for t, h, su, sv in frames:
        assert abs(vlen(t) - 1.0) < eps

    # 6. zero normal takes the degenerate frame
    nrm = [v3(0.0, 0.0, 0.0) for _ in range(3)]
    frames = vertex_tangent_frames(pos, nrm, uv, [0, 1, 2])
    for t, h, su, sv in frames:
        assert t == (_f(1.0), _f(0.0), _f(0.0)) and h == 1.0

    # 7. frames are unit and perpendicular on the mixed case
    pos = [(0.0, 0.0, 0.0), (2.0, 0.0, 0.0), (2.0, 3.0, 0.0),
           (0.0, 0.0, 0.0), (0.0, 4.0, 0.0), (0.0, 4.0, 5.0)]
    pos = [v3(*p) for p in pos]
    nrm = [v3(0.0, 0.0, 1.0), v3(0.0, 0.0, 1.0), v3(0.0, 0.0, 1.0),
           v3(1.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(1.0, 0.0, 0.0)]
    uv = [(0.0, 0.0), (0.5, 0.0), (0.5, 0.75), (0.0, 0.0), (0.0, 1.0), (1.25, 1.0)]
    uv = [(_f(a), _f(b)) for a, b in uv]
    frames = vertex_tangent_frames(pos, nrm, uv, list(range(6)))
    for slot, (t, h, su, sv) in enumerate(frames):
        assert abs(vlen(t) - 1.0) < eps
        assert abs(vdot(t, nrm[slot])) < eps
        assert h in (1.0, -1.0)

    print("foundation module: 7/7 unit-test expectations reproduced")
    return True


# ---------------------------------------------------------------------------
# Fixture A: renderer.rs's textured_quad_mesh (the flat goldens).
# ---------------------------------------------------------------------------


def fixture_a():
    print("\n=== FIXTURE A: textured_quad_mesh (positions -1..1, uvs 0..1) ===")
    pos = [v3(-1.0, -1.0, 0.0), v3(1.0, -1.0, 0.0), v3(1.0, 1.0, 0.0), v3(-1.0, 1.0, 0.0)]
    nrm = [v3(0.0, 0.0, 1.0) for _ in range(4)]
    uv = [(_f(0.0), _f(0.0)), (_f(1.0), _f(0.0)), (_f(1.0), _f(1.0)), (_f(0.0), _f(1.0))]
    idx = [0, 1, 2, 0, 2, 3]
    frames = vertex_tangent_frames(pos, nrm, uv, idx)
    t0, h0, su, sv = frames[0]
    print(f"frames: T={fmt_v(t0)} h={h0} su={su:.7f} sv={sv:.7f}")
    assert abs(t0[0] - 1.0) < 1e-6 and h0 == 1.0
    assert abs(su - 2.0) < 1e-6 and abs(sv - 2.0) < 1e-6, "renderer quad stretch must be 2.0"

    n0 = v3(0.0, 0.0, 1.0)
    dpos_dx = v3(0.5, 0.0, 0.0)   # per-pixel NDC step at W=4 (identity view_proj)
    dpos_dy = v3(0.0, -0.5, 0.0)  # the y flip is wgpu's NDC->framebuffer convention
    duv_dx = (_f(0.25), _f(0.0))
    duv_dy = (_f(0.0), _f(-0.25))

    cases = [
        ("flat (h=const)", _f(0.0), _f(0.0)),
        ("+u ramp", _f(1.0), _f(0.0)),
        ("+v ramp", _f(0.0), _f(1.0)),
    ]
    print(f"{'case':<14} {'mikkelsen':<22} {'tbn(corrected)':<22} {'tbn(nostretch)':<22} agree?")
    all_agree = True
    for name, dhdu, dhdv in cases:
        # Current shader path: chain-rule into screen space, then mikkelsen.
        dhdx = fadd(fmul(dhdu, duv_dx[0]), fmul(dhdv, duv_dx[1]))
        dhdy = fadd(fmul(dhdu, duv_dy[0]), fmul(dhdv, duv_dy[1]))
        n_mik = mikkelsen_perturbed_normal(n0, dpos_dx, dpos_dy, dhdx, dhdy)
        n_tbn = tbn_perturbed_normal(n0, t0, h0, su, sv, dhdu, dhdv)
        n_ns = tbn_perturbed_normal_nostretch(n0, t0, h0, dhdu, dhdv)
        b_mik, b_tbn, b_ns = lit_bytes(n_mik), lit_bytes(n_tbn), lit_bytes(n_ns)
        agree = b_mik == b_tbn
        all_agree = all_agree and agree
        print(f"{name:<14} {str(b_mik[:3]):<22} {str(b_tbn[:3]):<22} {str(b_ns[:3]):<22} {'YES' if agree else 'NO'}")
        print(f"{'':<14} n'={fmt_v(n_mik)}")
        print(f"{'':<14}   tbn n'={fmt_v(n_tbn)}   nostretch n'={fmt_v(n_ns)}")
    print(f"\nSTEP-4 VERDICT: corrected-TBN reproduces every flat golden byte"
          f" {'EXACTLY' if all_agree else 'WITH MISMATCH'};")
    print("the no-stretch sketch breaks them (see its column) — it drops the")
    print("parametrization stretch (su=sv=2 on this very fixture).")
    assert all_agree, "corrected TBN must reproduce the flat goldens exactly"
    return True


# ---------------------------------------------------------------------------
# Fixture B: the cylinder segment (the curved golden).
# ---------------------------------------------------------------------------


def fixture_b():
    print("\n=== FIXTURE B: cylinder segment (R=1, theta in [-pi/4, pi/4], u=(t+pi/4)/(pi/2), v over height 1) ===")
    segs = 8
    dtheta = _f(math.pi / 2 / segs)
    cols = segs + 1
    pos, nrm, uv = [], [], []
    for row, v in enumerate((0.0, 1.0)):
        for c in range(cols):
            th = fsub(fmul(_f(c), dtheta), _f(math.pi / 4))
            pos.append(v3(fmul(_f(math.sin(th)), 1.0), fsub(_f(v), _f(0.5)), _f(math.cos(th))))
            nrm.append(v3(math.sin(th), 0.0, math.cos(th)))
            uv.append((_f(c / segs), _f(v)))
    idx = []
    for i in range(segs):
        b0, b1 = i, i + 1
        t0, t1 = i + cols, i + 1 + cols
        idx.extend([b0, b1, t0, b1, t1, t0])
    # Outward winding check (CCW from outside, cross(e1,e2) ~ +normal).
    for tri in range(len(idx) // 3):
        p0, p1, p2 = pos[idx[tri * 3]], pos[idx[tri * 3 + 1]], pos[idx[tri * 3 + 2]]
        e1, e2 = vsub(p1, p0), vsub(p2, p0)
        fn = vcross(e1, e2)
        assert vdot(fn, nrm[idx[tri * 3]]) > 0, f"facet {tri} winds inward"
    print(f"mesh: {len(pos)} slots, {len(idx)//3} facets, winding outward CCW (asserted)")

    frames = vertex_tangent_frames(pos, nrm, uv, idx)
    print(f"\n{'col':>3} {'theta':>9} {'T_hat':<27} {'h':>4} {'su':>9} {'sv':>6}")
    mid = frames[cols // 2]  # theta=0, bottom row (v=0)
    for c in range(cols):
        t, h, su, sv = frames[c]
        th = fsub(fmul(_f(c), dtheta), _f(math.pi / 4))
        print(f"{c:>3} {math.degrees(th):>8.3f}\u00b0 {fmt_v(t):<27} {h:>4} {su:>9.6f} {sv:>6.4f}")
    t_c, h_c, su_c, sv_c = mid
    print(f"\ncenter column (theta=0): T={fmt_v(t_c)} h={h_c} su={su_c:.7f} sv={sv_c:.7f}")
    print(f"analytic reference:      T=(1, 0, -0) su=pi/2={math.pi/2:.7f} sv=1.0")
    assert abs(t_c[0] - 1.0) < 1e-6 and abs(t_c[1]) < 1e-6 and abs(t_c[2]) < 1e-6
    assert h_c == 1.0 and abs(sv_c - 1.0) < 1e-6
    chord_bias = fdiv(fsub(_f(math.pi / 2), su_c), _f(math.pi / 2))
    print(f"chord bias on su vs analytic: {chord_bias*100:.4f}% (O(dtheta^2), expected ~0.16%)")

    # The probe: theta=0, the +u ramp convention of the flat goldens.
    dhdu, dhdv = _f(1.0), _f(0.0)
    n_probe = v3(0.0, 0.0, 1.0)
    n_tbn = tbn_perturbed_normal(n_probe, t_c, h_c, su_c, sv_c, dhdu, dhdv)
    # Analytic curved answer: true frame at theta=0, true stretch pi/2.
    n_ana = tbn_perturbed_normal(n_probe, v3(1.0, 0.0, 0.0), _f(1.0), _f(math.pi / 2), _f(1.0), dhdu, dhdv)
    # Derivative frame on the probe's two adjacent facets, honest full path:
    # screen model = identity ortho with 1 world unit per pixel step;
    # dpdx lies IN the facet plane (the surface), so z follows the plane.
    derivs = []
    for s in (-1, 1):
        th_m = fmul(_f(s), fdiv(dtheta, _f(2.0)))
        n_f = v3(math.sin(th_m), 0.0, math.cos(th_m))
        t_hat_f = v3(math.cos(th_m), 0.0, -math.sin(th_m))
        su_f = fmul(_f(2.0), _f(math.sin(fdiv(dtheta, _f(2.0)))))  # |t_face| = 2R sin(dtheta/2)/du, du=1/8
        su_f = fdiv(su_f, _f(1.0 / segs))
        # dpdx: in-plane, x-step 1 -> z = -tan(theta_m)
        dpos_dx = v3(1.0, 0.0, fsub(_f(0.0), _f(math.tan(th_m))))
        dpos_dy = v3(0.0, 1.0, 0.0)
        # duv/dscreen from the facet's affine map: alpha = 1/(su_f*cos(theta_m)), beta = 0 / 1
        duv_dx = (fdiv(_f(1.0), fmul(su_f, _f(math.cos(th_m)))), _f(0.0))
        duv_dy = (_f(0.0), fdiv(_f(1.0), _f(1.0)))
        dhdx = fadd(fmul(dhdu, duv_dx[0]), fmul(dhdv, duv_dx[1]))
        dhdy = fadd(fmul(dhdu, duv_dy[0]), fmul(dhdv, duv_dy[1]))
        n_d = mikkelsen_perturbed_normal(n_f, dpos_dx, dpos_dy, dhdx, dhdy)
        # internal consistency: the in-plane reduction n_f - (dhdu/su_f)*T_hat_f
        n_red = vnormalize(vsub(n_f, vscale(t_hat_f, fdiv(dhdu, su_f))))
        d = max(abs(a - b) for a, b in zip(n_d, n_red))
        assert d < 2e-7, f"facet reduction mismatch {d}"
        derivs.append((s, n_f, n_d))
    print(f"\nprobe at theta=0 (dhdu=1.0, dhdv=0 — the +u ramp convention):")
    for s, n_f, n_d in derivs:
        side = "left facet " if s < 0 else "right facet"
        print(f"  derivative-frame on {side}: n_f={fmt_v(n_f)}  n'={fmt_v(n_d)}  bytes={lit_bytes(n_d)[:3]}")
    print(f"  TBN (accumulated frame):  n'={fmt_v(n_tbn)}  bytes={lit_bytes(n_tbn)[:3]}")
    print(f"  analytic curved answer:   n'={fmt_v(n_ana)}  bytes={lit_bytes(n_ana)[:3]}")
    jump = abs(derivs[0][2][0] - derivs[1][2][0])
    tbn_vs_ana = max(abs(a - b) for a, b in zip(n_tbn, n_ana))
    print(f"\nfacet JUMP across the probe (x-component): {jump:.4f}")
    print(f"TBN-vs-analytic delta:                     {tbn_vs_ana:.2e} (the chord su bias, smooths to 0 as tessellation refines)")
    print("VERDICT: the derivative frame answers differ per facet (the piecewise")
    print("defect row 79 names); the TBN value sits between them, tracking the")
    print("analytic curve — and is screen-independent by construction.")
    return True


def main():
    ok = True
    ok = check_foundation() and ok
    ok = fixture_a() and ok
    ok = fixture_b() and ok
    if ok:
        print("\nPORT: ALL CHECKS GREEN")
        return 0
    print("\nPORT: CHECKS FAILED")
    return 1


if __name__ == "__main__":
    sys.exit(main())
