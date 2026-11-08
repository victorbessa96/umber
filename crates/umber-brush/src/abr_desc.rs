//! Clean-room reader for the Photoshop-family **descriptor** structure that
//! `.abr` brush files carry in their `desc` (and `phry`) `8BIM` sections —
//! row 282 of the wave-abr design brief
//! (`docs/loop/briefs/2026-11-08-wave-abr-desc-design.md`).
//!
//! Provenance: the layout logic is a clean-room port of the vendored
//! `ag-psd` reference (`docs/claw-artifacts/ag-psd/src-tree/src/descriptor.rs`:
//! the version-16 wrapper, the class structure, the OSType dispatch, the
//! units table, the unicode and ascii-or-class-id strings, the reference
//! structure). No reference code, comments or prose were copied; where a
//! prose description disagrees with the reference, the reference wins.
//!
//! Layout, in brief (every multi-byte field big-endian unless named):
//!
//! * a versioned descriptor is a `u32` version (must be 16) followed by a
//!   descriptor structure;
//! * a descriptor structure is a class structure (unicode name +
//!   ascii-or-class-id) then a `u32` item count, each item a key
//!   (ascii-or-class-id), a four-byte OSType signature and the value;
//! * a unicode string is a `u32` code-unit count then that many UTF-16
//!   code units; a trailing NUL unit is dropped;
//! * an ascii-or-class-id is an `i32` length, then that many Latin-1 bytes
//!   — a length of 0 means a four-byte class id follows instead.
//!
//! Every OSType the reference dispatches is decoded (`obj `, `Objc`/`GlbO`,
//! `VlLs`, `doub`, `UntF`/`UnFl`, `TEXT`, `enum`, `long`, `comp`, `bool`,
//! `type`/`GlbC`, `alis`, `tdta`, `ObAr`, `Pth `). An unknown signature is
//! a named strict error ([`AbrError::UnsupportedDescriptorType`]) — the
//! descriptor has no per-item length, so an unknown value cannot be
//! skipped. Extending the table for a real file is a later kill (non-goal).
//!
//! Safety caps (hostile input), each checked BEFORE the allocation it
//! guards, each breach an [`AbrError::DescriptorTooLarge`] naming the cap:
//! [`MAX_DESC_ITEMS`], [`MAX_LIST_ITEMS`], [`MAX_TEXT_CHARS`],
//! [`MAX_RAW_BYTES`], [`MAX_DESC_DEPTH`], [`MAX_REFERENCE_ITEMS`] and
//! [`MAX_DESC_NODES`]. Every read is bounds-checked; a short read is an
//! [`AbrError::MalformedDescriptor`], never a panic.
//!
//! Judgment calls:
//!
//! * The cap values: 4096 items per descriptor, 16384 entries per list /
//!   object array, 16384 UTF-16 units per string, 16 MiB per raw-data or
//!   alias blob, nesting depth 32, 256 reference items. Real brush files
//!   sit far below each (a brush descriptor is a few dozen keys, nesting
//!   under ten); the caps exist so a few hostile bytes cannot ask for a
//!   gigabyte.
//! * [`MAX_DESC_NODES`] (1,000,000 values per versioned descriptor) is an
//!   extra cap beyond the brief's list: the per-level caps alone still let
//!   a 64 MiB section expand into tens of millions of tree nodes.
//! * Negative `i32` counts / lengths are malformed (the reference would
//!   cast them to huge `usize`s).
//! * Unit codes are mapped to the reference's names (`#Pxl` → `Pixels`,
//!   ...); an unknown code is [`AbrError::InvalidDescriptorUnits`].
//! * `parse_percent`/`parse_angle` keep the reference's "missing" defaults
//!   (1.0 and 0.0); a present value with the wrong units is
//!   [`AbrError::InvalidDescriptorUnits`]. `parse_units_to_number` has no
//!   default: a missing value is [`AbrError::MalformedDescriptor`].
//! * Unicode text decodes lossily (an unpaired surrogate becomes U+FFFD):
//!   the reader never fails on text content, only on its length.
//!
//! Non-goals: writing descriptors (never — `.abr` is read-only for umber).

use crate::abr::AbrError;

/// Maximum items in one descriptor structure.
pub const MAX_DESC_ITEMS: usize = 4096;

/// Maximum entries in one `VlLs` list, one `ObAr` object array, or one
/// object-array item's value run.
pub const MAX_LIST_ITEMS: usize = 16_384;

