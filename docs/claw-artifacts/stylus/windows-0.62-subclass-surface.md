# windows 0.62.2 — the subclass API surface (vendored)

Vendored 2026-10-20 by local-10e6ed18 for the W-C4 winink live-hook
lane (the §5 item 2 "API-UNVERIFIED" clause of the W-C3a plan —
verified, no longer unverified). Source of truth: the vendored crate
at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/windows-0.62.2/`,
lock-pinned `windows 0.62.2` (Cargo.lock). Nothing here is guessed;
every cite was read from the vendored bytes before writing.

## The manifest change (the lane's first line)

`crates/stylus/Cargo.toml:19` — the windows dep is target-gated
(`[target.'cfg(windows)'.dependencies]`, optional behind the
`winink = ["dep:windows"]` feature) with features:

```toml
features = ["Win32_UI_Input_Pointer", "Win32_UI_WindowsAndMessaging",
            "Win32_Foundation"]
```

The hook adds `"Win32_UI_Shell"` to that list — one manifest line.
Everything else the hook needs is already enabled (see below).

## The three subclass symbols (Win32/UI/Shell/mod.rs)

```rust
// :4318
pub unsafe fn SetWindowSubclass(hwnd: HWND,
        pfnsubclass: SUBCLASSPROC, uidsubclass: usize,
        dwrefdata: usize) -> BOOL
// :2352
pub unsafe fn RemoveWindowSubclass(hwnd: HWND,
        pfnsubclass: SUBCLASSPROC, uidsubclass: usize) -> BOOL
// :226
pub unsafe fn DefSubclassProc(hwnd: HWND, umsg: u32,
        wparam: WPARAM, lparam: LPARAM) -> LRESULT
// :55755
pub type SUBCLASSPROC = Option<unsafe extern "system" fn(
        hwnd: HWND, umsg: u32, wparam: WPARAM, lparam: LPARAM,
        uidsubclass: usize, dwrefdata: usize) -> LRESULT>;
```

(All under `windows::Win32::UI::Shell::`; the HWND/WPARAM/LPARAM/
LRESULT arms come from the already-enabled `Win32_Foundation`.)

## The contract the hook must keep

- **ALWAYS call `DefSubclassProc`** for every message — observed or
  not. winit's own window procedure (its Touch handling, its
  WM_POINTER paths, everything) sits below the subclass chain;
  skipping the call would eat winit's messages and break the
  platform loop. The §5 item 2 wording is a hard requirement, not
  style.
- The proc observes ONLY the WM_POINTER* set the parked decode
  already consumes — `WM_POINTERDOWN`, `WM_POINTERUPDATE`,
  `WM_POINTERUP`, `WM_POINTERENTER`, `WM_POINTERLEAVE` (imported in
  the parked `winink.rs:30` from the already-enabled
  WindowsAndMessaging feature) — routes each into
  `PenState::decode(pointer_id, msg)` (winink.rs:78), and forwards.
- The proc NEVER owns the window. The parked header's claim —
  "passive decode, no window ownership" (winink.rs:1) — stays true
  under the subclass: it is an observer installed on top of the
  existing chain, not a replacement.
- State hand-off: `dwrefdata` carries the pointer to the hook's
  state (a `Box` leaked into the subclass registration, or an index
  into a static registry keyed by `uidsubclass`). The decode path
  itself stays `&mut self` on `PenState` exactly as parked.
- Teardown: `RemoveWindowSubclass` on app exit (window close can't
  leak the subclass). Pair the calls with the same `uidsubclass`.

## The pointer-side symbols (already enabled, already parked)

`Win32_UI_Input_Pointer` (Pointer/mod.rs, vendored):
- `:95` `GetPointerPenInfo(pointerid: u32, peninfo: *mut
  POINTER_PEN_INFO) -> Result<()>` — the parked decode's core fetch
  (imported at `winink.rs:27` together with `GetPointerInfo`,
  `POINTER_INFO`, `POINTER_PEN_INFO`).
- `:101` `GetPointerPenInfoHistory` — the batched form, available if
  the hook ever needs coalesced history; NOT used by the parked
  decode (noted so the lane doesn't re-invent it).

`Win32_UI_WindowsAndMessaging` (already enabled): the WM_POINTER*
constants, `PT_PEN`, `PEN_MASK_TILT_X/Y` — all imported by the
parked bytes (winink.rs:30).

## The install side (cross-reference)

The HWND comes from eframe's `HasWindowHandle` — see the companion
artifact `eframe-0.36-rwh-surface.md` (CreationContext at App::new,
Frame per-frame fallback, kittest NotSupported-safe). The two
windows-side facts the install needs: `SetWindowSubclass` is
idempotent per (hwnd, uidsubclass) — a second install with the same
id returns FALSE (windows_core::BOOL), which is why the
AtomicBool guard in the eframe doc is the belt to the API's
braces — and the subclass lives for the window's lifetime, so the
install point must be the app's window, not a child or a device
surface.

## Evidence index (vendored, read 2026-10-20)

- `windows-0.62.2/src/Windows/Win32/UI/Shell/mod.rs:226` —
  DefSubclassProc.
- `Shell/mod.rs:2352` — RemoveWindowSubclass.
- `Shell/mod.rs:4318` — SetWindowSubclass.
- `Shell/mod.rs:55755` — SUBCLASSPROC.
- `windows-0.62.2/src/Windows/Win32/UI/Input/Pointer/mod.rs:95` —
  GetPointerPenInfo.
- `Pointer/mod.rs:101` — GetPointerPenInfoHistory.
- Parked S1 bytes citing the message set: `crates/stylus/src/
  winink.rs:1,27,30,78`; `pen_flags.rs:91-131` (PenFlagTracker);
  manifest features at `crates/stylus/Cargo.toml:18-19,27-31`.
