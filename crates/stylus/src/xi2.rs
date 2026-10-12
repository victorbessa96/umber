//! X11 XI2 valuator backend (Linux, feature `xi2`, default OFF).
//!
//! Reads pen Motion/Button/Enter/Leave events off the X server through the
//! XInput 2.x protocol and maps them onto the crate's backend-agnostic
//! [`crate::TabletEvent`] (tagged [`crate::InputSource::X11Xinput2`]).
//! The x11_dl constant names (`XI_Motion`, `XI_ButtonPress`, …) are FFI
//! bindings from `x11-dl`, not ours to rename — the case lint is waived
//! module-wide.
#![allow(non_upper_case_globals)]
//! The FFI goes through `x11-dl` — the same dlopen wrapper winit 0.30
//! itself uses (`winit-0.30.13/src/platform_impl/linux/x11/` re-exports
//! `x11_dl::xinput2::*` / `x11_dl::xlib::*`), so every signature below
//! was read off `x11-dl-2.21.0/src/{xinput2,xlib}.rs`, not guessed.
//!
//! # Judgment calls and v1 boundaries
//!
//! * **Its own `XOpenDisplay`, not winit's.** The app cannot lend us the
//!   event loop's `Display*`: winit owns it (`xdisplay.rs` opens it, the
//!   event loop pumps it) and an Xlib `Display` is not thread-safe to
//!   share with a second pumper. So v1 opens a SECOND connection — its
//!   own XI2 client — and selects its own event mask on the root window.
//!   Cost: one extra X connection per app run. Sharing winit's display
//!   (via a raw-handle hook eframe 0.36 does not expose) is the named
//!   follow-up, not v1.
//! * **Proximity is synthesized.** XI2 has NO proximity event types (XI 1.x
//!   had them; 2.x removed them — the `XI_*` constants run DeviceChanged,
//!   Key, Button, Motion, Enter, Leave, …). `ProximityIn`/`Out` therefore
//!   come from `XI_Enter`/`XI_Leave` on the root window. Mode/detail
//!   filtering (grab modes, virtual crossings) is NOT applied in v1 —
//!   every Enter/Leave toggles hover, and the poller only logs it.
//! * **Axis roles come from valuator labels** (`XIValuatorClassInfo.label`
//!   resolved with `XGetAtomName`): `"Abs Pressure"` → pressure,
//!   `"Abs Tilt X"` / `"Abs Tilt Y"` → tilt. That is the xf86-input-wacom
//!   / libinput label convention — Wacom, Huion and XP-Pen all expose it.
//!   If no pressure label resolves, the mapping has no pressure axis and
//!   every Motion reports 1.0 (the touch-fallback philosophy: a stroke
//!   must paint, never go ghost-faint). Tilt needs BOTH axes, else `None`.
//! * **Tilt units are degrees, raw.** Wacom reports tilt-x/y natively in
//!   degrees (typical axis range −64..64 or −90..90); the values are used
//!   as degrees and clamped to the axis `[min, max]`. No unit conversion.
//! * **Pressure normalizes per-axis:** `(raw − min) / (max − min)` clamped
//!   to `0..1`. A degenerate axis (`max <= min`) reports 1.0 (paints).
//!   A NaN raw value normalizes to NaN and the app's pressure resolve
//!   (`stylus_input::resolve_pressure`) folds non-positive/non-finite to
//!   1.0 — the same `None`/0.0→1.0 rule the `Event::Touch` arm uses.
//! * **Contact is button detail 1.** `XI_ButtonPress`/`Release` with
//!   `detail == 1` (logical left = tip) become `Contact` — or `Eraser`
//!   when the event's `sourceid` belongs to an eraser device. Barrel
//!   buttons (`detail >= 2`) become `Button`. The eraser set is built at
//!   `open()` from `XIQueryDevice` names containing `"eraser"`
//!   (case-insensitive — the xf86-input-wacom eraser slave is literally
//!   `"… Pen eraser"`); an eraser reported as a valuator or a barrel
//!   button instead is NOT detected in v1.
//! * **Positions are root-relative** (`root_x`/`root_y` — identical to
//!   `event_x`/`event_y` here since the selection window IS the root).
//!   The app maps them into the viewport itself; this backend never sees
//!   egui coordinates.
//! * **Time is server milliseconds → ns** (`ms * 1_000_000`, saturating).
//!   The epoch is the X server's start — relative only, like winink's
//!   `dwTime` epoch. Cross-backend clock comparison is meaningless.
//! * **No hotplug re-query.** Mappings snapshot at `open()`; a tablet
//!   plugged in later strokes at pressure 1.0 with no tilt until restart.
//!   Re-querying on `XI_HierarchyChanged` is the follow-up.
//! * **Poll is bounded:** one `poll()` drains at most 256 events so a
//!   tablet flood cannot stall a frame; the remainder waits for the next
//!   frame (Xlib queues it on our own connection — nothing is lost).
//!
//! # Unverified on real hardware
//!
//! No tablet is present on the build machine: `open()` against real
//! hardware, the label scan against a real `XIQueryDevice` reply, and the
//! valuator-mask walk against real `XIDeviceEvent`s have never run. The
//! pure mapping layer (`normalize_pressure`, `valuator_pairs`,
//! `pressure_from_pairs`, `tilt_from_pairs`, `label_role`,
//! `is_eraser_device`, `contact_or_eraser`, `ms_to_ns`) is unit-tested;
//! the `unsafe` pump (`open`/`poll`/`Drop`) is honest FFI with the
//! return-code contracts read off the XI2 man pages (not off a run).