/// Maximum UTF-16 code units (or Latin-1 bytes) in one string.
pub const MAX_TEXT_CHARS: usize = 16_384;

/// Maximum bytes in one `tdta` raw-data or `alis` alias blob.
pub const MAX_RAW_BYTES: usize = 16 * 1024 * 1024;

/// Maximum nesting depth of descriptors and lists.
pub const MAX_DESC_DEPTH: usize = 32;

/// Maximum items in one `obj ` reference structure.
pub const MAX_REFERENCE_ITEMS: usize = 256;

/// Maximum total values decoded from one versioned descriptor.
pub const MAX_DESC_NODES: usize = 1_000_000;

/// The only descriptor version the format defines.
const DESCRIPTOR_VERSION: u32 = 16;

/// The reference's unit-code table: four-byte code → unit name.
const UNITS_MAP: &[(&str, &str)] = &[
    ("#Ang", "Angle"),
    ("#Rsl", "Density"),
    ("#Rlt", "Distance"),
    ("#Nne", "None"),
    ("#Prc", "Percent"),
    ("#Pxl", "Pixels"),
    ("#Mlm", "Millimeters"),
    ("#Pnt", "Points"),
    ("RrPi", "Picas"),
    ("RrIn", "Inches"),
    ("RrCm", "Centimeters"),
];

/// One parsed descriptor: its class and its keyed values, in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    /// The class id (e.g. `sampledBrush`, `computedBrush`, `null`).
    pub class_id: String,
    /// The class's unicode display name (usually empty).
    pub name: String,
    /// The `(key, value)` items, in file order. Keys may repeat; lookups
    /// through [`Descriptor::get`] return the first match.
    pub items: Vec<(String, DescValue)>,
}

/// One descriptor value, one variant per OSType family.
#[derive(Debug, Clone, PartialEq)]
pub enum DescValue {
    /// `Objc` / `GlbO`: a nested descriptor.
    Descriptor(Descriptor),
    /// `VlLs`: a list of typed values.
    List(Vec<DescValue>),
    /// `doub`: a plain double.
    Double(f64),
    /// `UntF` (f64) / `UnFl` (f32, widened): a value with units.
    UnitDouble {
        /// The unit name from the units table (`Pixels`, `Percent`, ...).
        units: &'static str,
        /// The numeric value.
        value: f64,
    },
    /// `TEXT`: a unicode string.
    Text(String),
    /// `enum`: `"<type>.<value>"`.
    Enum(String),
    /// `long`: a signed 32-bit integer.
    Integer(i32),
    /// `comp`: a 64-bit integer stored as two `u32` halves.
    LargeInteger {
        /// The low 32 bits.
        low: u32,
        /// The high 32 bits.
        high: u32,
    },
    /// `bool`.
    Boolean(bool),
    /// `type` / `GlbC`: a class reference.
    Class {
        /// The class id.
        class_id: String,
        /// The class's unicode display name.
        name: String,
    },
    /// `alis`: an alias blob, decoded Latin-1.
    Alias(String),
    /// `tdta`: raw bytes.
    RawData(Vec<u8>),
    /// `obj `: a reference structure.
    Reference(Vec<RefItem>),
    /// `ObAr`: an object array.
    ObjectArray(Vec<ObjectArrayItem>),
    /// `Pth `: a file path.
    Path {
        /// The path record's four-byte signature.
        sig: String,
        /// The path text.
        path: String,
    },
}

/// One item of an `obj ` reference structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefItem {
    /// `prop`: a property key id.
    Property(String),
    /// `Clss`: a class.
    Class {
        /// The class id.
        class_id: String,
        /// The class's unicode display name.
        name: String,
    },
    /// `Enmr`: `"<type>.<value>"`.
    Enumerated(String),
    /// `rele`: an offset.
    Offset(u32),
    /// `Idnt`: an identifier.
    Identifier(i32),
    /// `indx`: an index.
    Index(i32),
    /// `name`: a unicode name.
    Name(String),
}

/// One item of an `ObAr` object array: a type id and its value run.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectArrayItem {
    /// The item's type id (e.g. `Hrzn` / `Vrtc`).
    pub type_id: String,
    /// The item's values.
    pub values: Vec<f64>,
}

impl Descriptor {
    /// The first value stored under `key`, if any.
    pub fn get(&self, key: &str) -> Option<&DescValue> {
        self.items
            .iter()
            .find(|(item_key, _)| item_key == key)
            .map(|(_, value)| value)
    }
}

