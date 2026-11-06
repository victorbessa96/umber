#!/usr/bin/env python3
"""perf_rebaseline.py — worklist #2: re-baseline perf/baseline.json on the
4096-default paint path (docs/specs/requirements.md §5 'Performance floor').

The pre-core-loop baselines ran at 512², so they measured the wrong path.
This script launches ./target/release/umber --control <port> with a fully
scratch XDG dir set (pattern from scripts/desktop_checks.py), opens a scratch
.umber project whose texture set is pinned at resolution 4096 (the
DEFAULT_TARGET_SIZE path, paint_state.rs), loads a deterministic
1,002,252-triangle cube-grid GLB fixture (generated in-script, written under
scratch only), then drives N timed input→photon samples over the control
channel per the input_photon protocol (crates/umber-app/src/input_photon.rs):

  input  = one `command.run` request — the control channel's input event,
           drained at the top of fn ui once per frame (drain_control);
  photon = the next presented frame, observed via `viewport.screenshot`
           (egui's Screenshot event fires after the frame is presented);
  sample = wall time from the command.run send to the screenshot reply.

Consecutive screenshot replies also yield frame-to-frame (viewport) times.
The exact JSONL method names are umber-app's control.rs METHODS: auth, ping,
version, methods, ui.inspect, viewport.screenshot, command.list, command.run,
mesh.load, project.open — the wire protocol is auth-first, one JSON object
per line (scripts/control_call.py, scripts/desktop_checks.py's ControlClient).

Output: perf/baseline.json keeps the existing flat schema perf_compare.py
reads (viewport_mean_ms, viewport_p95_ms, input_photon_p50_ms,
input_photon_p95_ms — see TRACKED in scripts/perf_compare.py) and adds the
provenance keys date, commit, gpu, resolution, mesh_tris, samples, p50_ms,
p95_ms, viewport_fps_estimate (all non-TRACKED keys are inert to the gate).

Usage:
  python3 scripts/perf_rebaseline.py [--binary PATH] [--samples N]
      [--warmup N] [--scratch-root DIR] [--cache-dir DIR] [--keep]
      [--out perf/baseline.json] [--dry-run]

--dry-run prints the plan and launches nothing. Exit 0 on a written baseline.

Note on adapters with a 256 MiB max_buffer_size (e.g. Intel HD Graphics 530,
Vulkan and GL): the 2D UV-view wireframe of a 1M-tri mesh tessellates into a
~360 MB egui index buffer and wgpu fatals the device (label egui_index_buffer).
The script therefore runs `window.toggle_hide_ui` after project.open and
before mesh.load; the 3D viewport keeps rendering the mesh via its own wgpu
buffers, and all sampling goes over the control channel regardless.
"""

import argparse
import json
import os
import re
import secrets
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
from array import array
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_BINARY = REPO / "target" / "release" / "umber"
DEFAULT_OUT = REPO / "perf" / "baseline.json"

# The fixture mirrors crates/umber-gpu/src/ref_scene.rs::cube_grid(289):
# 6 faces x (289+1)^2 vertices, 12*289^2 = 1,002,252 triangles.
SUBDIVISIONS = 289
SET_NAME = "ReferenceMaterial"  # single glTF material name -> texture set name
RESOLUTION = 4096  # DEFAULT_TARGET_SIZE (umber-paint paint_state.rs)

# Input events rotated across samples: every one dispatches through
# command.run into the app and forces a repaint (the control accept thread
# request_repaints after queueing each UI-forwarded request, 5f3e33f).
INPUT_COMMANDS = [
    "paint.toggle_mask_mode",
    "paint.invert_mask",
    "view.toggle_grid",
    "view.toggle_wireframe",
    "view.cycle_view_mode",
    "tool.brush_larger",
    "tool.brush_smaller",
]