use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::os::raw::{c_int, c_uchar};
use std::ptr;

use thiserror::Error;
use x11_dl::xinput2::{
    XIAllDevices, XIDeviceEvent, XIDeviceInfo, XIEnterEvent, XIEventMask, XISetMask,
    XIValuatorClass, XIValuatorClassInfo, XI_ButtonPress, XI_ButtonRelease, XI_Enter, XI_Leave,
    XI_Motion, XInput2,
};
use x11_dl::xlib::{Atom, Display, GenericEvent, Time, Window, XEvent, XGenericEventCookie, Xlib};

use crate::{InputSource, SourcedEvent, TabletEvent};

/// Failures opening the XI2 backend. All are runtime-environment
/// conditions (no X server, old server, no tablet stack) — the app treats
/// every one as "touch fallback", never as fatal.
#[derive(Debug, Error)]
pub enum Xi2Error {
    #[error("XI2: could not open libX11/libXi: {detail}")]
    Library { detail: String },
    #[error("XI2: XOpenDisplay failed (no X display)")]
    NoDisplay,
    #[error("XI2: XInputExtension not present on this server")]
    NoExtension,
    #[error("XI2: server speaks XInput {major}.{minor}, need 2.x")]
    BadVersion { major: c_int, minor: c_int },
    #[error("XI2: XIQueryDevice failed")]
    QueryFailed,
    #[error("XI2: XISelectEvents failed")]
    SelectFailed,
}

/// One valuator axis: its XI2 axis number plus the protocol range.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AxisDesc {
    index: c_int,
    min: f64,
    max: f64,
}

/// A slave device's interesting axes, keyed by `sourceid`.
#[derive(Debug, Clone, Default, PartialEq)]
struct DeviceMapping {
    pressure: Option<AxisDesc>,
    tilt_x: Option<AxisDesc>,
    tilt_y: Option<AxisDesc>,
}

/// Which role a valuator label atom names. The strings are the
/// xf86-input-wacom / libinput convention (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisRole {
    Pressure,
    TiltX,
    TiltY,
    Other,
}

/// Maps an `XIValuatorClassInfo` label to its role. Pure (testable).
fn label_role(label: &str) -> AxisRole {
    match label {
        "Abs Pressure" => AxisRole::Pressure,
        "Abs Tilt X" => AxisRole::TiltX,
        "Abs Tilt Y" => AxisRole::TiltY,
        _ => AxisRole::Other,
    }
}

/// Whether an `XIQueryDevice` device name is an eraser end. Pure
/// (testable): substring match, ASCII case-insensitive (driver names
/// are ASCII — `"Wacom … Pen eraser"`).
fn is_eraser_device(name: &str) -> bool {
    name.to_lowercase().contains("eraser")
}

