//! Test-only synthetic HDF5 writer.
//!
//! Writes byte layouts straight from the research doc's grids so round-trip
//! tests pin exact values. This module is `#[cfg(test)]` only: the crate
//! ships no write-shaped API (import-only FOREVER).
//!
//! Every layout mirrors `hdf5.rs` exactly (superblock v0/v2, v1/v2 object
//! headers, `OCHK`, the §6.3 messages, v1 B-trees, `SNOD`, local `HEAP`).
//! Real-file variance stays corpus-unverified (§8) on both sides.

use std::io::Write;
use std::path::PathBuf;

/// Message-type numbers (mirror `hdf5.rs`).
pub const M_DATASPACE: u16 = 1;
pub const M_LINK_INFO: u16 = 2;
pub const M_DATATYPE: u16 = 3;
pub const M_LINK: u16 = 6;
pub const M_LAYOUT: u16 = 8;
pub const M_FILTER: u16 = 11;
pub const M_ATTRIBUTE: u16 = 12;
pub const M_CONTINUATION: u16 = 16;
pub const M_SYMBOL_TABLE: u16 = 17;

/// Filter identifiers (mirror `hdf5.rs`).
pub const F_DEFLATE: u16 = 1;
pub const F_SHUFFLE: u16 = 2;
pub const F_FLETCHER32: u16 = 3;

/// Byte writer with forward patching.
#[derive(Debug, Default)]
pub struct B {
    /// File image under construction.
    pub buf: Vec<u8>,
}

impl B {
    /// Empty file image.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current write address.
    pub fn pos(&self) -> u64 {
        self.buf.len() as u64
    }

    /// Append raw bytes, returning their address.
    pub fn bytes(&mut self, data: &[u8]) -> u64 {
        let at = self.pos();
        self.buf.extend_from_slice(data);
        at
    }

    /// Append one byte.
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    /// Append a LE u16.
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Append a LE u32.
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Append a LE u64.
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Reserve zero bytes, returning their address.
    pub fn zeros(&mut self, n: usize) -> u64 {
        let at = self.pos();
        self.buf.extend(std::iter::repeat_n(0, n));
        at
    }

