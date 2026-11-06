#!/usr/bin/env python3
"""End-to-end smoke for the release app over its --control channel.

Launches the release `umber` binary with `--control <port>` on a fresh
profile (scratch XDG dirs), then drives the same command ids a human
reaches from the palette:

  auth -> mesh.load <cube> -> command.run layer.add_fill
       -> command.run bake.run -> job.status (wait for the landing)
       -> command.run export.run -> job.status
       -> viewport.screenshot

and asserts:
  * every reply is ok, and an unknown command id is a typed error;
  * the bake wrote at least one file, each nonzero;
  * the export wrote files, each nonzero, and the base-colour PNG is not
    all black (a default fill layer is solid white, so a black export
    means the layer never reached the exported map);
  * the viewport screenshot is not all black (the A2-2 class: a fresh
    mesh must not render as a black silhouette).

Exits nonzero with a message naming the failed step. Kills only the pid
it started. Stdlib only (PNG decode included) so CI needs no pip.

Usage:
  python3 scripts/e2e-smoke.py [--bin target/release/umber]
      [--mesh crates/umber-mesh/tests/assets/cube_uv_ascii7400.fbx]
      [--port 0] [--timeout 600] [--keep] [--break-with UNKNOWN_ID]

--break-with runs an extra command.run with the given id and expects it to
succeed: the deliberate-failure check (an unknown id must make the smoke
exit nonzero).
"""

import argparse
import base64
import json
import os
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib


class SmokeFail(Exception):
    pass


# Calibrated on the cube + one default (solid white) fill layer: see the
# kill report for the measured values on lavapipe and on Intel HD 530.
VIEWPORT_P99_MIN = 120


def inspect_width(inspect: dict) -> float:
    """The window's logical width: the right edge of the widest leaf."""
    return max((leaf["rect"][2] for leaf in inspect.get("panels", [])), default=1.0)


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Control:
    """One authed JSONL connection to the app's control channel."""

    def __init__(self, port: int, token: str, timeout: float):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=timeout)
        self.sock.settimeout(timeout)
        self.f = self.sock.makefile("rw", encoding="utf-8", newline="\n")
        self.next_id = 0
        reply = self.call("auth", {"token": token})
        if reply.get("ok") is not True:
            raise SmokeFail(f"auth refused: {reply}")

    def call(self, method: str, params=None) -> dict:
        self.next_id += 1
        req = {"id": self.next_id, "method": method}
        if params is not None:
            req["params"] = params
        self.f.write(json.dumps(req) + "\n")
        self.f.flush()
        line = self.f.readline()
        if not line:
            raise SmokeFail(f"{method}: connection closed (did the app die?)")
        return json.loads(line)

    def close(self):
        try:
            self.f.close()
            self.sock.close()
        except OSError:
            pass

    def ok(self, method: str, params=None) -> dict:
        reply = self.call(method, params)
        if reply.get("ok") is not True:
            raise SmokeFail(f"{method} {params or ''}: {reply.get('error', reply)}")
        return reply["result"]