/// Server milliseconds → nanoseconds. Saturating: a far-future `Time`
/// must not panic a debug build.
fn ms_to_ns(time_ms: Time) -> u64 {
    time_ms.saturating_mul(1_000_000)
}

/// Normalizes one raw pressure sample into `0..1` against the axis
/// range. Degenerate ranges (`max <= min`) report 1.0 — the stroke must
/// paint (see the module docs). NaN passes through as NaN; the app's
/// pressure resolve folds it to 1.0.
fn normalize_pressure(raw: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 1.0;
    }
    ((raw - min) / (max - min)).clamp(0.0, 1.0) as f32
}

/// Clamps one raw tilt sample to its axis range (degrees, Wacom
/// convention — see the module docs).
fn clamp_tilt(raw: f64, min: f64, max: f64) -> f32 {
    raw.clamp(min, max) as f32
}

/// Zips set-axis numbers with their values. `mask` is the raw
/// `XIValuatorState.mask` bytes, `values` one entry per set bit (the XI2
/// guarantee). Pure (testable) — the unsafe pump builds the two slices
/// and calls this.
fn valuator_pairs(mask: &[u8], values: &[f64]) -> Vec<(c_int, f64)> {
    let mut pairs = Vec::new();
    let mut values = values.iter();
    for (byte_idx, byte) in mask.iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) != 0 {
                let Some(value) = values.next() else {
                    // Fewer values than set bits: the driver lied. Stop
                    // rather than pair axes with the wrong samples.
                    return pairs;
                };
                pairs.push((byte_idx as c_int * 8 + bit, *value));
            }
        }
    }
    pairs
}

/// Pressure for one Motion event: the mapped pressure axis's sample, or
/// 1.0 when the device has no pressure axis or this event carries no
/// sample for it (see the module docs).
fn pressure_from_pairs(mapping: Option<&DeviceMapping>, pairs: &[(c_int, f64)]) -> f32 {
    let Some(axis) = mapping.and_then(|m| m.pressure) else {
        return 1.0;
    };
    let Some((_, raw)) = pairs.iter().find(|(index, _)| *index == axis.index) else {
        return 1.0;
    };
    normalize_pressure(*raw, axis.min, axis.max)
}

/// Tilt for one Motion event: `Some([x, y])` degrees only when BOTH axes
/// are mapped AND both are sampled (a half tilt is worse than none).
fn tilt_from_pairs(mapping: Option<&DeviceMapping>, pairs: &[(c_int, f64)]) -> Option<[f32; 2]> {
    let mapping = mapping?;
    let (x_axis, y_axis) = (mapping.tilt_x?, mapping.tilt_y?);
    let (_, x_raw) = pairs.iter().find(|(index, _)| *index == x_axis.index)?;
    let (_, y_raw) = pairs.iter().find(|(index, _)| *index == y_axis.index)?;
    Some([
        clamp_tilt(*x_raw, x_axis.min, x_axis.max),
        clamp_tilt(*y_raw, y_axis.min, y_axis.max),
    ])
}

/// Tip ButtonPress/Release → `Contact`, or `Eraser` when the source is
/// an eraser device (see the module docs). Pure (testable).
fn contact_or_eraser(is_eraser: bool, down: bool, time_ns: u64) -> TabletEvent {
    if is_eraser {
        TabletEvent::Eraser {
            active: down,
            time_ns,
        }
    } else {
        TabletEvent::Contact { down, time_ns }
    }
}

/// The XI2 event reader: our own X connection plus the per-device axis
/// mappings snapshotted at [`Self::open`].
///
/// Stays on the UI thread: the raw `Display*` is `!Send + !Sync` by
/// construction, and only `poll()` touches it. `Drop` closes the
/// display (before the dlopen handles unload — field drop order runs
/// `Drop::drop` first).
pub struct Xi2Reader {
    xlib: Xlib,
    xi: XInput2,
    display: *mut Display,
    xi_opcode: c_int,
    mappings: HashMap<c_int, DeviceMapping>,
    erasers: HashSet<c_int>,
}