/// Reads a versioned descriptor (`u32` version 16 + descriptor structure)
/// from the start of `bytes`. Trailing bytes after the structure are
/// ignored (sections are length-delimited by their `8BIM` header).
///
/// # Errors
///
/// [`AbrError::UnsupportedDescriptorVersion`] for a version other than 16,
/// [`AbrError::UnsupportedDescriptorType`] for an unknown OSType,
/// [`AbrError::InvalidDescriptorUnits`] for an unknown unit code,
/// [`AbrError::DescriptorTooLarge`] for a cap breach and
/// [`AbrError::MalformedDescriptor`] for truncated or invalid data.
pub fn read_version_and_descriptor(bytes: &[u8]) -> Result<Descriptor, AbrError> {
    let mut cursor = Cursor::new(bytes);
    let version = cursor.u32()?;
    if version != DESCRIPTOR_VERSION {
        return Err(AbrError::UnsupportedDescriptorVersion(version));
    }
    read_descriptor_structure(&mut cursor)
}

/// The first value stored under `key` in `desc`.
pub fn dget<'a>(desc: &'a Descriptor, key: &str) -> Option<&'a DescValue> {
    desc.get(key)
}

/// The nested descriptor, when `value` is one.
pub fn as_descriptor(value: Option<&DescValue>) -> Option<&Descriptor> {
    match value {
        Some(DescValue::Descriptor(desc)) => Some(desc),
        _ => None,
    }
}

/// `(units, value)` when `value` is a unit double.
pub fn as_units(value: Option<&DescValue>) -> Option<(&'static str, f64)> {
    match value {
        Some(DescValue::UnitDouble { units, value }) => Some((*units, *value)),
        _ => None,
    }
}

/// The numeric value of an integer, double, unit double or large integer;
/// 0.0 for anything else (the reference's default).
pub fn as_number(value: Option<&DescValue>) -> f64 {
    match value {
        Some(DescValue::Integer(number)) => f64::from(*number),
        Some(DescValue::Double(number)) => *number,
        Some(DescValue::UnitDouble { value, .. }) => *value,
        Some(DescValue::LargeInteger { low, high }) => {
            ((u64::from(*high) << 32) | u64::from(*low)) as f64
        }
        _ => 0.0,
    }
}

/// True only for a present `bool` true (the reference's default is false).
pub fn as_bool(value: Option<&DescValue>) -> bool {
    matches!(value, Some(DescValue::Boolean(true)))
}

/// The text of a `TEXT` or `enum` value.
pub fn as_text(value: Option<&DescValue>) -> Option<&str> {
    match value {
        Some(DescValue::Text(text) | DescValue::Enum(text)) => Some(text.as_str()),
        _ => None,
    }
}

/// A `Percent` value as a fraction (value / 100); missing → 1.0.
///
/// # Errors
///
/// [`AbrError::InvalidDescriptorUnits`] when the value has other units.
pub fn parse_percent(value: Option<&DescValue>) -> Result<f64, AbrError> {
    match as_units(value) {
        None => Ok(1.0),
        Some(("Percent", number)) => Ok(number / 100.0),
        Some((units, _)) => Err(AbrError::InvalidDescriptorUnits(units.to_string())),
    }
}

/// An `Angle` value in degrees; missing → 0.0.
///
/// # Errors
///
/// [`AbrError::InvalidDescriptorUnits`] when the value has other units.
pub fn parse_angle(value: Option<&DescValue>) -> Result<f64, AbrError> {
    match as_units(value) {
        None => Ok(0.0),
        Some(("Angle", number)) => Ok(number),
        Some((units, _)) => Err(AbrError::InvalidDescriptorUnits(units.to_string())),
    }
}

/// A unit double whose units must be `expected`; no default.
///
/// # Errors
///
/// [`AbrError::InvalidDescriptorUnits`] for other units,
/// [`AbrError::MalformedDescriptor`] when the value is missing.
pub fn parse_units_to_number(value: Option<&DescValue>, expected: &str) -> Result<f64, AbrError> {
    match as_units(value) {
        Some((units, number)) if units == expected => Ok(number),
        Some((units, _)) => Err(AbrError::InvalidDescriptorUnits(units.to_string())),
        None => Err(malformed(format!("missing a value in {expected} units"))),
    }
}

/// The unit name for a four-byte unit code.
fn units_name(code: &str) -> Option<&'static str> {
    UNITS_MAP
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, name)| *name)
}

