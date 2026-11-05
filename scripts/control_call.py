#!/usr/bin/env python3
"""Minimal client for umber-app's loopback control channel (--control PORT).

Usage:
  control_call.py --port 8377 --token-file /tmp/umber-control-token \
      --method command.list
  control_call.py --port 8377 --token-file ... --method command.run \
      --params '{"id": "view.toggle_grid"}'

Protocol: one JSON object per line. First line must be the auth request
{"method": "auth", "params": {"token": ...}}; then one request per line.
Prints each reply line verbatim. Loopback only.
"""
import argparse
import json
import socket
import sys


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--token-file", required=True)
    ap.add_argument("--method", required=True)
    ap.add_argument("--params", default="{}")
    ap.add_argument("--timeout", type=float, default=35.0)
    args = ap.parse_args()

    with open(args.token_file) as f:
        token = f.read().strip()
    params = json.loads(args.params)

    with socket.create_connection((args.host, args.port), timeout=args.timeout) as s:
        s.settimeout(args.timeout)
        f = s.makefile("rw", encoding="utf-8", newline="\n")
        f.write(json.dumps({"id": 0, "method": "auth", "params": {"token": token}}) + "\n")
        f.flush()
        auth_reply = json.loads(f.readline())
        if not auth_reply.get("ok"):
            print(json.dumps(auth_reply), file=sys.stderr)
            return 2
        f.write(json.dumps({"id": 1, "method": args.method, "params": params}) + "\n")
        f.flush()
        print(f.readline().strip())
    return 0


if __name__ == "__main__":
    sys.exit(main())
