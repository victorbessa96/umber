#!/usr/bin/env python3
"""desktop_checks.py — desktop smoke driver for docs/loop/desktop-checks.md checks 1-4.

Run from a terminal ON THE DESKTOP (needs a real X11/Wayland session and
xdotool):  python3 scripts/desktop_checks.py --binary ./target/debug/umber

Checks (one `CHECK n: PASS/FAIL <evidence>` line each, in order):
  1  quit guard, dirty close — scratch project + unsaved stroke, window
     close via xdotool is intercepted (quit-guard dialog appears in the
     frame diff), Cancel keeps the process alive, then Discard quits and
     project.json bytes are unchanged on disk.
  2  clean SIGTERM — known PID, SIGTERM exits promptly, exit code recorded,
     no crash artifact under the scratch state dir.
  3  keyboard-driven paint stroke — xdotool drag in the viewport dirties
     the doc (ui.inspect), file.save_project writes the paint/ sidecar
     PNGs under the scratch project dir.
  4  screenshot sanity — fixture FBX loaded, --control viewport.screenshot
     PNG parses (magic + IHDR dims) and is not one uniform color.

Isolation: every launch gets a fresh scratch dir with XDG_CONFIG_HOME /
XDG_DATA_HOME / XDG_STATE_HOME / XDG_CACHE_HOME / TMPDIR redirected into
it and a per-run UMBER_CONTROL_TOKEN, on a unique scratch project dir —
nothing touches the real config. Safe to re-run (idempotent); child
processes are reaped in a finally block. Exit code = number of failed
checks.  --dry-run prints the plan and launches nothing.  --keep keeps
the scratch dir for inspection.

Notes: the control-channel JSONL client is inlined below —
scripts/control_call.py exposes only an argparse main(), no reusable
function, so the same wire protocol (auth-first, one JSON per line) is
implemented here directly. The quit-guard dialog is an egui Window inside
the app's own window (id "quit-guard"), so it is detected via the
viewport.screenshot frame diff, not via xdotool window search.
"""

import argparse
import base64
import hashlib
import json
import os
import secrets
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
FIXTURE = REPO / "crates" / "umber-mesh" / "tests" / "assets" / "cube_2mat_ascii7400.fbx"
DEFAULT_BINARY = REPO / "target" / "debug" / "umber"

# The fixture's two material names double as texture-set names, so mesh
# load routes tiles 3000/3001 onto the scratch project's sets (see
# scripts/gen-fbx-2mat-cube.py).
SET_NAMES = ["Body", "Hilt"]


# --------------------------------------------------------------------------
# control channel client (JSONL, auth-first — same wire protocol as
# scripts/control_call.py, inlined because that file has no library API)
# --------------------------------------------------------------------------

class ControlClient:
    def __init__(self, port, token, timeout=40.0):
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


# --------------------------------------------------------------------------
# minimal PNG decode (8-bit, color type 6/2, non-interlaced) — stdlib only
# --------------------------------------------------------------------------