impl Xi2Reader {
    /// Opens a second X connection, negotiates XI 2.x, snapshots the
    /// device/axis mappings, and selects Motion + Button + Enter/Leave
    /// on the root window. Every failure is an [`Xi2Error`] (the app
    /// falls back to touch — never fatal).
    pub fn open() -> Result<Self, Xi2Error> {
        let xlib = Xlib::open().map_err(|err| Xi2Error::Library {
            detail: format!("{err:?}"),
        })?;
        let xi = XInput2::open().map_err(|err| Xi2Error::Library {
            detail: format!("{err:?}"),
        })?;
        // SAFETY: null display name = `$DISPLAY`; null return = no
        // server. No other thread touches this `Display*` (it is ours
        // alone — the winit-sharing question is settled in the module
        // docs).
        let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
        if display.is_null() {
            return Err(Xi2Error::NoDisplay);
        }
        let mut reader = Self {
            xlib,
            xi,
            display,
            xi_opcode: 0,
            mappings: HashMap::new(),
            erasers: HashSet::new(),
        };
        // From here every early return must close the display: `Drop`
        // only runs on a fully built `Self`, so close by hand.
        if let Err(err) = reader.negotiate() {
            reader.close();
            return Err(err);
        }
        if let Err(err) = reader.scan_devices() {
            reader.close();
            return Err(err);
        }
        if let Err(err) = reader.select_root() {
            reader.close();
            return Err(err);
        }
        log::info!(
            "stylus: XI2 backend open ({} devices, {} erasers)",
            reader.mappings.len(),
            reader.erasers.len()
        );
        Ok(reader)
    }

    /// `XQueryExtension("XInputExtension")` for the XI opcode, then
    /// `XIQueryVersion(2.x)`. Return-code contracts per the XI2 man
    /// pages (unverified at runtime — no tablet hardware here).
    fn negotiate(&mut self) -> Result<(), Xi2Error> {
        let mut opcode: c_int = 0;
        let mut first_event: c_int = 0;
        let mut first_error: c_int = 0;
        // SAFETY: all out-params are valid `c_int` slots; the name is a
        // static C string. Nonzero return = extension present.
        let present = unsafe {
            (self.xlib.XQueryExtension)(
                self.display,
                c"XInputExtension".as_ptr(),
                &mut opcode,
                &mut first_event,
                &mut first_error,
            )
        };
        if present == 0 {
            return Err(Xi2Error::NoExtension);
        }
        self.xi_opcode = opcode;
        let (mut major, mut minor) = (2, 0);
        // SAFETY: same out-param contract. Returns 0 (`Success`) with
        // the negotiated version written back.
        let status = unsafe { (self.xi.XIQueryVersion)(self.display, &mut major, &mut minor) };
        if status != 0 || major < 2 {
            return Err(Xi2Error::BadVersion { major, minor });
        }
        Ok(())
    }

    /// One `XIQueryDevice(XIAllDevices)` pass: per-`sourceid` axis
    /// mappings (label-resolved) plus the eraser set (name-scanned).
    fn scan_devices(&mut self) -> Result<(), Xi2Error> {
        let mut count: c_int = 0;
        // SAFETY: `&mut count` is a valid out-param. Null return =
        // failure; otherwise an array of `count` entries owned by us
        // until `XIFreeDeviceInfo`.
        let infos = unsafe { (self.xi.XIQueryDevice)(self.display, XIAllDevices, &mut count) };
        if infos.is_null() {
            return Err(Xi2Error::QueryFailed);
        }
        if count <= 0 {
            // No devices (or a nonsense count — never build a slice
            // from it): free and carry empty mappings.
            unsafe { (self.xi.XIFreeDeviceInfo)(infos) };
            return Ok(());
        }
        // SAFETY: `count` entries, valid until freed below.
        let infos = unsafe { std::slice::from_raw_parts(infos, count as usize) };
        for info in infos {
            self.scan_device(info);
        }
        // SAFETY: the exact pointer `XIQueryDevice` returned.
        unsafe { (self.xi.XIFreeDeviceInfo)(infos.as_ptr() as *mut XIDeviceInfo) };
        Ok(())
    }