fn malformed(detail: impl Into<String>) -> AbrError {
    AbrError::MalformedDescriptor {
        detail: detail.into(),
    }
}

fn too_large(what: &'static str, count: u64, max: usize) -> AbrError {
    AbrError::DescriptorTooLarge {
        what,
        count,
        max: max as u64,
    }
}

/// Validates a stored count/length against its cap BEFORE anything is
/// allocated from it. Negative counts are malformed.
fn checked_count(raw: i64, max: usize, what: &'static str) -> Result<usize, AbrError> {
    let count = u64::try_from(raw).map_err(|_| malformed(format!("negative {what}")))?;
    if count > max as u64 {
        return Err(too_large(what, count, max));
    }
    usize::try_from(count).map_err(|_| malformed(format!("{what} does not fit in memory")))
}

/// Decodes bytes as Latin-1 (each byte → the code point of the same value).
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| byte as char).collect()
}

fn read_descriptor_structure(cursor: &mut Cursor<'_>) -> Result<Descriptor, AbrError> {
    let (class_id, name) = read_class_structure(cursor)?;
    let count = checked_count(i64::from(cursor.u32()?), MAX_DESC_ITEMS, "descriptor items")?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let key = read_ascii_or_class_id(cursor)?;
        let sig = cursor.sig()?;
        let value = read_os_type(cursor, &sig)?;
        items.push((key, value));
    }
    Ok(Descriptor {
        class_id,
        name,
        items,
    })
}

/// A class structure: `(class_id, name)`.
fn read_class_structure(cursor: &mut Cursor<'_>) -> Result<(String, String), AbrError> {
    let name = read_unicode_string(cursor)?;
    let class_id = read_ascii_or_class_id(cursor)?;
    Ok((class_id, name))
}

fn read_unicode_string(cursor: &mut Cursor<'_>) -> Result<String, AbrError> {
    let units = checked_count(i64::from(cursor.u32()?), MAX_TEXT_CHARS, "text")?;
    cursor.utf16(units, Endian::Big)
}

fn read_ascii_or_class_id(cursor: &mut Cursor<'_>) -> Result<String, AbrError> {
    let length = checked_count(i64::from(cursor.i32()?), MAX_TEXT_CHARS, "text")?;
    let length = if length == 0 { 4 } else { length };
    Ok(latin1(cursor.take(length)?))
}

fn read_reference(cursor: &mut Cursor<'_>) -> Result<Vec<RefItem>, AbrError> {
    let count = checked_count(
        i64::from(cursor.i32()?),
        MAX_REFERENCE_ITEMS,
        "reference items",
    )?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let sig = cursor.sig()?;
        let item = match sig.as_str() {
            "prop" => {
                let _ = read_class_structure(cursor)?;
                RefItem::Property(read_ascii_or_class_id(cursor)?)
            }
            "Clss" => {
                let (class_id, name) = read_class_structure(cursor)?;
                RefItem::Class { class_id, name }
            }
            "Enmr" => {
                let _ = read_class_structure(cursor)?;
                let type_id = read_ascii_or_class_id(cursor)?;
                let value = read_ascii_or_class_id(cursor)?;
                RefItem::Enumerated(format!("{type_id}.{value}"))
            }
            "rele" => {
                let _ = read_class_structure(cursor)?;
                RefItem::Offset(cursor.u32()?)
            }
            "Idnt" => RefItem::Identifier(cursor.i32()?),
            "indx" => RefItem::Index(cursor.i32()?),
            "name" => {
                let _ = read_class_structure(cursor)?;
                RefItem::Name(read_unicode_string(cursor)?)
            }
            other => return Err(AbrError::UnsupportedDescriptorType(other.to_string())),
        };
        items.push(item);
    }
    Ok(items)
}