def decode_png(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("bad PNG magic")
    pos, w, h, depth, ctype, idat = 8, 0, 0, 0, 0, b""
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos:pos + 4])
        typ = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + ln]
        if typ == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
        elif typ == b"IDAT":
            idat += body
        elif typ == b"IEND":
            break
        pos += 12 + ln
    if depth != 8 or ctype not in (2, 6):
        raise ValueError(f"unsupported PNG: depth={depth} ctype={ctype}")
    bpp = 4 if ctype == 6 else 3
    raw = zlib.decompress(idat)
    stride = w * bpp
    out = bytearray()
    prev = bytearray(stride)
    p = 0
    for _y in range(h):
        filt = raw[p]
        p += 1
        row = bytearray(raw[p:p + stride])
        p += stride
        if filt == 1:
            for i in range(bpp, stride):
                row[i] = (row[i] + row[i - bpp]) & 0xFF
        elif filt == 2:
            for i in range(stride):
                row[i] = (row[i] + prev[i]) & 0xFF
        elif filt == 3:
            for i in range(stride):
                a = row[i - bpp] if i >= bpp else 0
                row[i] = (row[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif filt == 4:
            for i in range(stride):
                a = row[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                pp = a + b - c
                pa, pb, pc = abs(pp - a), abs(pp - b), abs(pp - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                row[i] = (row[i] + pr) & 0xFF
        out += row
        prev = row
    return w, h, bpp, bytes(out)


def png_stats(data):
    w, h, bpp, px = decode_png(data)
    colors = {px[i:i + bpp] for i in range(0, len(px), bpp)}
    return w, h, len(colors)


def frame_diff(a, b):
    """Count differing pixels + their bbox between two decoded frames."""
    wa, ha, bppa, pa = a
    wb, hb, bppb, pb = b
    if (wa, ha) != (wb, hb) or bppa != bppb:
        return 10**9, None
    n, x0, y0, x1, y1 = 0, wa, ha, -1, -1
    for y in range(ha):
        ra, rb = pa[y * wa * bppa:(y + 1) * wa * bppa], pb[y * wb * bppb:(y + 1) * wb * bppb]
        for x in range(wa):
            if ra[x * bppa:x * bppa + bppa] != rb[x * bppb:x * bppb + bppb]:
                n += 1
                if x < x0: x0 = x
                if x > x1: x1 = x
                if y < y0: y0 = y
                if y > y1: y1 = y
    return n, (x0, y0, x1, y1) if n else None


# --------------------------------------------------------------------------
# xdotool + process helpers
# --------------------------------------------------------------------------

def have_xdotool():
    return shutil.which("xdotool") is not None


def xd(*args, timeout=25):
    return subprocess.run(["xdotool", *args], capture_output=True, text=True, timeout=timeout)


def find_umber_window(timeout=90):
    deadline = time.time() + timeout
    while time.time() < deadline:
        r = xd("search", "--name", "^Umber$")
        if r.returncode == 0 and r.stdout.strip():
            return r.stdout.split()[0]
        time.sleep(1.0)
    return None


def win_geom(win):
    r = xd("getwindowgeometry", "--shell", win)
    g = {}
    for line in r.stdout.splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            g[k.strip()] = int(v)
    return g  # X Y WIDTH HEIGHT


def paint_stroke(win):
    """xdotool LMB drag across the viewport center (brush is the default
    tool; check 3 sends `key b` first to be sure)."""
    g = win_geom(win)
    cx, cy = g["WIDTH"] // 2, g["HEIGHT"] // 2
    pts = [(cx - 150, cy - 60), (cx - 60, cy + 30), (cx + 40, cy - 20), (cx + 150, cy + 50)]
    cmds = ["mousemove", "--window", win, str(cx - 200), str(cy - 80),
            "mousedown", "1", "sleep", "0.3"]
    for x, y in pts:
        cmds += ["mousemove", "--sync", "--window", win, str(x), str(y), "sleep", "0.25"]
    cmds += ["mouseup", "1"]
    xd(*cmds, timeout=30)


def click_at(win, fx, fy):
    xd("mousemove", "--window", win, str(int(fx)), str(int(fy)),
       "sleep", "0.3", "click", "1", timeout=20)


def make_project(dirpath):
    """Minimal valid v1 .umber project dir (sets named after the fixture's
    materials so mesh-load tile routing lands on them)."""
    dirpath.mkdir(parents=True, exist_ok=True)
    channels = [{"name": "baseColor", "kind": "Color"}]
    doc = {
        "version": 1,
        "texture_sets": [
            {"name": n, "resolution": 1024, "channels": channels, "tiles": {}}
            for n in SET_NAMES
        ],
        "layer_sets": [
            {"texture_set": n, "layer_ids": [], "next_layer_id": 1} for n in SET_NAMES
        ],
        "settings": {"active_texture_set": SET_NAMES[0]},
    }
    (dirpath / "project.json").write_text(json.dumps(doc, indent=2))
    return dirpath / "project.json"


class Launch:
    """One app launch on a unique scratch dir with XDG fully redirected."""

    def __init__(self, binary, scratch, name, control=True, mesh_at_start=False):
        self.run = scratch / name
        self.run.mkdir(parents=True, exist_ok=True)
        xdg = self.run / "xdg"
        self.env = os.environ.copy()
        for key, sub in (("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                         ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache"),
                         ("TMPDIR", "../tmp")):
            p = (xdg / sub) if not sub.startswith("..") else self.run / "tmp"
            p.mkdir(parents=True, exist_ok=True)
            self.env[key] = str(p)
        self.token = secrets.token_hex(24)
        self.env["UMBER_CONTROL_TOKEN"] = self.token
        self.port = free_port() if control else None
        args = [str(binary)]
        if mesh_at_start:
            args.append(str(FIXTURE))
        if control:
            args += ["--control", str(self.port)]
        self.log = open(self.run / "app.log", "ab")
        self.proc = subprocess.Popen(args, cwd=REPO, env=self.env,
                                     stdout=self.log, stderr=subprocess.STDOUT,
                                     start_new_session=True)
        self.client = None

    def log_tail(self, n=400):
        try:
            self.log.flush()
            return (self.run / "app.log").read_text(errors="replace")[-n:]
        except OSError:
            return ""

    def wait_control(self, timeout=90):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"app exited early rc={self.proc.returncode}; log: {self.log_tail()}")
            try:
                client = ControlClient(self.port, self.token)
                client.call("ping")
                self.client = client
                return client
            except (OSError, RuntimeError):
                time.sleep(1.0)
        raise RuntimeError(f"control channel not ready in {timeout}s; log: {self.log_tail()}")

    def alive(self):
        return self.proc.poll() is None

    def screenshot(self):
        res = self.client.call("viewport.screenshot")
        return base64.b64decode(res["png_base64"]), res

    def inspect(self):
        return self.client.call("ui.inspect")

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


def wait_dirty(launch, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            if launch.inspect().get("dirty"):
                return True
        except RuntimeError:
            pass
        time.sleep(0.5)
    return False


# --------------------------------------------------------------------------
# the four checks
# --------------------------------------------------------------------------

DIALOG_PIX_THRESHOLD = 800  # frame-diff pixels that count as "dialog drawn"


def check1_quit_guard(binary, scratch):
    if not have_xdotool():
        return "FAIL", "xdotool not found on PATH"
    proj_json = make_project(scratch / "c1" / "project")
    launch = Launch(binary, scratch, "c1", control=True)
    try:
        c = launch.wait_control()
        c.call("project.open", {"path": str(proj_json.parent)})
        c.call("mesh.load", {"path": str(FIXTURE)})
        win = find_umber_window()
        if not win:
            return "FAIL", "no 'Umber' window found via xdotool"
        xd("windowactivate", "--sync", win)
        xd("key", "b")  # brush tool
        time.sleep(1)
        paint_stroke(win)
        if not wait_dirty(launch):
            return "FAIL", f"stroke did not dirty the doc; log: {launch.log_tail(200)}"
        base_png, _ = launch.screenshot()
        base = decode_png(base_png)
        hash0 = hashlib.sha256(proj_json.read_bytes()).hexdigest()[:16]
        xd("windowclose", win)
        time.sleep(2.5)
        if not launch.alive():
            return "FAIL", "process exited on window close despite dirty doc (quit guard did not intercept)"
        shot_png, _ = launch.screenshot()
        n, bbox = frame_diff(base, decode_png(shot_png))
        if n < DIALOG_PIX_THRESHOLD or not bbox:
            return "FAIL", f"no quit-guard dialog in frame after close (diff={n}px); alive=True"
        # Cancel is the rightmost of the three buttons at the dialog's foot.
        cx = bbox[0] + (bbox[2] - bbox[0]) * 0.85
        cy = bbox[1] + (bbox[3] - bbox[1]) * 0.80
        cancelled = False
        n2 = n
        for _ in range(3):
            click_at(win, cx, cy)
            time.sleep(1.2)
            if not launch.alive():
                return "FAIL", "process exited after clicking Cancel (should stay alive)"
            n2, _ = frame_diff(base, decode_png(launch.screenshot()[0]))
            if n2 < DIALOG_PIX_THRESHOLD:
                cancelled = True
                break
        if not cancelled:
            return "FAIL", f"Cancel click did not dismiss the dialog (diff still {n2}px), process alive"
        # Second close -> Discard (middle button) -> quit proceeds.
        xd("windowclose", win)
        time.sleep(2.5)
        if not launch.alive():
            return "FAIL", "process exited on second close (guard should ask again while dirty)"
        shot_png, _ = launch.screenshot()
        n3, bbox = frame_diff(base, decode_png(shot_png))
        if n3 < DIALOG_PIX_THRESHOLD or not bbox:
            return "FAIL", f"dialog did not reappear on second close (diff={n3}px)"
        cx = bbox[0] + (bbox[2] - bbox[0]) * 0.50
        cy = bbox[1] + (bbox[3] - bbox[1]) * 0.80
        clicked_discard = False
        rc = None
        for _ in range(3):
            click_at(win, cx, cy)
            try:
                rc = launch.proc.wait(15)
                clicked_discard = True
                break
            except subprocess.TimeoutExpired:
                continue
        if not clicked_discard:
            return "FAIL", "Discard click did not quit the app within 15s x3"
        hash1 = hashlib.sha256(proj_json.read_bytes()).hexdigest()[:16]
        if hash1 != hash0:
            return "FAIL", f"project.json changed on disk across dirty-quit ({hash0} -> {hash1})"
        return "PASS", (f"dirty close intercepted (dialog diff={n}px), Cancel kept pid "
                        f"{launch.proc.pid} alive and dialog dismissed, Discard quit rc={rc}, "
                        f"project.json sha {hash0} unchanged")
    finally:
        launch.close()


def check2_sigterm(binary, scratch):
    launch = Launch(binary, scratch, "c2", control=False, mesh_at_start=True)
    try:
        win = find_umber_window(timeout=90) if have_xdotool() else None
        if not win:
            time.sleep(20)  # no xdotool/window visibility: give GPU startup a beat
        pid = launch.proc.pid
        os.kill(pid, signal.SIGTERM)
        try:
            rc = launch.proc.wait(30)
        except subprocess.TimeoutExpired:
            return "FAIL", f"SIGTERM to pid {pid}: still running after 30s"
        state = launch.run / "xdg" / "state"
        crashes = [p for p in state.rglob("*") if p.is_file() and "crash" in p.name.lower()]
        code = -rc if rc < 0 else rc  # 143 == 128+SIGTERM via shell; python gives -15
        if crashes:
            return "FAIL", f"SIGTERM pid={pid} exit={code} but crash artifacts written: {crashes}"
        return "PASS", (f"SIGTERM pid={pid} -> clean exit code {code} "
                        f"(143 = no SIGTERM handler, per the 2026-11-05 note) within 30s, "
                        f"0 crash artifacts under scratch state dir, window={'found' if win else 'n/a'}")
    finally:
        launch.close()


def check3_keyboard_stroke_sidecar(binary, scratch):
    if not have_xdotool():
        return "FAIL", "xdotool not found on PATH"
    proj_json = make_project(scratch / "c3" / "project")
    launch = Launch(binary, scratch, "c3", control=True)
    try:
        c = launch.wait_control()
        c.call("project.open", {"path": str(proj_json.parent)})
        c.call("mesh.load", {"path": str(FIXTURE)})
        win = find_umber_window()
        if not win:
            return "FAIL", "no 'Umber' window found via xdotool"
        xd("windowactivate", "--sync", win)
        xd("key", "b")
        time.sleep(1)
        paint_stroke(win)
        if not wait_dirty(launch):
            return "FAIL", f"xdotool stroke did not set dirty; log: {launch.log_tail(200)}"
        c.call("command.run", {"id": "file.save_project"})
        sidecars = []
        deadline = time.time() + 20
        while time.time() < deadline and not sidecars:
            sidecars = sorted((proj_json.parent / "paint").rglob("*.png"))
            if not sidecars:
                time.sleep(0.5)
        if not sidecars:
            return "FAIL", f"dirty=True after stroke but no paint/ sidecar PNG after save; log: {launch.log_tail(200)}"
        names = ", ".join(str(p.relative_to(proj_json.parent)) for p in sidecars)
        return "PASS", f"xdotool stroke -> ui.inspect dirty=True; file.save_project wrote sidecar(s): {names}"
    finally:
        launch.close()


def check4_screenshot_sanity(binary, scratch):
    launch = Launch(binary, scratch, "c4", control=True, mesh_at_start=True)
    try:
        c = launch.wait_control()
        deadline = time.time() + 60
        mesh = None
        while time.time() < deadline:
            ins = c.call("ui.inspect")
            mesh = ins.get("mesh")
            if mesh:
                break
            time.sleep(1.0)
        if not mesh:
            return "FAIL", f"fixture mesh not loaded at startup; log: {launch.log_tail(200)}"
        time.sleep(3)  # let a few frames paint
        png, res = launch.screenshot()
        if png[:8] != b"\x89PNG\r\n\x1a\n":
            return "FAIL", "viewport.screenshot payload is not a PNG"
        w, h, ncolors = png_stats(png)
        if (w, h) != (res.get("width"), res.get("height")):
            return "FAIL", f"PNG IHDR {w}x{h} != reply dims {res.get('width')}x{res.get('height')}"
        if ncolors <= 1:
            return "FAIL", f"screenshot is one uniform color ({w}x{h}, 1 color)"
        return "PASS", (f"viewport.screenshot PNG ok: {w}x{h}, magic valid, dims match reply, "
                        f"{ncolors} distinct colors, mesh={mesh}")
    finally:
        launch.close()


CHECKS = [
    ("1 quit-guard dirty close (xdotool close -> Cancel -> Discard)", check1_quit_guard),
    ("2 clean SIGTERM (known PID, exit code recorded)", check2_sigterm),
    ("3 keyboard stroke -> paint/ sidecar under scratch project", check3_keyboard_stroke_sidecar),
    ("4 viewport.screenshot PNG sanity on fixture FBX", check4_screenshot_sanity),
]


def plan_lines(binary):
    return [f"CHECK {i+1} [dry-run] would run: {name} (binary={binary}, fixture={FIXTURE})"
            for i, (name, _) in enumerate(CHECKS)]


def main():
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--binary", default=str(DEFAULT_BINARY),
                    help="path to the umber binary (default: %(default)s)")
    ap.add_argument("--dry-run", action="store_true",
                    help="print the plan for checks 1-4 and launch nothing")
    ap.add_argument("--scratch-root", default=None,
                    help="parent dir for the per-run scratch dir (default: mkdtemp)")
    ap.add_argument("--keep", action="store_true", help="keep the scratch dir after the run")
    args = ap.parse_args()

    binary = Path(args.binary)
    if args.dry_run:
        for line in plan_lines(binary):
            print(line)
        print("DRY-RUN: no processes launched, no scratch dir created.")
        return 0

    if not binary.is_file():
        print(f"CHECK 1..4: FAIL binary not found: {binary}")
        return 4
    if not FIXTURE.is_file():
        print(f"CHECK 1..4: FAIL fixture not found: {FIXTURE}")
        return 4

    if args.scratch_root:
        scratch = Path(args.scratch_root)
        scratch.mkdir(parents=True, exist_ok=True)
        scratch = Path(tempfile.mkdtemp(prefix="dchecks-", dir=scratch))
    else:
        scratch = Path(tempfile.mkdtemp(prefix="umber-dchecks-"))
    print(f"# scratch: {scratch}  binary: {binary}")

    failures = 0
    try:
        for i, (name, fn) in enumerate(CHECKS, start=1):
            try:
                status, evidence = fn(binary, scratch)
            except Exception as e:  # a check that throws is a FAIL, not a crash
                status, evidence = "FAIL", f"exception: {type(e).__name__}: {e}"
            print(f"CHECK {i}: {status} {evidence}", flush=True)
            if status != "PASS":
                failures += 1
    finally:
        if args.keep:
            print(f"# scratch kept: {scratch}")
        else:
            shutil.rmtree(scratch, ignore_errors=True)
    return failures


if __name__ == "__main__":
    sys.exit(main())