    /// Folds one `XIDeviceInfo` into `mappings`/`erasers`. Called only
    /// from [`Self::scan_devices`] (its unsafety contract covers us).
    fn scan_device(&mut self, info: &XIDeviceInfo) {
        let name = if info.name.is_null() {
            String::new()
        } else {
            // SAFETY: driver-provided NUL-terminated name, read-only.
            unsafe { CStr::from_ptr(info.name).to_string_lossy().into_owned() }
        };
        let eraser = is_eraser_device(&name);
        if info.classes.is_null() || info.num_classes <= 0 {
            return;
        }
        // SAFETY: `num_classes` entries, valid for the scan's duration.
        let classes =
            unsafe { std::slice::from_raw_parts(info.classes, info.num_classes as usize) };
        for class in classes {
            if (*class).is_null() {
                continue;
            }
            // SAFETY: the class array owns these; read-only.
            let any = unsafe { &**class };
            if any._type != XIValuatorClass {
                continue;
            }
            // SAFETY: `_type` discriminates — this entry IS a valuator
            // class (`xinput2.rs`: `XIValuatorClassInfo`).
            let valuator = unsafe { &*(*class as *const XIValuatorClassInfo) };
            let role = self.atom_role(valuator.label);
            let axis = AxisDesc {
                index: valuator.number,
                min: valuator.min,
                max: valuator.max,
            };
            let mapping = self.mappings.entry(valuator.sourceid).or_default();
            match role {
                AxisRole::Pressure => mapping.pressure = Some(axis),
                AxisRole::TiltX => mapping.tilt_x = Some(axis),
                AxisRole::TiltY => mapping.tilt_y = Some(axis),
                AxisRole::Other => {}
            }
            if eraser {
                self.erasers.insert(info.deviceid);
                self.erasers.insert(valuator.sourceid);
            }
        }
        if eraser {
            log::info!("stylus: XI2 eraser device: {name:?}");
        }
    }

    /// Resolves a valuator label atom to its [`AxisRole`]. Null atoms
    /// and unresolvable names are `Other` (v1: no fallback ordering —
    /// see the module docs).
    fn atom_role(&self, atom: Atom) -> AxisRole {
        if atom == 0 {
            return AxisRole::Other;
        }
        // SAFETY: atom came from `XIQueryDevice`; the returned string
        // (when non-null) is ours until `XFree`.
        let name_ptr = unsafe { (self.xlib.XGetAtomName)(self.display, atom) };
        if name_ptr.is_null() {
            return AxisRole::Other;
        }
        let role = unsafe { CStr::from_ptr(name_ptr).to_string_lossy() };
        let role = label_role(&role);
        unsafe { (self.xlib.XFree)(name_ptr as *mut std::ffi::c_void) };
        role
    }

    /// Selects Motion + ButtonPress/Release + Enter/Leave for ALL
    /// devices on the root window. Return-code contract per the XI2 man
    /// page (nonzero = success; unverified at runtime).
    fn select_root(&mut self) -> Result<(), Xi2Error> {
        // SAFETY: our own display; root always exists on it.
        let root: Window = unsafe { (self.xlib.XDefaultRootWindow)(self.display) };
        // 4 bytes cover event types 0..31 (`XI_LASTEVENT` is 26).
        let mut bytes = [0 as c_uchar; 4];
        XISetMask(&mut bytes, XI_Motion);
        XISetMask(&mut bytes, XI_ButtonPress);
        XISetMask(&mut bytes, XI_ButtonRelease);
        XISetMask(&mut bytes, XI_Enter);
        XISetMask(&mut bytes, XI_Leave);
        let mut mask = XIEventMask {
            deviceid: XIAllDevices,
            mask_len: bytes.len() as c_int,
            mask: bytes.as_mut_ptr(),
        };
        // SAFETY: `mask` + `bytes` outlive the call; root is ours.
        let status = unsafe { (self.xi.XISelectEvents)(self.display, root, &mut mask, 1) };
        if status == 0 {
            return Err(Xi2Error::SelectFailed);
        }
        Ok(())
    }

    /// Closes the display. Only called for half-built readers (see
    /// [`Self::open`]) — fully built ones close in `Drop`.
    fn close(&mut self) {
        // SAFETY: non-null (checked at open), owned by us, closed once:
        // `open` calls this only on paths that return `Err` (no `Drop`
        // follows), and `Drop` runs only on success paths.
        unsafe {
            (self.xlib.XCloseDisplay)(self.display);
        }
        self.display = ptr::null_mut();
    }

