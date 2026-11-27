# Control bridge timeout assertion portability

- Lock: `razul-control-timeout-test`.
- Trigger: full unfiltered CI `46047753773` at `203e760c`.

## Evidence and disposition

The GroundPlane repair cleared the former native abort: Windows completed the app bin with **928 passed, 1 failed** in 1730.79 seconds. The only failure was `control::tests::stale_bridge_cleanup_cannot_close_a_newer_client` at `control.rs:1177`.

The test expected `WouldBlock` when proving the newer TCP client remained open. Windows returned `TimedOut` for the same `set_read_timeout` expiry. The production behavior was correct: the newer client did not reach EOF, and the subsequent generation-2 cleanup still closed it.

## Fix and verification

The assertion now accepts `WouldBlock` on Unix and `TimedOut` on Windows, with the platform distinction documented. The generation-2 EOF assertion remains unchanged, so the test still proves both stale-cleanup isolation and current-cleanup closure.

- Local focused test: **1 passed, 0 failed**.
- `cargo clippy -p umber-app --all-targets -- -D warnings`: clean.
- Rustfmt, scorecard, layers, parity, and `git diff --check`: clean.
- Windows filtered confirmation and full-suite rerun: pending.