fn read_units(cursor: &mut Cursor<'_>) -> Result<&'static str, AbrError> {
    let code = cursor.sig()?;
    units_name(&code).ok_or(AbrError::InvalidDescriptorUnits(code))
}

fn read_os_type(cursor: &mut Cursor<'_>, sig: &str) -> Result<DescValue, AbrError> {
    cursor.count_node()?;
    let value = match sig {
        "obj " => DescValue::Reference(read_reference(cursor)?),
        "Objc" | "GlbO" => {
            cursor.enter()?;
            let desc = read_descriptor_structure(cursor)?;
            cursor.leave();
            DescValue::Descriptor(desc)
        }
        "VlLs" => {
            let count = checked_count(i64::from(cursor.i32()?), MAX_LIST_ITEMS, "list items")?;
            cursor.enter()?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                let item_sig = cursor.sig()?;
                items.push(read_os_type(cursor, &item_sig)?);
            }
            cursor.leave();
            DescValue::List(items)
        }
        "doub" => DescValue::Double(cursor.f64()?),
        "UntF" => {
            let units = read_units(cursor)?;
            DescValue::UnitDouble {
                units,
                value: cursor.f64()?,
            }
        }
        "UnFl" => {
            let units = read_units(cursor)?;
            DescValue::UnitDouble {
                units,
                value: f64::from(cursor.f32()?),
            }
        }
        "TEXT" => DescValue::Text(read_unicode_string(cursor)?),
        "enum" => {
            let type_id = read_ascii_or_class_id(cursor)?;
            let value = read_ascii_or_class_id(cursor)?;
            DescValue::Enum(format!("{type_id}.{value}"))
        }
        "long" => DescValue::Integer(cursor.i32()?),
        "comp" => {
            let low = cursor.u32()?;
            let high = cursor.u32()?;
            DescValue::LargeInteger { low, high }
        }
        "bool" => DescValue::Boolean(cursor.u8()? != 0),
        "type" | "GlbC" => {
            let (class_id, name) = read_class_structure(cursor)?;
            DescValue::Class { class_id, name }
        }
        "alis" => {
            let length = checked_count(i64::from(cursor.i32()?), MAX_RAW_BYTES, "alias bytes")?;
            DescValue::Alias(latin1(cursor.take(length)?))
        }
        "tdta" => {
            let length = checked_count(i64::from(cursor.i32()?), MAX_RAW_BYTES, "raw bytes")?;
            DescValue::RawData(cursor.take(length)?.to_vec())
        }
        "ObAr" => {
            let _version = cursor.i32()?;
            let _name = read_unicode_string(cursor)?;
            let _type_id = read_ascii_or_class_id(cursor)?;
            let count = checked_count(
                i64::from(cursor.i32()?),
                MAX_LIST_ITEMS,
                "object array items",
            )?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                let type_id = read_ascii_or_class_id(cursor)?;
                let _unfl = cursor.sig()?;
                let _units = cursor.sig()?;
                let value_count = checked_count(
                    i64::from(cursor.i32()?),
                    MAX_LIST_ITEMS,
                    "object array values",
                )?;
                let mut values = Vec::with_capacity(value_count);
                for _ in 0..value_count {
                    values.push(cursor.f64()?);
                }
                items.push(ObjectArrayItem { type_id, values });
            }
            DescValue::ObjectArray(items)
        }
        "Pth " => {
            let _length = cursor.i32()?;
            let path_sig = cursor.sig()?;
            let _path_size = cursor.i32_le()?;
            let units = checked_count(i64::from(cursor.i32_le()?), MAX_TEXT_CHARS, "text")?;
            let path = cursor.utf16(units, Endian::Little)?;
            DescValue::Path {
                sig: path_sig,
                path,
            }
        }
        other => return Err(AbrError::UnsupportedDescriptorType(other.to_string())),
    };
    Ok(value)
}

/// Byte order of a UTF-16 run.
#[derive(Clone, Copy)]
enum Endian {
    Big,
    Little,
}