    /// Maps one `XIDeviceEvent` (Motion or Button) plus its valuator
    /// pairs onto `SourcedEvent`s. Pure given the pairs (the mask walk
    /// is [`valuator_pairs`]).
    fn map_device_event(&self, dev: &XIDeviceEvent, pairs: &[(c_int, f64)]) -> Vec<SourcedEvent> {
        let time_ns = ms_to_ns(dev.time);
        let mapping = self.mappings.get(&dev.sourceid);
        let is_eraser = self.erasers.contains(&dev.sourceid);
        let event = match dev.evtype {
            XI_Motion => TabletEvent::Motion {
                pos: [dev.root_x as f32, dev.root_y as f32],
                pressure: pressure_from_pairs(mapping, pairs),
                tilt: tilt_from_pairs(mapping, pairs),
                time_ns,
            },
            XI_ButtonPress | XI_ButtonRelease => {
                let down = dev.evtype == XI_ButtonPress;
                if dev.detail == 1 {
                    contact_or_eraser(is_eraser, down, time_ns)
                } else {
                    TabletEvent::Button {
                        button: dev.detail.clamp(0, 255) as u8,
                        down,
                        time_ns,
                    }
                }
            }
            _ => return Vec::new(),
        };
        vec![SourcedEvent {
            source: InputSource::X11Xinput2,
            event,
        }]
    }

    /// Reads one GenericEvent cookie's payload. Returns the events (0
    /// or 1 — one XI2 wire event maps to one `TabletEvent`).
    fn map_cookie(&self, cookie: &XGenericEventCookie) -> Vec<SourcedEvent> {
        if cookie.evtype == XI_Enter {
            // SAFETY: `evtype` discriminates — payload IS an enter event;
            // `XGetEventData` fetched it (caller contract).
            let enter = unsafe { &*(cookie.data as *const XIEnterEvent) };
            return vec![SourcedEvent {
                source: InputSource::X11Xinput2,
                event: TabletEvent::ProximityIn {
                    time_ns: ms_to_ns(enter.time),
                },
            }];
        }
        if cookie.evtype == XI_Leave {
            let enter = unsafe { &*(cookie.data as *const XIEnterEvent) };
            return vec![SourcedEvent {
                source: InputSource::X11Xinput2,
                event: TabletEvent::ProximityOut {
                    time_ns: ms_to_ns(enter.time),
                },
            }];
        }
        if cookie.evtype == XI_Motion
            || cookie.evtype == XI_ButtonPress
            || cookie.evtype == XI_ButtonRelease
        {
            // SAFETY: same discrimination contract for device events.
            let dev = unsafe { &*(cookie.data as *const XIDeviceEvent) };
            let pairs = self.device_pairs(dev);
            return self.map_device_event(dev, &pairs);
        }
        Vec::new()
    }

    /// Builds the valuator pairs for one device event from its raw
    /// mask/values pointers (the XI2 mask idiom — see the module docs).
    fn device_pairs(&self, dev: &XIDeviceEvent) -> Vec<(c_int, f64)> {
        const MAX_MASK_BYTES: usize = 8;
        let state = &dev.valuators;
        if state.mask.is_null() || state.values.is_null() || state.mask_len <= 0 {
            return Vec::new();
        }
        let mask_len = (state.mask_len as usize).min(MAX_MASK_BYTES);
        // SAFETY: `mask_len` bytes readable (`XIValuatorState` owns at
        // least `mask_len`); the values count equals the set-bit count
        // (the XI2 guarantee), capped by the same walk.
        unsafe {
            let mask = std::slice::from_raw_parts(state.mask as *const u8, mask_len);
            let axes: usize = mask.iter().map(|b| b.count_ones() as usize).sum();
            let values = std::slice::from_raw_parts(state.values, axes);
            valuator_pairs(mask, values)
        }
    }
}