# cube_grid's face table verbatim (ref_scene.rs): (normal, u, v) with
# u x v = normal, so a→b→c winds CCW seen from outside.
FACES = [
    ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)),
    ((-1.0, 0.0, 0.0), (0.0, 0.0, 1.0), (0.0, 1.0, 0.0)),
    ((0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (1.0, 0.0, 0.0)),
    ((0.0, -1.0, 0.0), (1.0, 0.0, 0.0), (0.0, 0.0, 1.0)),
    ((0.0, 0.0, 1.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)),
    ((0.0, 0.0, -1.0), (0.0, 1.0, 0.0), (1.0, 0.0, 0.0)),
]
# (normal, u, v) with u x v = normal, so a->b->c winds CCW seen from outside
# — the ref_scene.rs table verbatim.


# ---------------------------------------------------------------------------
# control channel client (JSONL, auth-first — same wire protocol as
# scripts/control_call.py, inlined because that file has no library API;
# identical to scripts/desktop_checks.py's ControlClient)
# ---------------------------------------------------------------------------

class ControlClient:
    def __init__(self, port, token, timeout=60.0):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=timeout)
        self.sock.settimeout(timeout)
        self.f = self.sock.makefile("rw", encoding="utf-8", newline="\n")
        self._id = 0
        r = self._rpc("auth", {"token": token})
        if not r.get("ok"):
            raise RuntimeError(f"control auth failed: {r}")

    def _rpc(self, method, params):
        self._id += 1
        self.f.write(json.dumps({"id": self._id, "method": method, "params": params or {}}) + "\n")
        self.f.flush()
        line = self.f.readline()
        if not line:
            raise RuntimeError(f"control connection closed on {method}")
        return json.loads(line)

    def call(self, method, params=None):
        r = self._rpc(method, params)
        if not r.get("ok"):
            raise RuntimeError(f"{method} failed: {r.get('error')}")
        return r.get("result")

    def close(self):
        try:
            self.f.close()
            self.sock.close()
        except OSError:
            pass


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---------------------------------------------------------------------------
# launch (scratch XDG dirs, per-run token — pattern from desktop_checks.py)
# ---------------------------------------------------------------------------