/// A bounds-checked cursor over one descriptor payload, carrying the
/// nesting depth and the decoded-node budget.
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    depth: usize,
    nodes: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            depth: 0,
            nodes: 0,
        }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], AbrError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| malformed("length overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| malformed("unexpected end of descriptor"))?;
        self.offset = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], AbrError> {
        let slice = self.take(N)?;
        <[u8; N]>::try_from(slice).map_err(|_| malformed("unexpected end of descriptor"))
    }

    fn u8(&mut self) -> Result<u8, AbrError> {
        let [byte] = self.array::<1>()?;
        Ok(byte)
    }

    fn u32(&mut self) -> Result<u32, AbrError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32, AbrError> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    fn i32_le(&mut self) -> Result<i32, AbrError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32, AbrError> {
        Ok(f32::from_be_bytes(self.array()?))
    }

    fn f64(&mut self) -> Result<f64, AbrError> {
        Ok(f64::from_be_bytes(self.array()?))
    }

    /// A four-byte signature, decoded Latin-1.
    fn sig(&mut self) -> Result<String, AbrError> {
        Ok(latin1(self.take(4)?))
    }

    /// `units` UTF-16 code units (already capped by the caller); a
    /// trailing NUL unit is dropped.
    fn utf16(&mut self, units: usize, endian: Endian) -> Result<String, AbrError> {
        let byte_len = units
            .checked_mul(2)
            .ok_or_else(|| malformed("text length overflow"))?;
        let bytes = self.take(byte_len)?;
        let mut code_units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| match (endian, pair) {
                (Endian::Big, [high, low]) => u16::from_be_bytes([*high, *low]),
                (Endian::Little, [low, high]) => u16::from_le_bytes([*low, *high]),
                _ => 0,
            })
            .collect();
        if code_units.last() == Some(&0) {
            code_units.pop();
        }
        Ok(String::from_utf16_lossy(&code_units))
    }

    fn enter(&mut self) -> Result<(), AbrError> {
        self.depth += 1;
        if self.depth > MAX_DESC_DEPTH {
            return Err(too_large(
                "nesting depth",
                self.depth as u64,
                MAX_DESC_DEPTH,
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn count_node(&mut self) -> Result<(), AbrError> {
        self.nodes += 1;
        if self.nodes > MAX_DESC_NODES {
            return Err(too_large("value count", self.nodes as u64, MAX_DESC_NODES));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Version 16 + class structure (empty name, class id `null`) + the
    /// given item count; the caller appends the items.
    fn header(item_count: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&16u32.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes()); // unicode name: 0 units
        bytes.extend_from_slice(&0i32.to_be_bytes()); // class id follows
        bytes.extend_from_slice(b"null");
        bytes.extend_from_slice(&item_count.to_be_bytes());
        bytes
    }

    fn key(bytes: &mut Vec<u8>, key: &[u8; 4]) {
        bytes.extend_from_slice(&0i32.to_be_bytes());
        bytes.extend_from_slice(key);
    }

    #[test]
    fn missing_percent_defaults_to_one_and_angle_to_zero() {
        assert_eq!(parse_percent(None), Ok(1.0));
        assert_eq!(parse_angle(None), Ok(0.0));
        let wrong = DescValue::UnitDouble {
            units: "Pixels",
            value: 3.0,
        };
        assert_eq!(
            parse_percent(Some(&wrong)),
            Err(AbrError::InvalidDescriptorUnits("Pixels".to_string()))
        );
        assert_eq!(parse_units_to_number(Some(&wrong), "Pixels"), Ok(3.0));
    }

    #[test]
    fn text_drops_only_the_trailing_nul() {
        let mut bytes = header(1);
        key(&mut bytes, b"Nm  ");
        bytes.extend_from_slice(b"TEXT");
        bytes.extend_from_slice(&3u32.to_be_bytes());
        for unit in [0x0048u16, 0x00E9, 0x0000] {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        let desc = read_version_and_descriptor(&bytes).expect("text fixture parses");
        assert_eq!(desc.class_id, "null");
        assert_eq!(as_text(dget(&desc, "Nm  ")), Some("H\u{e9}"));
    }

    #[test]
    fn unfl_widens_and_maps_units() {
        let mut bytes = header(1);
        key(&mut bytes, b"Dmtr");
        bytes.extend_from_slice(b"UnFl");
        bytes.extend_from_slice(b"#Pxl");
        bytes.extend_from_slice(&2.5f32.to_be_bytes());
        let desc = read_version_and_descriptor(&bytes).expect("UnFl fixture parses");
        assert_eq!(as_units(dget(&desc, "Dmtr")), Some(("Pixels", 2.5)));
    }

    #[test]
    fn negative_list_length_is_malformed() {
        let mut bytes = header(1);
        key(&mut bytes, b"Brsh");
        bytes.extend_from_slice(b"VlLs");
        bytes.extend_from_slice(&(-1i32).to_be_bytes());
        let error = read_version_and_descriptor(&bytes).unwrap_err();
        assert!(
            matches!(error, AbrError::MalformedDescriptor { .. }),
            "expected MalformedDescriptor, got {error:?}"
        );
    }

    #[test]
    fn oversized_item_count_is_refused_before_allocating() {
        let bytes = header(u32::MAX);
        assert_eq!(
            read_version_and_descriptor(&bytes).unwrap_err(),
            AbrError::DescriptorTooLarge {
                what: "descriptor items",
                count: u64::from(u32::MAX),
                max: MAX_DESC_ITEMS as u64,
            }
        );
    }
}