impl crate::StylusStream for Xi2Reader {
    /// Drains queued XI2 events (at most 256 — see the module docs).
    /// Never blocks: `XPending == 0` returns empty immediately.
    fn drain(&mut self) -> Vec<SourcedEvent> {
        const MAX_PER_POLL: usize = 256;
        let mut out = Vec::new();
        // SAFETY: our own display, pumped only here.
        while out.len() < MAX_PER_POLL && unsafe { (self.xlib.XPending)(self.display) } > 0 {
            // SAFETY: zeroed `XEvent` is a valid out-param for
            // `XNextEvent`; union field reads below are unsafe blocks.
            let mut raw: XEvent = unsafe { std::mem::zeroed() };
            unsafe {
                (self.xlib.XNextEvent)(self.display, &mut raw);
            }
            // SAFETY: union read — `type_` is the common initial field.
            if unsafe { raw.type_ } != GenericEvent {
                continue;
            }
            // SAFETY: union read — the event IS a cookie holder.
            let mut cookie: XGenericEventCookie = unsafe { raw.generic_event_cookie };
            if cookie.extension != self.xi_opcode {
                continue;
            }
            // SAFETY: `XGetEventData` fetches the payload (nonzero =
            // fetched); every path below runs `XFreeEventData` before
            // continuing. `cookie.data` is valid between the two.
            if unsafe { (self.xlib.XGetEventData)(self.display, &mut cookie) } == 0 {
                continue;
            }
            out.extend(self.map_cookie(&cookie));
            unsafe {
                (self.xlib.XFreeEventData)(self.display, &mut cookie);
            }
        }
        out
    }
}