class Launch:
    def __init__(self, binary, scratch, name, control=True, cache_dir=None):
        self.run = scratch / name
        self.run.mkdir(parents=True, exist_ok=True)
        xdg = self.run / "xdg"
        self.env = os.environ.copy()  # display/Wayland env passes through
        for key, sub in (("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                         ("XDG_STATE_HOME", "state")):
            p = xdg / sub
            p.mkdir(parents=True, exist_ok=True)
            self.env[key] = str(p)
        # XDG_CACHE_HOME: per-run scratch by default; --cache-dir points at a
        # persistent dir so the driver's shader cache stays warm across runs
        # (only affects time-to-first-frame, never the measured frames).
        if cache_dir:
            Path(cache_dir).mkdir(parents=True, exist_ok=True)
            self.env["XDG_CACHE_HOME"] = str(cache_dir)
        else:
            p = xdg / "cache"
            p.mkdir(parents=True, exist_ok=True)
            self.env["XDG_CACHE_HOME"] = str(p)
        (self.run / "tmp").mkdir(parents=True, exist_ok=True)
        self.env["TMPDIR"] = str(self.run / "tmp")
        self.token = secrets.token_hex(24)
        self.env["UMBER_CONTROL_TOKEN"] = self.token
        self.port = free_port() if control else None
        args = [str(binary)]
        if control:
            args += ["--control", str(self.port)]
        self.log = open(self.run / "app.log", "ab")
        self.proc = subprocess.Popen(args, cwd=REPO, env=self.env,
                                     stdout=self.log, stderr=subprocess.STDOUT,
                                     start_new_session=True)
        self.client = None

    def log_text(self, n=4000):
        try:
            self.log.flush()
            return (self.run / "app.log").read_text(errors="replace")[-n:]
        except OSError:
            return ""

    def wait_control(self, timeout=120):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"app exited early rc={self.proc.returncode}; log: {self.log_text()}")
            try:
                client = ControlClient(self.port, self.token)
                client.call("ping")
                self.client = client
                return client
            except (OSError, RuntimeError):
                time.sleep(1.0)
        raise RuntimeError(f"control channel not ready in {timeout}s; log: {self.log_text()}")

    def wait_ui_frame(self, client, timeout=480):
        """ping/methods answer on the accept thread; the FIRST UI-forwarded
        call only completes once the first frame has painted, which pays the
        driver's shader compile (minutes cold on Mesa/anv). Poll ui.inspect
        until the UI thread answers."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"app exited during first frame rc={self.proc.returncode}; log: {self.log_text()}")
            try:
                client.call("ui.inspect")
                return
            except (OSError, RuntimeError):
                time.sleep(2.0)
        raise RuntimeError(f"UI thread not painting within {timeout}s; log: {self.log_text()}")

    def alive(self):
        return self.proc.poll() is None

    def close(self):
        try:
            if self.client:
                self.client.close()
        finally:
            if self.alive():
                try:
                    os.killpg(self.proc.pid, signal.SIGTERM)
                    self.proc.wait(10)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    try:
                        os.killpg(self.proc.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            self.log.close()


# ---------------------------------------------------------------------------
# scratch project + deterministic 1M-tri GLB fixture (scratch only)
# ---------------------------------------------------------------------------

def make_project(dirpath):
    """Minimal v1 .umber project with the fixture's set at resolution 4096,
    plus paint/target_sizes.txt pinning the per-set paint size at 4096."""
    dirpath.mkdir(parents=True, exist_ok=True)
    channels = [{"name": "baseColor", "kind": "Color"}]
    doc = {
        "version": 1,
        "texture_sets": [
            {"name": SET_NAME, "resolution": RESOLUTION, "channels": channels, "tiles": {}}
        ],
        "layer_sets": [
            {"texture_set": SET_NAME, "layer_ids": [], "next_layer_id": 1}
        ],
        "settings": {"active_texture_set": SET_NAME},
    }
    (dirpath / "project.json").write_text(json.dumps(doc, indent=2))
    paint = dirpath / "paint"
    paint.mkdir(exist_ok=True)
    (paint / "target_sizes.txt").write_text(f"{SET_NAME}={RESOLUTION}x{RESOLUTION}\n")
    return dirpath / "project.json"


def gen_cube_grid_glb(path, subdivisions=SUBDIVISIONS):
    """Binary glTF mirroring ref_scene.rs::cube_grid(subdivisions):
    [-1,1]^3 cube, each face a subdivisions^2 quad grid with per-face
    (flat normal, per-face UV) vertices, CCW from outside.
    Deterministic (pure arithmetic, no RNG)."""
    n = subdivisions
    row = n + 1
    verts_per_face = row * row
    positions = array("f")
    normals = array("f")
    uvs = array("f")
    indices = array("I")
    for normal, u, v in FACES:
        for j in range(row):
            t = j / n
            for i in range(row):
                s = i / n
                a, b = 2.0 * s - 1.0, 2.0 * t - 1.0
                for k in range(3):
                    positions.append(normal[k] + a * u[k] + b * v[k])
                    normals.append(normal[k])
                uvs.append(s)
                uvs.append(t)
        base = len(positions) // 3 - verts_per_face
        for j in range(n):
            for i in range(n):
                a = base + j * row + i
                b = a + 1
                c = a + 1 + row
                d = a + row
                indices.extend((a, b, c, a, c, d))
    tri_count = len(indices) // 3
    assert tri_count == 12 * n * n, tri_count
    if n == SUBDIVISIONS:
        assert tri_count == 1_002_252, tri_count

    pos_bytes = positions.tobytes()
    nrm_bytes = normals.tobytes()
    uv_bytes = uvs.tobytes()
    idx_bytes = indices.tobytes()
    bin_blob = pos_bytes + nrm_bytes + uv_bytes + idx_bytes
    views = []
    off = 0
    for blob in (pos_bytes, nrm_bytes, uv_bytes, idx_bytes):
        views.append({"buffer": 0, "byteOffset": off, "byteLength": len(blob)})
        off += len(blob)
    nverts = len(positions) // 3
    gltf = {
        "asset": {"version": "2.0", "generator": "scripts/perf_rebaseline.py cube_grid(289)"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
            "indices": 3, "material": 0, "mode": 4,
        }]}],
        "materials": [{"name": SET_NAME}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": nverts, "type": "VEC3",
             "min": [-1.0, -1.0, -1.0], "max": [1.0, 1.0, 1.0]},
            {"bufferView": 1, "componentType": 5126, "count": nverts, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": nverts, "type": "VEC2"},
            {"bufferView": 3, "componentType": 5125, "count": len(indices), "type": "SCALAR"},
        ],
        "bufferViews": views,
        "buffers": [{"byteLength": len(bin_blob)}],
    }
    json_blob = json.dumps(gltf, separators=(",", ":")).encode()
    json_blob += b" " * (-len(json_blob) % 4)
    bin_pad = bin_blob + b"\x00" * (-len(bin_blob) % 4)
    total = 12 + 8 + len(json_blob) + 8 + len(bin_pad)
    with open(path, "wb") as f:
        f.write(struct.pack("<4sII", b"glTF", 2, total))
        f.write(struct.pack("<I4s", len(json_blob), b"JSON"))
        f.write(json_blob)
        f.write(struct.pack("<I4s", len(bin_pad), b"BIN\x00"))
        f.write(bin_pad)
    return nverts, tri_count


# ---------------------------------------------------------------------------
# stats (nearest-rank percentiles, the input_photon.rs / ref_scene.rs rule)
# ---------------------------------------------------------------------------

def percentile_sorted(sorted_vals, p):
    rank = int(-(-p * len(sorted_vals) // 1))  # ceil(p*n)
    rank = max(1, min(rank, len(sorted_vals)))
    return sorted_vals[rank - 1]


def stats(samples):
    s = sorted(samples)
    return {
        "p50": percentile_sorted(s, 0.50),
        "p95": percentile_sorted(s, 0.95),
        "mean": sum(s) / len(s),
        "n": len(s),
    }


# ---------------------------------------------------------------------------
# gpu string from the app's own log (egui_wgpu's adapter census line)
# ---------------------------------------------------------------------------

ADAPTER_RE = re.compile(
    r"backend: (?P<backend>\w+), device_type: (?P<dtype>\w+), name: \"(?P<name>[^\"]+)\""
    r", driver: \"(?P<driver>[^\"]+)\", driver_info: \"(?P<dinfo>[^\"]+)\""
)


def gpu_string_from_log(log_text):
    vulkan = None
    for m in ADAPTER_RE.finditer(log_text):
        if m.group("backend").lower() == "vulkan" and vulkan is None:
            vulkan = m
    m = vulkan or (ADAPTER_RE.search(log_text))
    if not m:
        return "unknown (no egui_wgpu adapter line in app.log)"
    name = m.group("name")
    software = any(k in name.lower() for k in ("llvmpipe", "lavapipe", "software", "svga"))
    cls = "lavapipe (software Vulkan)" if software else f"real GPU ({m.group('dtype')}, {m.group('backend')} backend)"
    return (f"{name} — {m.group('driver')}, {m.group('dinfo')} | {cls}"
            f" | adapter census: {m.group('backend')}/{m.group('dtype')}")


# ---------------------------------------------------------------------------
# the run
# ---------------------------------------------------------------------------

def plan_lines(binary, samples, warmup, out):
    return [
        f"PLAN [dry-run]: binary={binary}",
        f"  1. mkdtemp scratch; XDG_CONFIG/DATA/STATE/CACHE_HOME + TMPDIR redirected into it; per-run UMBER_CONTROL_TOKEN",
        f"  2. generate deterministic cube_grid({SUBDIVISIONS}) GLB fixture under scratch "
        f"(6x{row_str()}^2 verts, 12*{SUBDIVISIONS}^2 = 1,002,252 tris)",
        f"  3. write scratch .umber project: texture set '{SET_NAME}' resolution={RESOLUTION} "
        f"+ paint/target_sizes.txt '{SET_NAME}={RESOLUTION}x{RESOLUTION}'",
        f"  4. launch {binary} --control <free port>, log to scratch app.log; poll ui.inspect until the first frame paints (cold shader compile)",
        f"  5. control channel (auth-first JSONL): project.open, window.toggle_hide_ui (avoids the >256MiB egui index-buffer fatal on 1M-tri wireframe), mesh.load fixture, ui.inspect (assert paint_available + mesh), command.list (pick enabled input commands)",
        f"  6. warmup: {warmup} x viewport.screenshot (shader JIT, unmeasured)",
        f"  7. samples: {samples} x [command.run (input event) -> viewport.screenshot (photon: "
        f"post-present frame)]; sample = send->screenshot-reply wall ms; consecutive screenshot "
        f"replies give viewport frame intervals",
        f"  8. percentiles nearest-rank (input_photon.rs rule); write {out} keeping the flat "
        f"TRACKED keys (viewport_mean_ms, viewport_p95_ms, input_photon_p50_ms, "
        f"input_photon_p95_ms) + provenance keys",
        f"  9. SIGTERM the app, reap, remove scratch (unless --keep)",
        "DRY-RUN: no processes launched, no scratch dir created, no file written.",
    ]


def row_str():
    return SUBDIVISIONS + 1


def run(binary, samples, warmup, scratch_root, keep, out, cache_dir):
    if not binary.is_file():
        print(f"FAIL: binary not found: {binary}")
        return 1
    if scratch_root:
        scratch = Path(scratch_root)
        scratch.mkdir(parents=True, exist_ok=True)
        scratch = Path(tempfile.mkdtemp(prefix="rebaseline-", dir=scratch))
    else:
        scratch = Path(tempfile.mkdtemp(prefix="umber-rebaseline-"))
    print(f"# scratch: {scratch}  binary: {binary}")
    launch = Launch(binary, scratch, "run", control=True, cache_dir=cache_dir)
    try:
        c = launch.wait_control()
        print("# control channel up:", c.call("methods"))
        launch.wait_ui_frame(c)  # first frame pays the driver's shader compile
        print("# first UI frame painted")

        fixture = scratch / "fixture" / f"cube_grid_{SUBDIVISIONS}.glb"
        fixture.parent.mkdir(parents=True, exist_ok=True)
        nverts, tris = gen_cube_grid_glb(fixture)
        print(f"# fixture: {fixture} ({fixture.stat().st_size} bytes, {nverts} verts, {tris} tris)")

        proj = make_project(scratch / "project")
        print("# project.open:", c.call("project.open", {"path": str(proj.parent)}))
        # hide UI chrome before loading the 1M-tri mesh: on adapters whose
        # max_buffer_size is 256 MiB (this box's Intel HD 530, Vulkan and GL
        # alike), the 2D UV-view wireframe of a 1M-tri mesh tessellates into
        # a ~360 MB egui index buffer and wgpu fatals (measured crash:
        # "Buffer size 360933516 > maximum 268435456", label egui_index_buffer).
        # window.toggle_hide_ui avoids that path; the 3D viewport keeps
        # rendering the mesh through its own wgpu callback buffers.
        r = c.call("command.run", {"id": "window.toggle_hide_ui"})
        print("# hide_ui:", r)
        print("# mesh.load:", c.call("mesh.load", {"path": str(fixture)}))

        ins = None
        deadline = time.time() + 120
        while time.time() < deadline:
            ins = c.call("ui.inspect")
            if ins.get("mesh"):
                break
            time.sleep(1.0)
        if not ins or not ins.get("paint_available"):
            print("FAIL: paint session not available after project.open; log:", launch.log_text(800))
            return 1
        if not ins.get("mesh"):
            print("FAIL: mesh not loaded; log:", launch.log_text(800))
            return 1
        print(f"# inspect: paint_available={ins['paint_available']} mesh={ins['mesh']} "
              f"active_set={ins.get('active_set')} sets={ins.get('sets')}")

        enabled = {r["id"] for r in c.call("command.list")["commands"] if r.get("enabled")}
        inputs = [cmd for cmd in INPUT_COMMANDS if cmd in enabled]
        if not inputs:
            print("FAIL: no enabled input commands; command.list:", c.call("command.list"))
            return 1
        print("# input commands:", inputs)

        # warmup frames (shader JIT) — unmeasured, per ref_scene WARMUP_FRAMES
        for _ in range(warmup):
            c.call("viewport.screenshot")

        i2p_ms = []        # input (command.run send) -> photon (screenshot reply)
        frame_ms = []      # consecutive screenshot replies (present-to-present)
        prev_present = None
        for i in range(samples):
            cmd = inputs[i % len(inputs)]
            t_send = time.perf_counter()
            c.call("command.run", {"id": cmd})
            c.call("viewport.screenshot")
            t_present = time.perf_counter()
            i2p_ms.append((t_present - t_send) * 1000.0)
            if prev_present is not None:
                frame_ms.append((t_present - prev_present) * 1000.0)
            prev_present = t_present
            if (i + 1) % 50 == 0:
                print(f"# sample {i + 1}/{samples} (last i2p {i2p_ms[-1]:.2f} ms)", flush=True)

        i2p = stats(i2p_ms)
        vp = stats(frame_ms)
        gpu = gpu_string_from_log(launch.log_text(200000))
        commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO,
                                capture_output=True, text=True).stdout.strip()
        date = subprocess.run(["date", "-u", "+%F"], capture_output=True, text=True).stdout.strip()

        baseline = {
            "date": date,
            "commit": commit,
            "gpu": gpu,
            "resolution": RESOLUTION,
            "mesh_tris": tris,
            "samples": i2p["n"],
            "p50_ms": round(i2p["p50"], 4),
            "p95_ms": round(i2p["p95"], 4),
            "viewport_fps_estimate": round(1000.0 / vp["mean"], 3) if vp["mean"] else None,
            # existing flat schema perf_compare.py TRACKED reads:
            "viewport_mean_ms": round(vp["mean"], 4),
            "viewport_p95_ms": round(vp["p95"], 4),
            "input_photon_p50_ms": round(i2p["p50"], 4),
            "input_photon_p95_ms": round(i2p["p95"], 4),
            "adapter": gpu,
            "note": (
                f"worklist #2 re-baseline on the {RESOLUTION}-default paint path: "
                f"{i2p['n']} input->photon samples over the app's --control JSONL channel "
                f"(input = command.run drained at frame top; photon = viewport.screenshot "
                f"reply, egui's post-present Screenshot event; sample = send->reply wall ms, "
                f"PNG encode included in the photon leg). Viewport frame times are consecutive "
                f"screenshot-reply intervals ({vp['n']} frames). Mesh = cube_grid({SUBDIVISIONS}) "
                f"GLB ({tris} tris) generated under scratch; scratch project pins texture set "
                f"'{SET_NAME}' at resolution {RESOLUTION} (project.json + paint/target_sizes.txt). "
                f"Present proxy is post-present, so input->photon is a lower bound on true "
                f"event->present (same honesty note as input_photon.rs). Non-TRACKED keys are "
                f"inert to perf_compare.py."
            ),
        }
        out = Path(out)
        out.write_text(json.dumps(baseline, indent=2) + "\n")
        json.loads(out.read_text())  # parse check
        print("# baseline written:", out)
        print(json.dumps(baseline, indent=2))
        return 0
    finally:
        launch.close()
        if keep:
            print(f"# scratch kept: {scratch}")
        else:
            shutil.rmtree(scratch, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--binary", default=str(DEFAULT_BINARY))
    ap.add_argument("--samples", type=int, default=300,
                    help="timed input->photon samples (default: %(default)s)")
    ap.add_argument("--warmup", type=int, default=3,
                    help="unmeasured warmup frames (default: %(default)s)")
    ap.add_argument("--scratch-root", default=None,
                    help="parent dir for the scratch dir (default: mkdtemp under TMPDIR)")
    ap.add_argument("--keep", action="store_true", help="keep the scratch dir")
    ap.add_argument("--out", default=str(DEFAULT_OUT),
                    help="baseline path (default: %(default)s)")
    ap.add_argument("--cache-dir", default=None,
                    help="persistent XDG_CACHE_HOME for the app (warm shader cache; "
                         "default: per-run scratch)")
    ap.add_argument("--dry-run", action="store_true", help="print the plan, launch nothing")
    args = ap.parse_args()
    if args.dry_run:
        for line in plan_lines(Path(args.binary), args.samples, args.warmup, args.out):
            print(line)
        return 0
    return run(Path(args.binary), args.samples, args.warmup, args.scratch_root,
               args.keep, args.out, args.cache_dir)


if __name__ == "__main__":
    sys.exit(main())