def png_rgba(path: str):
    """Decode an 8-bit RGB/RGBA (non-interlaced) PNG -> (w, h, channels, rows)."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SmokeFail(f"{path}: not a PNG")
    pos, idat, w, h, ch = 8, b"", None, 0, 4
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or ctype not in (2, 6) or interlace:
                raise SmokeFail(f"{path}: unsupported PNG (depth {depth}, type {ctype})")
            ch = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    if w is None:
        raise SmokeFail(f"{path}: no IHDR")
    raw = zlib.decompress(idat)
    stride = w * ch
    rows, prev, i = [], bytearray(stride), 0
    for _ in range(h):
        ftype = raw[i]
        line = bytearray(raw[i + 1 : i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - ch] if x >= ch else 0
            b = prev[x]
            c = prev[x - ch] if x >= ch else 0
            if ftype == 1:
                line[x] = (line[x] + a) & 0xFF
            elif ftype == 2:
                line[x] = (line[x] + b) & 0xFF
            elif ftype == 3:
                line[x] = (line[x] + (a + b) // 2) & 0xFF
            elif ftype == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pred) & 0xFF
            elif ftype != 0:
                raise SmokeFail(f"{path}: bad PNG filter type {ftype}")
        rows.append(bytes(line))
        prev = line
    return w, h, ch, rows


def region_luma(path: str, rect, scale: float):
    """Luma percentiles (p50, p99) inside `rect` (logical points, scaled to
    the screenshot's pixels by `scale`)."""
    w, h, ch, rows = png_rgba(path)
    x0, y0, x1, y1 = (int(round(v * scale)) for v in rect)
    x0, y0, x1, y1 = max(0, x0), max(0, y0), min(w, x1), min(h, y1)
    if x1 - x0 < 8 or y1 - y0 < 8:
        raise SmokeFail(f"viewport rect {rect} is empty at scale {scale:.2f}")
    vals = []
    for y in range(y0, y1, 2):
        row = rows[y]
        for x in range(x0, x1, 2):
            i = x * ch
            vals.append((row[i] * 299 + row[i + 1] * 587 + row[i + 2] * 114) // 1000)
    vals.sort()
    return vals[len(vals) // 2], vals[int(len(vals) * 0.99)]


def viewport_rect(inspect: dict):
    """The 3D Viewport leaf's rect from `ui.inspect` (logical points)."""
    for leaf in inspect.get("panels", []):
        if leaf.get("active") == "Viewport":
            return leaf["rect"]
    raise SmokeFail(f"no active Viewport leaf in ui.inspect: {inspect.get('panels')}")


def max_rgb(path: str) -> int:
    """The brightest RGB channel value over every pixel."""
    _, _, ch, rows = png_rgba(path)
    best = 0
    for row in rows:
        for x in range(0, len(row), ch):
            best = max(best, row[x], row[x + 1], row[x + 2])
            if best == 255:
                return best
    return best


def wait_job(ctl: Control, kind: str, before: int, timeout: float) -> dict:
    deadline = time.monotonic() + timeout
    while True:
        status = ctl.ok("job.status")[kind]
        if status["completed"] > before:
            return status
        if time.monotonic() > deadline:
            raise SmokeFail(f"{kind} never landed within {timeout:.0f}s: {status}")
        time.sleep(0.5)


def nonzero_files(label: str, paths):
    if not paths:
        raise SmokeFail(f"{label}: no output files")
    for p in paths:
        if not os.path.isabs(p):
            raise SmokeFail(f"{label}: job.status path is not absolute: {p}")
        if not os.path.isfile(p) or os.path.getsize(p) == 0:
            raise SmokeFail(f"{label}: missing or empty output {p}")


def run(args) -> None:
    root = os.path.abspath(os.path.dirname(os.path.dirname(__file__)))
    binary = os.path.abspath(args.bin or os.path.join(root, "target/release/umber"))
    mesh = os.path.abspath(
        args.mesh or os.path.join(root, "crates/umber-mesh/tests/assets/cube_uv_ascii7400.fbx")
    )
    for p in (binary, mesh):
        if not os.path.exists(p):
            raise SmokeFail(f"missing {p}")
    port = args.port or free_port()
    work = tempfile.mkdtemp(prefix="umber-e2e-")
    token = os.urandom(16).hex()
    env = dict(os.environ)
    # Fresh profile: config/data/state under the scratch dir, nothing from
    # the user's real preferences or layouts leaks in.
    for var, sub in (
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ):
        env[var] = os.path.join(work, sub)
        os.makedirs(env[var], exist_ok=True)
    env["UMBER_CONTROL_TOKEN"] = token
    log_path = os.path.join(work, "app.log")
    log = open(log_path, "w")
    # The bake/export panels write relative to the working directory
    # (`bakes/`, `export/`), so the app runs inside the scratch dir.
    proc = subprocess.Popen(
        [binary, "--control", str(port)], cwd=work, env=env, stdout=log, stderr=log
    )
    print(f"e2e: pid {proc.pid}, port {port}, work {work}")
    passed = False
    try:
        deadline = time.monotonic() + 120
        ctl = None
        while ctl is None:
            if proc.poll() is not None:
                raise SmokeFail(f"app exited {proc.returncode} before listening (log {log_path})")
            try:
                ctl = Control(port, token, args.timeout)
            except OSError:
                if time.monotonic() > deadline:
                    raise SmokeFail("control port never listened within 120s")
                time.sleep(0.5)

        loaded = ctl.ok("mesh.load", {"path": mesh})
        print(f"e2e: mesh.load -> {loaded}")
        unknown = ctl.call("command.run", {"id": "nope.not_a_command"})
        if unknown.get("ok") is not False or "unknown command" not in str(unknown.get("error")):
            raise SmokeFail(f"unknown id was not a typed error: {unknown}")
        if args.break_with:
            ctl.ok("command.run", {"id": args.break_with})

        ctl.ok("command.run", {"id": "layer.add_fill"})
        # The fill rasterizes in the frame loop: let a few frames pass.
        for _ in range(10):
            ctl.ok("ui.inspect")
            time.sleep(0.2)

        before = ctl.ok("job.status")
        ran = ctl.ok("command.run", {"id": "bake.run"})
        print(f"e2e: {ran}")
        bake = wait_job(ctl, "bake", before["bake"]["completed"], args.timeout)
        print(f"e2e: bake landed: {bake['status']}")
        if bake["status"].lower().startswith(("bake failed", "falha")):
            raise SmokeFail(f"bake failed: {bake['status']}")
        nonzero_files("bake", bake["outputs"])

        ran = ctl.ok("command.run", {"id": "export.run"})
        print(f"e2e: {ran}")
        export = wait_job(ctl, "export", before["export"]["completed"], args.timeout)
        print(f"e2e: export landed: {export['status']}")
        if export["status"].lower().startswith(("export failed", "falha")):
            raise SmokeFail(f"export failed: {export['status']}")
        nonzero_files("export", export["written"])
        bases = [p for p in export["written"] if "basecolor" in os.path.basename(p).lower()]
        if not bases:
            raise SmokeFail(f"export wrote no base-colour PNG: {export['written']}")
        for p in bases:
            brightest = max_rgb(p)
            print(f"e2e: {os.path.basename(p)} brightest channel {brightest}")
            if brightest == 0:
                raise SmokeFail(f"{p}: exported base colour is all black")

        # The 3D viewport, cropped by its ui.inspect rect (the window shot
        # holds chrome too, so a whole-frame check could never fail): a
        # white fill on a lit cube must read bright, never the black
        # silhouette of A2-2.
        inspect = ctl.ok("ui.inspect")
        rect = viewport_rect(inspect)
        shot = ctl.ok("viewport.screenshot")
        shot_path = os.path.join(work, "viewport.png")
        with open(shot_path, "wb") as fh:
            fh.write(base64.b64decode(shot["png_base64"]))
        scale = shot["width"] / max(1.0, inspect_width(inspect))
        p50, p99 = region_luma(shot_path, rect, scale)
        print(f"e2e: viewport rect {rect} x{scale:.2f}: luma p50 {p50} p99 {p99}")
        if p99 < VIEWPORT_P99_MIN:
            raise SmokeFail(
                f"3D viewport reads dark (luma p99 {p99} < {VIEWPORT_P99_MIN}): "
                "the filled cube is not showing"
            )
        ctl.close()
        print("E2E SMOKE: PASS")
        passed = True
    finally:
        if proc.poll() is None:
            os.kill(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                os.kill(proc.pid, signal.SIGKILL)
                proc.wait()
        log.close()
        if not passed:
            # CI keeps nothing after the job: put the app's own words in the
            # job log (a bare "exited 101" told nobody why).
            try:
                with open(log_path, errors="replace") as fh:
                    tail = [ln for ln in fh.read().splitlines() if "zbus" not in ln][-60:]
                print("e2e: ---- app.log tail ----", file=sys.stderr)
                for ln in tail:
                    print(f"e2e: | {ln}", file=sys.stderr)
            except OSError as err:
                print(f"e2e: app.log unreadable: {err}", file=sys.stderr)
        if args.keep or not passed:
            # A failed run keeps its app.log and outputs for diagnosis.
            print(f"e2e: kept {work}")
        else:
            shutil.rmtree(work, ignore_errors=True)


def main() -> int:
    ap = argparse.ArgumentParser(description="End-to-end smoke over --control.")
    ap.add_argument("--bin")
    ap.add_argument("--mesh")
    ap.add_argument("--port", type=int, default=0)
    ap.add_argument("--timeout", type=float, default=600.0)
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("--break-with")
    args = ap.parse_args()
    try:
        run(args)
    except SmokeFail as e:
        print(f"E2E SMOKE: FAIL: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