impl Drop for Xi2Reader {
    fn drop(&mut self) {
        if !self.display.is_null() {
            // SAFETY: owned, non-null, closed exactly once (half-built
            // readers null it in `close` — see `open`).
            unsafe {
                (self.xlib.XCloseDisplay)(self.display);
            }
            self.display = ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_normalizes_mid_range() {
        // 512 of 0..1023 — the exact fraction, not a rounded literal.
        let p = normalize_pressure(512.0, 0.0, 1023.0);
        assert!(
            (p - (512.0 / 1023.0) as f32).abs() < 1e-6,
            "mid pressure must be the exact fraction, got {p}"
        );
    }

    #[test]
    fn pressure_pins_the_endpoints() {
        assert_eq!(normalize_pressure(0.0, 0.0, 1023.0), 0.0);
        assert_eq!(normalize_pressure(1023.0, 0.0, 1023.0), 1.0);
    }

    #[test]
    fn pressure_clamps_outside_the_axis_range() {
        assert_eq!(normalize_pressure(-40.0, 0.0, 1023.0), 0.0);
        assert_eq!(normalize_pressure(5000.0, 0.0, 1023.0), 1.0);
    }

    #[test]
    fn pressure_degenerate_range_paints_full() {
        // A driver that reports max <= min carries no range information;
        // the stroke must paint, not vanish (module-docs boundary).
        assert_eq!(normalize_pressure(0.0, 0.0, 0.0), 1.0);
        assert_eq!(normalize_pressure(7.0, 5.0, 5.0), 1.0);
    }

    #[test]
    fn tilt_clamps_to_axis_range_in_degrees() {
        assert_eq!(clamp_tilt(100.0, -64.0, 64.0), 64.0);
        assert_eq!(clamp_tilt(-100.0, -64.0, 64.0), -64.0);
        assert_eq!(clamp_tilt(12.5, -64.0, 64.0), 12.5);
    }

    #[test]
    fn labels_map_to_roles() {
        assert_eq!(label_role("Abs Pressure"), AxisRole::Pressure);
        assert_eq!(label_role("Abs Tilt X"), AxisRole::TiltX);
        assert_eq!(label_role("Abs Tilt Y"), AxisRole::TiltY);
        assert_eq!(label_role("Rel X"), AxisRole::Other);
        assert_eq!(label_role(""), AxisRole::Other);
        // Case-sensitive: the driver spells them exactly so.
        assert_eq!(label_role("abs pressure"), AxisRole::Other);
    }

    #[test]
    fn eraser_names_match_case_insensitively() {
        assert!(is_eraser_device("Wacom Intuos Pro M Pen eraser"));
        assert!(is_eraser_device("WACOM ERASER"));
        assert!(is_eraser_device("eraser"));
        assert!(!is_eraser_device("Wacom Intuos Pro M Pen"));
        assert!(!is_eraser_device(""));
    }

    #[test]
    fn valuator_mask_walk_pairs_axes_with_values() {
        // Axes 1 and 2 set: values pair in ascending axis order (the XI2
        // mask idiom — axis 0 unset, so the first value is axis 1's).
        let pairs = valuator_pairs(&[0b0000_0110], &[10.0, 20.0]);
        assert_eq!(pairs, vec![(1, 10.0), (2, 20.0)]);
    }

    #[test]
    fn valuator_mask_walk_reaches_the_second_byte() {
        let pairs = valuator_pairs(&[0x00, 0b0000_0001], &[7.0]);
        assert_eq!(pairs, vec![(8, 7.0)]);
    }

    #[test]
    fn valuator_mask_walk_stops_when_values_run_out() {
        // Two bits set, one value: pairs only the first axis rather than
        // misaligning the rest (a corrupt driver must not shift axes).
        let pairs = valuator_pairs(&[0b0000_0011], &[9.0]);
        assert_eq!(pairs, vec![(0, 9.0)]);
    }

    #[test]
    fn pressure_comes_from_the_mapped_axis() {
        let mapping = DeviceMapping {
            pressure: Some(AxisDesc {
                index: 2,
                min: 0.0,
                max: 1023.0,
            }),
            tilt_x: None,
            tilt_y: None,
        };
        let p = pressure_from_pairs(Some(&mapping), &[(0, 400.0), (1, 300.0), (2, 768.0)]);
        assert!(
            (p - (768.0 / 1023.0) as f32).abs() < 1e-6,
            "must sample axis 2, got {p}"
        );
    }

    #[test]
    fn pressure_without_axis_or_sample_is_full() {
        // No mapping (unknown/hotplugged device) and a mapping whose
        // axis this event doesn't sample both paint full (module docs).
        assert_eq!(pressure_from_pairs(None, &[(2, 0.0)]), 1.0);
        let mapping = DeviceMapping {
            pressure: Some(AxisDesc {
                index: 2,
                min: 0.0,
                max: 1023.0,
            }),
            tilt_x: None,
            tilt_y: None,
        };
        assert_eq!(pressure_from_pairs(Some(&mapping), &[(0, 400.0)]), 1.0);
    }

    #[test]
    fn tilt_needs_both_axes_sampled() {
        let both = DeviceMapping {
            pressure: None,
            tilt_x: Some(AxisDesc {
                index: 3,
                min: -64.0,
                max: 64.0,
            }),
            tilt_y: Some(AxisDesc {
                index: 4,
                min: -64.0,
                max: 64.0,
            }),
        };
        assert_eq!(
            tilt_from_pairs(Some(&both), &[(3, 10.0), (4, -5.0)]),
            Some([10.0, -5.0])
        );
        // Half a tilt is worse than none: y missing → None.
        assert_eq!(tilt_from_pairs(Some(&both), &[(3, 10.0)]), None);
        assert_eq!(tilt_from_pairs(None, &[(3, 10.0), (4, -5.0)]), None);
        // Clamped to the axis range (degrees, Wacom convention).
        assert_eq!(
            tilt_from_pairs(Some(&both), &[(3, 90.0), (4, -90.0)]),
            Some([64.0, -64.0])
        );
    }

    #[test]
    fn tip_contact_maps_to_contact_or_eraser() {
        assert_eq!(
            contact_or_eraser(false, true, 11),
            TabletEvent::Contact {
                down: true,
                time_ns: 11
            }
        );
        assert_eq!(
            contact_or_eraser(false, false, 12),
            TabletEvent::Contact {
                down: false,
                time_ns: 12
            }
        );
        assert_eq!(
            contact_or_eraser(true, true, 13),
            TabletEvent::Eraser {
                active: true,
                time_ns: 13
            }
        );
        assert_eq!(
            contact_or_eraser(true, false, 14),
            TabletEvent::Eraser {
                active: false,
                time_ns: 14
            }
        );
    }

    #[test]
    fn server_milliseconds_become_nanoseconds() {
        assert_eq!(ms_to_ns(1500), 1_500_000_000);
        assert_eq!(ms_to_ns(0), 0);
    }
}
