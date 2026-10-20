# eframe 0.36.2 — the raw-window-handle surface (vendored)

Vendored 2026-10-20 by local-10e6ed18 for the W-C4 winink live-hook
lane (the §5 item 2 "API-UNVERIFIED" clause of the W-C3a plan —
verified, no longer unverified). Source of truth: the vendored
crate at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/eframe-0.36.2/`,
lock-pinned `eframe 0.36.2` + `raw-window-handle 0.6.2` (Cargo.lock).
Nothing here is guessed; every cite below was read from the vendored
bytes before writing.

## The two accessors

`CreationContext` and `Frame` BOTH implement
`raw_window_handle::HasWindowHandle` (`epi.rs:99-105` and
`epi.rs:695-701`):

```rust
fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError>
```

- **At creation** — `App::new(cc: &CreationContext)`:
  `cc.window_handle()`. The struct keeps the raw handle in
  `pub(crate) raw_window_handle: Result<RawWindowHandle, HandleError>`
  (`epi.rs:92`), wrapped by the trait impl.
- **Per frame** — `App::update(.., frame: &mut Frame)`:
  `frame.window_handle()` (same field at `epi.rs:688`, same impl
  shape at `epi.rs:704`).
- Both structs also implement `HasDisplayHandle` the same way
  (display handles — not needed by the winink hook, noted for
  completeness).

## The headless (kittest) shape

Both `_new_kittest` constructors mock the field as
`Err(HandleError::NotSupported)` (`epi.rs:134` for CreationContext,
`epi.rs:730` for Frame). Consequence for the hook: **the install
path must treat `Err(HandleError::NotSupported)` as a first-class
"no window" outcome — degrade to no-subclass, do not unwrap, do not
retry-panic.** This keeps every kittest headless-clean by
construction.

## Extracting the HWND (Windows)

`WindowHandle::as_raw()` yields `RawWindowHandle`; the Windows
variant of raw-window-handle 0.6:

```rust
if let raw_window_handle::RawWindowHandle::Win32(w) = handle.as_raw() {
    // w.hwnd: NonZeroIsize — the HWND as a non-zero pointer integer
    let hwnd = windows::Win32::Foundation::HWND(w.hwnd.get());
}
```

`Win32WindowHandle`'s only other field is `hinstance` (not needed by
the subclass APIs). Non-Windows targets never reach this arm — the
winink module is `cfg(windows)`-gated in the stylus crate already
(`crates/stylus/Cargo.toml:18`).

## Install-point design (for the hook lane)

- **One-shot at `App::new`**: the handle is live before the first
  frame (`CreationContext` receives it from the fully-created
  winit window — `epi.rs:86` also keeps the `Arc<winit::window::Window>`
  itself, but that field is `pub(crate)`, so the trait is the only
  public road).
- **Fallback**: if creation-time install is refused or the handle
  errors, retry from `App::update` at most once per frame until it
  succeeds, guarded by an `AtomicBool` (or `std::sync::Once` per
  window) so the subclass is never installed twice.
- **Kittest**: `NotSupported` → the guard marks "no subclass
  available" and the app runs poll-less headless (the stylus stream
  simply stays absent — `open_system_stream()` already returns
  `Option`).

## Evidence index (vendored, read 2026-10-20)

- `eframe-0.36.2/src/epi.rs:17-19` — the `raw_window_handle` import
  (`HasWindowHandle`, `HasDisplayHandle`, `HandleError`,
  `RawWindowHandle`, `WindowHandle` …).
- `epi.rs:85-96` — CreationContext's window/raw-handle fields.
- `epi.rs:99-105` — `impl HasWindowHandle for CreationContext`.
- `epi.rs:122-135` — `_new_kittest` mock (NotSupported).
- `epi.rs:686-692` — Frame's raw-handle fields.
- `epi.rs:695-701` — `impl HasWindowHandle for Frame`.
- `epi.rs:717-731` — Frame `_new_kittest` mock (NotSupported).
- `native/wgpu_integration.rs:13,329` — eframe's own internal use of
  the same trait (`HasWindowHandle as _`), confirming the pattern.