    /// Patch a LE u16 at an address.
    pub fn patch_u16(&mut self, at: u64, v: u16) {
        self.buf[at as usize..at as usize + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Patch a LE u32 at an address.
    pub fn patch_u32(&mut self, at: u64, v: u32) {
        self.buf[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Patch a LE u64 at an address.
    pub fn patch_u64(&mut self, at: u64, v: u64) {
        self.buf[at as usize..at as usize + 8].copy_from_slice(&v.to_le_bytes());
    }

    /// Superblock v0 slot (72 bytes) with zeroed root addresses; returns
    /// (slot base, btree patch site, heap patch site). The slot may sit at
    /// any 512-multiple (the signature probe set), not just offset 0.
    pub fn superblock_v0_slot(&mut self) -> (u64, u64, u64) {
        debug_assert_eq!(self.pos() % 512, 0);
        let base = self.pos();
        self.bytes(&[137, 72, 68, 70, 13, 10, 26, 10]);
        self.u8(0); // version
        self.u8(0); // free-space version
        self.u8(0); // root-group version
        self.u8(0); // reserved
        self.u8(0); // shared-header version
        self.u8(8); // size of offsets
        self.u8(8); // size of lengths
        self.u8(0); // reserved
        self.u16(4); // group leaf K
        self.u16(16); // group internal K
        self.u32(0); // consistency flags
        self.u64(0); // base address
        self.u64(u64::MAX); // extension (undefined)
        self.u64(0); // end of file (patched by finish)
        self.u64(u64::MAX); // driver info (undefined)
                            // The root symbol-table entry (spec): link name offset (0),
                            // object header address, cache type 1, reserved, and the
                            // 16-byte scratch pad carrying b-tree + local heap (the
                            // synthetic root has no separate object header; the walker
                            // reads cache type 1 and uses the scratch pad).
        self.u64(0); // link name offset
        self.u64(0); // root object header (unused by cache type 1)
        self.u32(1); // cache type: symbol table
        self.u32(0); // reserved
        let btree_at = self.pos();
        self.u64(0);
        let heap_at = self.pos();
        self.u64(0);
        (base, btree_at, heap_at)
    }

    /// Superblock v2 slot (52 bytes); returns (slot base, root patch site).
    pub fn superblock_v2_slot(&mut self) -> (u64, u64) {
        debug_assert_eq!(self.pos() % 512, 0);
        let base = self.pos();
        self.bytes(&[137, 72, 68, 70, 13, 10, 26, 10]);
        self.u8(2); // version
        self.u8(8); // size of offsets
        self.u8(8); // size of lengths
        self.u8(0); // consistency flags
        self.u64(0); // base address
        self.u64(u64::MAX); // extension (undefined)
        self.u64(0); // end of file (patched by finish)
        let root_at = self.pos();
        self.u64(0);
        self.u64(u64::MAX); // superblock extension (undefined)
        (base, root_at)
    }

    /// Patch the end-of-file address (v0 slot: offset 40; v2 slot: 28).
    pub fn finish(&mut self, eof_at: u64) {
        let end = self.pos();
        self.patch_u64(eof_at, end);
    }

    /// v1 object header from (type, payload) messages.
    pub fn ohdr_v1(&mut self, messages: &[(u16, Vec<u8>)]) -> u64 {
        let at = self.pos();
        let mut area: Vec<u8> = Vec::new();
        for (mtype, payload) in messages {
            area.extend_from_slice(&mtype.to_le_bytes());
            area.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            area.push(0); // flags
            area.extend_from_slice(&[0, 0, 0]); // reserved
            area.extend_from_slice(payload);
            while area.len() % 8 != 0 {
                area.push(0);
            }
        }
        self.u8(1); // version
        self.u8(0); // reserved
        self.u16(messages.len() as u16);
        self.u32(1); // reference count
        self.u32(area.len() as u32);
        self.u32(0); // reserved
        self.bytes(&area);
        at
    }

    /// v2 object header from (type, payload) messages (chunk-size code 1).
    pub fn ohdr_v2(&mut self, messages: &[(u8, Vec<u8>)]) -> u64 {
        let at = self.pos();
        let mut area: Vec<u8> = Vec::new();
        for (mtype, payload) in messages {
            area.push(*mtype);
            area.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            area.push(0); // flags
            area.extend_from_slice(payload);
        }
        self.bytes(b"OHDR");
        self.u8(2); // version
        self.u8(0x01); // flags: chunk-size code 1
        self.bytes(&[0, 0]); // reserved
        self.u32(area.len() as u32);
        self.bytes(&area);
        at
    }

    /// `OCHK` continuation chunk holding v2 messages.
    pub fn ochk(&mut self, messages: &[(u8, Vec<u8>)]) -> u64 {
        let at = self.pos();
        let mut area: Vec<u8> = Vec::new();
        for (mtype, payload) in messages {
            area.push(*mtype);
            area.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            area.push(0);
            area.extend_from_slice(payload);
        }
        self.bytes(b"OCHK");
        self.u32(area.len() as u32);
        self.bytes(&area);
        at
    }

    /// Local heap block; returns (block address, name offsets in order).
    pub fn heap(&mut self, names: &[&str]) -> (u64, Vec<u64>) {
        let mut segment: Vec<u8> = Vec::new();
        let mut offsets = Vec::with_capacity(names.len());
        for name in names {
            offsets.push(segment.len() as u64);
            segment.extend_from_slice(name.as_bytes());
            segment.push(0);
        }
        let seg_addr = self.bytes(&segment);
        let at = self.pos();
        self.bytes(b"HEAP");
        self.u8(0); // version
        self.bytes(&[0, 0, 0]); // reserved
        self.u64(segment.len() as u64); // data segment size
        self.u64(0); // free-list head
        self.u64(seg_addr);
        (at, offsets)
    }

    /// `SNOD` node from (heap name offset, header address) entries.
    pub fn snod(&mut self, entries: &[(u32, u64)]) -> u64 {
        let at = self.pos();
        self.bytes(b"SNOD");
        self.u8(1); // version
        self.u8(0); // reserved
        self.u16(entries.len() as u16);
        for (name_off, header) in entries {
            self.u64(u64::from(*name_off)); // link name offset, off-wide
            self.u64(*header);
            self.u32(0); // cache type
            self.u32(0); // reserved
            self.zeros(16); // scratch
        }
        at
    }

    /// Group v1 B-tree from (heap key, child address) entries.
    pub fn btree_group(&mut self, entries: &[(u32, u64)], level: u8) -> u64 {
        let at = self.pos();
        self.bytes(b"TREE");
        self.u8(0); // node type: group
        self.u8(level);
        self.u16(entries.len() as u16);
        self.u64(u64::MAX); // left sibling (undefined)
        self.u64(u64::MAX); // right sibling (undefined)
        for (key, child) in entries {
            self.u64(u64::from(*key)); // off-wide key: the largest heap object size
            self.u64(*child);
        }
        at
    }

    /// Chunk v1 B-tree leaf from (mask, stored size, coords, child) entries.
    pub fn btree_chunk_leaf(&mut self, rank: usize, entries: &[(u32, u64, Vec<u64>, u64)]) -> u64 {
        let at = self.pos();
        self.bytes(b"TREE");
        self.u8(1); // node type: chunk index
        self.u8(0); // level: leaf
        self.u16(entries.len() as u16);
        self.u64(u64::MAX);
        self.u64(u64::MAX);
        for (mask, size, coords, child) in entries {
            debug_assert_eq!(coords.len(), rank);
            self.u32(*size as u32);
            self.u32(*mask);
            // One offset per dimension (the corpus node at 272159007
            // pins the shape); the child follows the last offset.
            for coord in coords {
                self.u64(*coord);
            }
            self.u64(*child);
        }
        at
    }
}

/// v1 dataspace payload: rank, dims, optional max-dims (8-byte sizes).
pub fn dataspace_v1(rank: u8, dims: &[u64], maxdims: Option<&[u64]>) -> Vec<u8> {
    let mut out = vec![
        1,
        rank,
        if maxdims.is_some() { 0x01 } else { 0 },
        0,
        0,
        0,
        0,
        0,
    ];
    for dim in dims {
        out.extend_from_slice(&dim.to_le_bytes());
    }
    if let Some(max) = maxdims {
        for dim in max {
            out.extend_from_slice(&dim.to_le_bytes());
        }
    }
    out
}

/// v2 simple dataspace payload: rank, dims (8-byte sizes). No test
/// exercises v2 DATASPACE messages yet (the v2-header test carries
/// v1 dataspaces); the first v2-dataspace corpus file drives it —
/// named follow-up, API surface for that day.
#[cfg(test)]
#[allow(dead_code)]
pub fn dataspace_v2_simple(rank: u8, dims: &[u64]) -> Vec<u8> {
    let mut out = vec![2, rank, 0, 1];
    for dim in dims {
        out.extend_from_slice(&dim.to_le_bytes());
    }
    out
}

/// Fixed-point datatype payload (LE, offset 0, full precision).
fn fixed_dtype(size: u32, signed: bool) -> Vec<u8> {
    let mut out = vec![0x10, if signed { 0x10 } else { 0x00 }, 0, 0];
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // bit offset
    out.extend_from_slice(&(8 * size as u16).to_le_bytes()); // precision
    out
}

/// IEEE-754 float datatype payload (LE, exact property bytes).
fn float_dtype(size: u32, exp_loc: u16, exp_size: u16, bias: u32) -> Vec<u8> {
    let mant_size = (size * 8 - exp_size as u32 - 1) as u16;
    let mut out = vec![0x11, 0, 0, 0];
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&exp_loc.to_le_bytes());
    out.extend_from_slice(&exp_size.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // mantissa location
    out.extend_from_slice(&mant_size.to_le_bytes());
    out.extend_from_slice(&bias.to_le_bytes());
    out
}

/// Allow-list datatype payloads.
pub fn dtype_u8() -> Vec<u8> {
    fixed_dtype(1, false)
}

/// Allow-list datatype payloads.
pub fn dtype_u16() -> Vec<u8> {
    fixed_dtype(2, false)
}

/// Allow-list datatype payloads.
pub fn dtype_i32() -> Vec<u8> {
    fixed_dtype(4, true)
}

/// Allow-list datatype payloads.
pub fn dtype_f32() -> Vec<u8> {
    float_dtype(4, 23, 8, 127)
}

/// Allow-list datatype payloads.
pub fn dtype_f64() -> Vec<u8> {
    float_dtype(8, 52, 11, 1023)
}

/// Fixed-length string datatype payload.
pub fn dtype_string(len: u32) -> Vec<u8> {
    let mut out = vec![0x13, 0, 0, 0];
    out.extend_from_slice(&len.to_le_bytes());
    out
}

/// Big-endian u16: outside the allow-list (must fail typed).
pub fn dtype_be_u16() -> Vec<u8> {
    let mut out = vec![0x10, 0x01, 0, 0];
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out
}

/// Compound datatype: outside the allow-list (must fail typed).
pub fn dtype_compound() -> Vec<u8> {
    let mut out = vec![0x16, 0, 0, 0];
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out
}

/// Compact layout payload with inline data.
pub fn layout_compact(data: &[u8]) -> Vec<u8> {
    let mut out = vec![3, 0];
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Contiguous layout payload.
pub fn layout_contiguous(addr: u64, size: u64) -> Vec<u8> {
    let mut out = vec![3, 1];
    out.extend_from_slice(&addr.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out
}

/// Chunked layout payload: 1-byte dimensionality, B-tree, u32 dims, elem.
pub fn layout_chunked(rank: u8, btree: u64, chunk_dims: &[u32], elem: u32) -> Vec<u8> {
    let mut out = vec![3, 2, rank];
    out.extend_from_slice(&btree.to_le_bytes());
    for dim in chunk_dims {
        out.extend_from_slice(&dim.to_le_bytes());
    }
    out.extend_from_slice(&elem.to_le_bytes());
    out
}

/// Filter pipeline payload: (id, client u32s) entries.
pub fn pipeline(version: u8, filters: &[(u16, &[u32])]) -> Vec<u8> {
    let mut out = vec![version, filters.len() as u8, 0, 0];
    for (id, client) in filters {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // name length
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&(client.len() as u16).to_le_bytes());
        if version == 1 {
            while out.len() % 8 != 0 {
                out.push(0);
            }
        }
        for value in *client {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// Hard-link payload (2-byte name length, hard target).
pub fn link_hard(name: &str, target: u64) -> Vec<u8> {
    let mut out = vec![1, 0x01 | 0x08, 0];
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&target.to_le_bytes());
    out
}

/// Soft-link payload.
pub fn link_soft(name: &str, target: &str) -> Vec<u8> {
    let mut out = vec![1, 0x01 | 0x08, 1];
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&(target.len() as u16).to_le_bytes());
    out.extend_from_slice(target.as_bytes());
    out
}

/// External-link payload (reported, not followed).
pub fn link_external(name: &str) -> Vec<u8> {
    let mut out = vec![1, 0x01 | 0x08, 64];
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

/// Link-info payload: compact (heap undefined) or dense (heap defined).
pub fn link_info(heap_addr: Option<u64>) -> Vec<u8> {
    let mut out = vec![0, 0];
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&heap_addr.unwrap_or(u64::MAX).to_le_bytes());
    out.extend_from_slice(&u64::MAX.to_le_bytes());
    out
}

/// Symbol-table message payload.
pub fn symtab_msg(btree: u64, heap: u64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&btree.to_le_bytes());
    out.extend_from_slice(&heap.to_le_bytes());
    out
}

/// Attribute message payload (name only matters: counted, not parsed).
pub fn attr_msg(name: &str) -> Vec<u8> {
    let mut out = vec![1, 0];
    out.extend_from_slice(&(name.len() as u16 + 1).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // datatype size placeholder
    out.extend_from_slice(&1u16.to_le_bytes()); // dataspace size placeholder
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    while out.len() % 8 != 0 {
        out.push(0);
    }
    out.extend_from_slice(&[0xAA, 0xBB]); // opaque datatype/dataspace/data
    out
}

/// `projectsettings.ini` payload for the §2.1 byte shape: each present
/// field is UTF-16LE bytes, a 2-byte terminator, a 5-byte gap, one value.
pub fn stamp_payload(
    creation_major: Option<u8>,
    creation_minor: Option<u8>,
    save_major: Option<u8>,
    save_minor: Option<u8>,
) -> Vec<u8> {
    let mut out = b"PainterProject\x00\xff".to_vec();
    let mut field = |name: &str, value: u8| {
        for unit in name.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]); // terminator (+2)
        out.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55]); // +5 gap
        out.push(value);
    };
    if let Some(major) = creation_major {
        field(".versionAtCreation/major", major);
    }
    if let Some(minor) = creation_minor {
        field(".versionAtCreation/minor", minor);
    }
    if let Some(major) = save_major {
        field(".versionAtLastSave/major", major);
    }
    if let Some(minor) = save_minor {
        field(".versionAtLastSave/minor", minor);
    }
    out.extend_from_slice(b"tail\x00");
    out
}

/// HBO newer-binary payload (marker + object count).
pub fn hbo_newer_payload() -> Vec<u8> {
    let mut out = vec![0x11, 0x0B, 0x00, 0x69, 0x03, 0, 0, 0];
    out.extend(std::iter::repeat_n(0x5A, 24));
    out
}

/// HBO 12-byte-header payload with a `version_check` era.
pub fn hbo_tagged_payload(version_check: u32) -> Vec<u8> {
    let mut out = vec![0xDD, 0x2F, 0x7C, 0x1B];
    out.extend_from_slice(&version_check.to_le_bytes());
    out.extend_from_slice(&81u32.to_le_bytes());
    out.extend(std::iter::repeat_n(0xA5, 32));
    out
}

/// Byte-shuffle `data` for elements of `elem` bytes (encoder direction).
pub fn shuffle_bytes(data: &[u8], elem: usize) -> Vec<u8> {
    debug_assert_eq!(data.len() % elem, 0);
    let count = data.len() / elem;
    let mut out = Vec::with_capacity(data.len());
    for j in 0..elem {
        for k in 0..count {
            out.push(data[k * elem + j]);
        }
    }
    out
}

/// Raw-deflate `data` (no zlib header: the HDF5 filter stream).
pub fn deflate_bytes(data: &[u8]) -> Vec<u8> {
    // The stream encoder (not Compress::compress_vec, whose zero-capacity
    // output Vec makes Finish return BufError — the status the failing
    // tests pinned).
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).expect("test deflate write");
    enc.finish().expect("test deflate finish")
}

/// Write a finished image to a unique scratch file for [`crate::open`].
pub fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "umber-spp-{name}-{}-{}.spp",
        std::process::id(),
        bytes.len()
    ));
    std::fs::write(&path, bytes).expect("test scratch write must succeed");
    path
}
