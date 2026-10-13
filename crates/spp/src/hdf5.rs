//! HDF5-subset byte reader (research doc §6.1–§6.6).
//!
//! Every structural read is bounds-checked; every message walk is
//! total-size-checked. Out-of-subset content becomes [`SppError::Unsupported`]
//! or a loud [`Tree`] note — never a panic, never silent.
//!
//! ## Judgment calls (v1 boundaries, corpus-unverified until §8)
//!
//! - Message-type numbers (1 dataspace, 2 link-info, 3 datatype, 6 link,
//!   8 layout, 11 filter pipeline, 12 attribute, 16 continuation, 17 symbol
//!   table) are best-effort recall of the HDF5 message table; the first
//!   corpus file must re-pin them.
//! - Superblock field offsets follow the spec's byte grid: v0 addresses at
//!   base+24, v1 at base+28 (v1 adds the indexed-storage K), v2/v3 root
//!   header address after base/extension/EOF. Offset/length widths accept
//!   2, 4 or 8 bytes; anything else is corrupt.
//! - v1 object-header `header_size` is the message-area byte count; trailing
//!   alignment padding is accepted, overruns are corrupt.
//! - v2 object headers carry a `u32` message-area size after two reserved
//!   bytes; only chunk-size code 1 (2-byte message sizes) is supported.
//!   `OCHK` continuation chunks carry a `u32` size the same way. A
//!   continuation target is dispatched by peeking `OCHK` versus a v1 header.
//! - Link-info density is decided by the heap address: defined means the
//!   group is fractal-heap resident and is reported, never traversed (§6.5).
//! - Chunk B-tree keys are exactly `u32` byte-size, `u32` filter-mask,
//!   `(D+1)` `u64` offsets (§6.2/§6.4); the first D offsets are the chunk
//!   coordinates. Missing chunks read as zeros (HDF5 fill semantics).
//! - Attribute messages are counted, not parsed; only the name bytes are
//!   read, to record `m3_x64_128` presence (§7-U7). Values never decode.
//! - Optional filters (flag bit 0) that are unknown are skipped loudly;
//!   required unknown filters are `Unsupported`.
//! - Cycles and nesting deeper than 64 are cut with a loud typed outcome,
//!   so hostile files cannot recurse forever.

use crate::{GroupKind, SppError};
use std::collections::{HashMap, HashSet};

/// Cap on a single decoded dataset payload (corrupt geometry must not OOM).
pub const MAX_DATASET_BYTES: u64 = 256 * 1024 * 1024;
/// Recursion cap for group nesting.
const MAX_DEPTH: usize = 64;
/// HDF5 file signature (`\211 H D F \r \n \032 \n`).
const SIGNATURE: [u8; 8] = [137, 72, 68, 70, 13, 10, 26, 10];
/// Offsets probed for the signature (first 8 doublings, §1.3).
const PROBE_OFFSETS: [u64; 8] = [0, 512, 1024, 2048, 4096, 8192, 16384, 32768];

/// Message-type numbers (best-effort recall, see module docs).
const MSG_DATASPACE: u16 = 1;
const MSG_LINK_INFO: u16 = 2;
const MSG_DATATYPE: u16 = 3;
const MSG_LINK: u16 = 6;
const MSG_LAYOUT: u16 = 8;
const MSG_FILTER: u16 = 11;
const MSG_ATTRIBUTE: u16 = 12;
const MSG_CONTINUATION: u16 = 16;
const MSG_SYMBOL_TABLE: u16 = 17;

/// Layout classes (§6.3): compact / contiguous / chunked; virtual is out.
const LAYOUT_COMPACT: u8 = 0;
const LAYOUT_CONTIGUOUS: u8 = 1;
const LAYOUT_CHUNKED: u8 = 2;

/// Filter identifiers (§6.3): deflate / shuffle / fletcher32 only.
const FILTER_DEFLATE: u16 = 1;
const FILTER_SHUFFLE: u16 = 2;
const FILTER_FLETCHER32: u16 = 3;

/// Newer v11+ HBO marker (§5.1).
pub const HBO_MARKER_V11: u32 = 0x6900_0B11;
/// HBO stream magic (§5.2).
pub const HBO_MAGIC: u32 = 0x1B7C_2FDD;

/// Whole-file parse result: raw datasets, group lines, loud notes.
#[derive(Debug, Default)]
pub struct Tree {
    /// Decoded datasets in traversal order.
    pub datasets: Vec<RawDataset>,
    /// Group lines in traversal order (root first).
    pub groups: Vec<RawGroup>,
    /// Loud skip lines (unknown messages, unresolvable soft links, …).
    pub notes: Vec<String>,
}

/// One decoded dataset: element bytes plus the §4.3 report facts.
#[derive(Debug, Clone)]
pub struct RawDataset {
    /// Full path inside the container.
    pub path: String,
    /// Dimension sizes (empty for scalar/null-adjacent shapes).
    pub shape: Vec<u64>,
    /// Datatype allow-list member.
    pub dtype: Dtype,
    /// `compact` / `contiguous` / `chunked`.
    pub layout_name: &'static str,
    /// Chunk geometry, when chunked.
    pub chunk_dims: Option<Vec<u64>>,
    /// Filters in pipeline order.
    pub filters: Vec<FilterKind>,
    /// Attribute message count (values never parsed).
    pub attr_count: usize,
    /// Whether an `m3_x64_128` attribute name was seen (presence-only).
    pub has_m3: bool,
    /// Row-major decoded element bytes.
    pub bytes: Vec<u8>,
}

/// Storage family line for one group.
#[derive(Debug, Clone)]
pub struct RawGroup {
    /// Full path (`/` for the root).
    pub path: String,
    /// Observed family.
    pub kind: GroupKind,
}

/// Strict datatype allow-list (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dtype {
    /// Unsigned 8-bit little-endian.
    U8,
    /// Unsigned 16-bit little-endian.
    U16,
    /// Signed 32-bit little-endian.
    I32,
    /// IEEE-754 32-bit little-endian.
    F32,
    /// IEEE-754 64-bit little-endian.
    F64,
    /// Fixed-length byte string of the given length.
    Str(usize),
}

impl Dtype {
    /// Element size in bytes.
    pub fn byte_size(self) -> usize {
        match self {
            Dtype::U8 => 1,
            Dtype::U16 => 2,
            Dtype::I32 | Dtype::F32 => 4,
            Dtype::F64 => 8,
            Dtype::Str(n) => n,
        }
    }

    /// Short report name (`u8`, `u16`, `i32`, `f32`, `f64`, `string`).
    pub fn name(self) -> &'static str {
        match self {
            Dtype::U8 => "u8",
            Dtype::U16 => "u16",
            Dtype::I32 => "i32",
            Dtype::F32 => "f32",
            Dtype::F64 => "f64",
            Dtype::Str(_) => "string",
        }
    }
}

/// Pipeline filters (§6.3): deflate / shuffle / fletcher32 only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// Raw deflate (client data opaque, level ignored).
    Deflate,
    /// Byte shuffle with the layout element size.
    Shuffle,
    /// Recorded, never verified.
    Fletcher32,
}

impl FilterKind {
    /// Short report name.
    pub fn name(self) -> &'static str {
        match self {
            FilterKind::Deflate => "deflate",
            FilterKind::Shuffle => "shuffle",
            FilterKind::Fletcher32 => "fletcher32",
        }
    }
}

/// Parse a whole file image into a [`Tree`].
pub fn parse(data: &[u8]) -> Result<Tree, SppError> {
    let (off_size, len_size, base, root) = parse_superblock(data)?;
    let ctx = Ctx {
        data,
        off_size,
        len_size,
        base,
    };
    let mut walk = Walker {
        ctx,
        visited: HashSet::new(),
        tree: Tree::default(),
    };
    match root {
        Root::SymTable { btree, heap } => {
            walk.traverse_old_group("/", btree, heap, 0)?;
        }
        Root::ObjectHeader(addr) => {
            walk.traverse_ohdr_group("/", addr, 0)?;
        }
    }
    Ok(walk.tree)
}

/// File-width context shared by every structural read.
#[derive(Debug, Clone, Copy)]
struct Ctx<'a> {
    data: &'a [u8],
    off_size: usize,
    len_size: usize,
    base: u64,
}

/// Superblock root: inline symbol-table entry (v0/v1) or header address.
#[derive(Debug, Clone, Copy)]
enum Root {
    SymTable { btree: u64, heap: u64 },
    ObjectHeader(u64),
}

/// Bounds-checked byte slice.
fn bytes_at(data: &[u8], off: u64, len: u64) -> Result<&[u8], SppError> {
    let start: usize = off
        .try_into()
        .map_err(|_| SppError::Corrupt("file offset out of range".into()))?;
    let want: usize = len
        .try_into()
        .map_err(|_| SppError::Corrupt("byte length out of range".into()))?;
    let end = start
        .checked_add(want)
        .ok_or_else(|| SppError::Corrupt("byte range overflows".into()))?;
    if end > data.len() {
        return Err(SppError::Corrupt(format!(
            "out of bounds: [{start}..{end}) of {}",
            data.len()
        )));
    }
    Ok(&data[start..end])
}

/// Little-endian integer of `width` bytes (width in 1, 2, 4 or 8).
fn uint_at(data: &[u8], off: u64, width: usize) -> Result<u64, SppError> {
    let raw = bytes_at(data, off, width as u64)?;
    let mut value: u64 = 0;
    for (i, b) in raw.iter().enumerate() {
        value |= (*b as u64) << (8 * i);
    }
    Ok(value)
}

/// The undefined address: all-FF at the offset width.
fn undefined_address(off_size: usize) -> u64 {
    if off_size >= 8 {
        u64::MAX
    } else {
        (1u64 << (8 * off_size)) - 1
    }
}

/// Resolve a stored address against the base (None = undefined address).
fn resolve(ctx: Ctx<'_>, stored: u64, what: &str) -> Result<Option<u64>, SppError> {
    if stored == undefined_address(ctx.off_size) {
        return Ok(None);
    }
    ctx.base.checked_add(stored).map(Some).ok_or_else(|| {
        let ctx_base = ctx.base;
        SppError::Corrupt(format!("{what} address overflows base {ctx_base}"))
    })
}

/// Parse the superblock: signature search plus v0–v3 roots (§6.1).
fn parse_superblock(data: &[u8]) -> Result<(usize, usize, u64, Root), SppError> {
    let mut start = None;
    for probe in PROBE_OFFSETS {
        let end = probe + 8;
        if end <= data.len() as u64 && data[probe as usize..end as usize] == SIGNATURE {
            start = Some(probe);
            break;
        }
    }
    let base_off = start.ok_or(SppError::NotHdf5)?;
    let version = *data
        .get(base_off as usize + 8)
        .ok_or_else(|| SppError::Corrupt("superblock truncated at version".into()))?;
    match version {
        0 | 1 => parse_superblock_v0_v1(data, base_off, version),
        2 | 3 => parse_superblock_v2_v3(data, base_off),
        other => Err(SppError::Unsupported(format!("superblock version {other}"))),
    }
}

/// Check an offset/length width (2, 4 or 8 bytes) and name the offender.
fn check_width(width: u64, what: &str) -> Result<usize, SppError> {
    match width {
        2 | 4 | 8 => Ok(width as usize),
        other => Err(SppError::Corrupt(format!("bad {what}: {other}"))),
    }
}

/// Superblock v0/v1: inline root symbol-table entry (§6.1).
fn parse_superblock_v0_v1(
    data: &[u8],
    base_off: u64,
    version: u8,
) -> Result<(usize, usize, u64, Root), SppError> {
    let off_size = check_width(uint_at(data, base_off + 13, 1)?, "size of offsets")?;
    let len_size = check_width(uint_at(data, base_off + 14, 1)?, "size of lengths")?;
    // v1 inserts the indexed-storage K (4 bytes) before the addresses.
    let addrs = base_off + if version == 1 { 28 } else { 24 };
    let off = off_size as u64;
    let base = uint_at(data, addrs, off_size)?;
    let btree_at = addrs + 4 * off;
    let heap_at = btree_at + off;
    bytes_at(data, heap_at, off)?;
    let ctx_probe = Ctx {
        data,
        off_size,
        len_size,
        base,
    };
    let btree_stored = uint_at(data, btree_at, off_size)?;
    let heap_stored = uint_at(data, heap_at, off_size)?;
    let btree = resolve(ctx_probe, btree_stored, "root b-tree")?
        .ok_or_else(|| SppError::Corrupt("v0/v1 root without symbol table".into()))?;
    let heap = resolve(ctx_probe, heap_stored, "root heap")?
        .ok_or_else(|| SppError::Corrupt("v0/v1 root without local heap".into()))?;
    Ok((off_size, len_size, base, Root::SymTable { btree, heap }))
}

/// Superblock v2/v3: root-group object-header address (§6.1).
fn parse_superblock_v2_v3(
    data: &[u8],
    base_off: u64,
) -> Result<(usize, usize, u64, Root), SppError> {
    let off_size = check_width(uint_at(data, base_off + 9, 1)?, "size of offsets")?;
    let len_size = check_width(uint_at(data, base_off + 10, 1)?, "size of lengths")?;
    let off = off_size as u64;
    let base = uint_at(data, base_off + 12, off_size)?;
    let root_at = base_off + 12 + 3 * off;
    bytes_at(data, root_at, off)?;
    let ctx_probe = Ctx {
        data,
        off_size,
        len_size,
        base,
    };
    let root_stored = uint_at(data, root_at, off_size)?;
    let root = resolve(ctx_probe, root_stored, "root object header")?
        .ok_or_else(|| SppError::Corrupt("v2/v3 root without object header".into()))?;
    Ok((off_size, len_size, base, Root::ObjectHeader(root)))
}

/// Recursive traversal state.
struct Walker<'a> {
    ctx: Ctx<'a>,
    visited: HashSet<u64>,
    tree: Tree,
}

/// Owned object-header message.
#[derive(Debug)]
enum Message {
    Dataspace(Space),
    /// Null dataspace: a dataset with no elements (skipped + reported).
    NullDataspace,
    Datatype(Dtype),
    Layout(Layout),
    Pipeline(Vec<FilterKind>),
    Link(Link),
    LinkInfo {
        dense: bool,
    },
    SymbolTable {
        btree: u64,
        heap: u64,
    },
    Continuation {
        addr: u64,
        size: u64,
    },
    Attribute {
        name: String,
    },
    Unknown {
        mtype: u16,
    },
}

/// Dataspace: rank plus dimension sizes (scalar = rank 0).
#[derive(Debug, Clone)]
struct Space {
    rank: usize,
    dims: Vec<u64>,
}

/// Data layout (§6.3).
#[derive(Debug)]
enum Layout {
    Compact(Vec<u8>),
    Contiguous { addr: u64, size: u64 },
    Chunked(Chunked),
}

/// Chunked layout facts.
#[derive(Debug)]
struct Chunked {
    rank: usize,
    btree: u64,
    chunk_dims: Vec<u64>,
    elem: usize,
}

/// Group link (§6.3).
#[derive(Debug)]
struct Link {
    name: String,
    target: LinkTarget,
}

/// Link target: hard address, soft path, or reported-not-followed external.
#[derive(Debug)]
enum LinkTarget {
    Hard(u64),
    Soft(String),
    External(String),
}

impl<'a> Walker<'a> {
    /// Child path under a group (`/` stays the root spelling).
    fn child_path(parent: &str, name: &str) -> String {
        if parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        }
    }

    /// Traverse an old-style group: B-tree + `SNOD` + local `HEAP` (§6.3).
    fn traverse_old_group(
        &mut self,
        path: &str,
        btree: u64,
        heap: u64,
        depth: usize,
    ) -> Result<(), SppError> {
        self.check_depth(path, depth)?;
        let heap_data = self.read_heap(heap)?;
        self.tree.groups.push(RawGroup {
            path: path.to_string(),
            kind: GroupKind::OldStyle,
        });
        let mut entries: Vec<(String, u64)> = Vec::new();
        self.walk_group_btree(btree, &heap_data, &mut entries)?;
        for (name, addr) in entries {
            let child = Self::child_path(path, &name);
            self.dispatch_child(&child, addr, depth + 1)?;
        }
        Ok(())
    }

    /// Traverse a group rooted at an object header (§6.2–§6.3).
    fn traverse_ohdr_group(&mut self, path: &str, addr: u64, depth: usize) -> Result<(), SppError> {
        self.check_depth(path, depth)?;
        if !self.visited.insert(addr) {
            let note_path = path.to_string();
            self.tree.notes.push(format!(
                "{note_path}: object header revisited (cycle, skipped)"
            ));
            return Ok(());
        }
        let messages = self.read_header(addr)?;
        // Old-style group carried inline: symbol-table message wins.
        if let Some(sym) = messages.iter().find_map(|m| match m {
            Message::SymbolTable { btree, heap } => Some((*btree, *heap)),
            _ => None,
        }) {
            let heap_data = self.read_heap(sym.1)?;
            self.push_group(path, GroupKind::OldStyle, &messages);
            let mut entries: Vec<(String, u64)> = Vec::new();
            self.walk_group_btree(sym.0, &heap_data, &mut entries)?;
            for (name, child_addr) in entries {
                let child = Self::child_path(path, &name);
                self.dispatch_child(&child, child_addr, depth + 1)?;
            }
            return Ok(());
        }
        // Dense groups report and stop (§6.5): no speculative traversal.
        if messages
            .iter()
            .any(|m| matches!(m, Message::LinkInfo { dense: true }))
        {
            self.tree.groups.push(RawGroup {
                path: path.to_string(),
                kind: GroupKind::Dense,
            });
            return Ok(());
        }
        self.push_group(path, GroupKind::Compact, &messages);
        // Hard children first so soft links resolve against the full map.
        let mut hard: Vec<(String, u64)> = Vec::new();
        let mut soft: Vec<(String, String)> = Vec::new();
        for msg in &messages {
            if let Message::Link(link) = msg {
                match &link.target {
                    LinkTarget::Hard(child_addr) => {
                        let child = Self::child_path(path, &link.name);
                        hard.push((child, *child_addr));
                    }
                    LinkTarget::Soft(target) => {
                        soft.push((link.name.clone(), target.clone()));
                    }
                    LinkTarget::External(name) => {
                        let link_name = name.clone();
                        let group_path = path.to_string();
                        self.tree.notes.push(format!(
                            "{group_path}: external/user-defined link '{link_name}' (not followed)"
                        ));
                    }
                }
            }
        }
        let mut known: HashMap<String, u64> = HashMap::new();
        for (child, child_addr) in &hard {
            known.insert(child.clone(), *child_addr);
        }
        for (child, child_addr) in hard {
            self.dispatch_child(&child, child_addr, depth + 1)?;
        }
        for (name, target) in soft {
            let resolved = resolve_soft(path, &target);
            let group_path = path.to_string();
            if known.contains_key(&resolved) {
                // The target is already imported under its hard path; the
                // alias is a loud line, never a duplicate import.
                self.tree.notes.push(format!(
                    "{group_path}: soft link '{name}' -> '{target}' (same object, not duplicated)"
                ));
            } else {
                self.tree.notes.push(format!(
                    "{group_path}: soft link '{name}' -> '{target}' unresolvable (skipped)"
                ));
            }
        }
        Ok(())
    }

    /// Record a compact/old-style group line with its attribute census.
    fn push_group(&mut self, path: &str, kind: GroupKind, messages: &[Message]) {
        let attrs = messages
            .iter()
            .filter(|m| matches!(m, Message::Attribute { .. }))
            .count();
        let detail = format!("attrs: {attrs}");
        self.tree.groups.push(RawGroup {
            path: path.to_string(),
            kind,
        });
        let group_path = path.to_string();
        self.tree
            .notes
            .push(format!("{group_path}: group ({detail})"));
    }

    /// Dispatch a child object header: dataset when it carries a dataspace.
    fn dispatch_child(&mut self, path: &str, addr: u64, depth: usize) -> Result<(), SppError> {
        if !self.visited.insert(addr) {
            let note_path = path.to_string();
            self.tree.notes.push(format!(
                "{note_path}: object header revisited (cycle, skipped)"
            ));
            return Ok(());
        }
        let messages = self.read_header(addr)?;
        let is_dataset = messages
            .iter()
            .any(|m| matches!(m, Message::Dataspace(_) | Message::NullDataspace));
        if is_dataset {
            self.decode_dataset(path, &messages)?;
        } else {
            // A group reached by address: re-dispatch without re-marking.
            self.visited.remove(&addr);
            self.traverse_ohdr_group(path, addr, depth)?;
        }
        Ok(())
    }

    /// Guard recursion depth (hostile nesting cannot overflow the stack).
    fn check_depth(&self, path: &str, depth: usize) -> Result<(), SppError> {
        if depth > MAX_DEPTH {
            let group_path = path.to_string();
            return Err(SppError::Corrupt(format!(
                "{group_path}: group nesting deeper than {MAX_DEPTH}"
            )));
        }
        Ok(())
    }

    /// Read an object header (v1 or v2) with continuations (§6.2).
    fn read_header(&mut self, addr: u64) -> Result<Vec<Message>, SppError> {
        let magic = bytes_at(self.ctx.data, addr, 4)?;
        let mut out = Vec::new();
        if magic == b"OHDR" {
            self.read_ohdr_v2(addr, &mut out)?;
        } else if magic.first() == Some(&1) {
            self.read_ohdr_v1(addr, &mut out)?;
        } else {
            return Err(SppError::Corrupt(format!(
                "object header at {addr}: unknown envelope"
            )));
        }
        // Continuations: OCHK chunks parse as v2, anything else as v1.
        let mut extra: Vec<(u64, u64)> = Vec::new();
        for msg in &out {
            if let Message::Continuation { addr, size } = msg {
                extra.push((*addr, *size));
            }
        }
        for (cont_addr, cont_size) in extra {
            if !self.visited.insert(cont_addr) {
                continue;
            }
            let peek = bytes_at(self.ctx.data, cont_addr, 4)?;
            if peek == b"OCHK" {
                self.read_ochk(cont_addr, cont_size, &mut out)?;
            } else {
                self.read_ohdr_v1(cont_addr, &mut out)?;
            }
        }
        // Unknown message types are loud skips, capped by the walk itself.
        let mut unknowns: Vec<u16> = Vec::new();
        for msg in &out {
            if let Message::Unknown { mtype } = msg {
                unknowns.push(*mtype);
            }
        }
        for mtype in unknowns {
            let at = addr;
            self.tree
                .notes
                .push(format!("{at}: unsupported message type {mtype} (skipped)"));
        }
        Ok(out)
    }

    /// v1 object header: 16-byte prefix, 8-padded messages (§6.2).
    fn read_ohdr_v1(&mut self, addr: u64, out: &mut Vec<Message>) -> Result<(), SppError> {
        let head = bytes_at(self.ctx.data, addr, 16)?;
        if head[0] != 1 {
            return Err(SppError::Corrupt(format!(
                "object header at {addr}: v1 version {}",
                head[0]
            )));
        }
        let count = u16::from_le_bytes([head[2], head[3]]) as usize;
        let area = u32::from_le_bytes([head[8], head[9], head[10], head[11]]) as u64;
        let base = addr + 16;
        let end = base
            .checked_add(area)
            .ok_or_else(|| SppError::Corrupt("v1 header area overflows".into()))?;
        bytes_at(self.ctx.data, base, area)?;
        let mut cursor = base;
        for _ in 0..count {
            let hdr = bytes_at(self.ctx.data, cursor, 8)?;
            let mtype = u16::from_le_bytes([hdr[0], hdr[1]]);
            let size = u16::from_le_bytes([hdr[2], hdr[3]]) as u64;
            let flags = hdr[4];
            let body = cursor + 8;
            let body_end = body
                .checked_add(size)
                .ok_or_else(|| SppError::Corrupt("v1 message size overflows".into()))?;
            if body_end > end {
                return Err(SppError::Corrupt(format!(
                    "object header at {addr}: message {mtype} size {size} overruns the header"
                )));
            }
            let payload = bytes_at(self.ctx.data, body, size)?;
            self.parse_message(mtype, flags, payload, out)?;
            let padded = (size + 7) & !7;
            cursor = body
                .checked_add(padded)
                .ok_or_else(|| SppError::Corrupt("v1 message padding overflows".into()))?;
            if cursor > end {
                return Err(SppError::Corrupt(format!(
                    "object header at {addr}: message {mtype} padding overruns the header"
                )));
            }
        }
        Ok(())
    }

    /// v2 object header: `OHDR` envelope, chunk-size code in flags (§6.2).
    fn read_ohdr_v2(&mut self, addr: u64, out: &mut Vec<Message>) -> Result<(), SppError> {
        let head = bytes_at(self.ctx.data, addr, 12)?;
        if head[4] != 2 {
            return Err(SppError::Corrupt(format!(
                "object header at {addr}: v2 version {}",
                head[4]
            )));
        }
        // Only 2-byte message sizes are supported; other codes are loud.
        if head[5] & 0x03 != 0x01 {
            let code = head[5] & 0x03;
            return Err(SppError::Unsupported(format!(
                "v2 object header chunk-size code {code}"
            )));
        }
        let area = u32::from_le_bytes([head[8], head[9], head[10], head[11]]) as u64;
        let base = addr + 12;
        let end = base
            .checked_add(area)
            .ok_or_else(|| SppError::Corrupt("v2 header area overflows".into()))?;
        bytes_at(self.ctx.data, base, area)?;
        self.read_v2_messages(base, end, out)?;
        Ok(())
    }

    /// `OCHK` continuation chunk: v2 messages over a sized area (§6.2).
    fn read_ochk(&mut self, addr: u64, _size: u64, out: &mut Vec<Message>) -> Result<(), SppError> {
        let head = bytes_at(self.ctx.data, addr, 8)?;
        if head[0..4] != *b"OCHK" {
            return Err(SppError::Corrupt(format!(
                "continuation at {addr}: not an OCHK block"
            )));
        }
        let area = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as u64;
        let base = addr + 8;
        let end = base
            .checked_add(area)
            .ok_or_else(|| SppError::Corrupt("OCHK area overflows".into()))?;
        bytes_at(self.ctx.data, base, area)?;
        self.read_v2_messages(base, end, out)?;
        Ok(())
    }

    /// v2 message walk: type u8 / size u16 / flags u8 (§6.2).
    fn read_v2_messages(
        &mut self,
        base: u64,
        end: u64,
        out: &mut Vec<Message>,
    ) -> Result<(), SppError> {
        let mut cursor = base;
        while cursor < end {
            let hdr = bytes_at(self.ctx.data, cursor, 4)?;
            let mtype = u16::from(hdr[0]);
            let size = u16::from_le_bytes([hdr[1], hdr[2]]) as u64;
            let flags = hdr[3];
            let mut body = cursor + 4;
            // Flag bit 2 carries a 2-byte creation order prefix.
            if flags & 0x04 != 0 {
                body += 2;
            }
            let body_end = body
                .checked_add(size)
                .ok_or_else(|| SppError::Corrupt("v2 message size overflows".into()))?;
            if body_end > end {
                return Err(SppError::Corrupt(format!(
                    "v2 message {mtype} size {size} overruns its header"
                )));
            }
            let payload = bytes_at(self.ctx.data, body, size)?;
            self.parse_message(mtype, flags, payload, out)?;
            cursor = body_end;
        }
        Ok(())
    }

    /// Dispatch one message payload (§6.3).
    fn parse_message(
        &mut self,
        mtype: u16,
        flags: u8,
        payload: &[u8],
        out: &mut Vec<Message>,
    ) -> Result<(), SppError> {
        let ctx = self.ctx;
        let msg = match mtype {
            MSG_DATASPACE => match parse_dataspace(ctx, payload)? {
                Some(space) => Message::Dataspace(space),
                None => Message::NullDataspace,
            },
            MSG_LINK_INFO => Message::LinkInfo {
                dense: parse_link_info(ctx, payload)?,
            },
            MSG_DATATYPE => Message::Datatype(parse_datatype(payload)?),
            MSG_LINK => Message::Link(parse_link(ctx, payload)?),
            MSG_LAYOUT => Message::Layout(parse_layout(ctx, payload)?),
            MSG_FILTER => Message::Pipeline(parse_pipeline(payload)?),
            MSG_ATTRIBUTE => Message::Attribute {
                name: parse_attribute_name(payload)?,
            },
            MSG_CONTINUATION => {
                let addr = uint_at(payload, 0, ctx.off_size)?;
                let size = uint_at(payload, ctx.off_size as u64, ctx.len_size)?;
                let target = resolve(ctx, addr, "continuation")?.ok_or_else(|| {
                    SppError::Corrupt("continuation to the undefined address".into())
                })?;
                Message::Continuation { addr: target, size }
            }
            MSG_SYMBOL_TABLE => {
                let btree_stored = uint_at(payload, 0, ctx.off_size)?;
                let heap_stored = uint_at(payload, ctx.off_size as u64, ctx.off_size)?;
                let btree = resolve(ctx, btree_stored, "symbol table b-tree")?
                    .ok_or_else(|| SppError::Corrupt("symbol table without b-tree".into()))?;
                let heap = resolve(ctx, heap_stored, "symbol table heap")?
                    .ok_or_else(|| SppError::Corrupt("symbol table without heap".into()))?;
                Message::SymbolTable { btree, heap }
            }
            _ => Message::Unknown { mtype },
        };
        let _ = flags;
        out.push(msg);
        Ok(())
    }

    /// Read a local heap's name segment (`HEAP`, §6.3).
    fn read_heap(&mut self, addr: u64) -> Result<Vec<u8>, SppError> {
        let ctx = self.ctx;
        let off = ctx.off_size as u64;
        let len = ctx.len_size as u64;
        let head = bytes_at(ctx.data, addr, 8 + 2 * len + off)?;
        if head[0..4] != *b"HEAP" {
            return Err(SppError::Corrupt(format!(
                "heap at {addr}: not a HEAP block"
            )));
        }
        let mut cursor = 8u64;
        let data_size = uint_at(ctx.data, addr + cursor, ctx.len_size)?;
        cursor += len;
        // Free-list head: structural, skipped.
        cursor += len;
        let segment_stored = uint_at(ctx.data, addr + cursor, ctx.off_size)?;
        let segment = resolve(ctx, segment_stored, "heap segment")?
            .ok_or_else(|| SppError::Corrupt(format!("heap at {addr}: undefined segment")))?;
        Ok(bytes_at(ctx.data, segment, data_size)?.to_vec())
    }

    /// Walk a group v1 B-tree into `(name, header-address)` entries (§6.3).
    fn walk_group_btree(
        &mut self,
        addr: u64,
        heap: &[u8],
        out: &mut Vec<(String, u64)>,
    ) -> Result<(), SppError> {
        let ctx = self.ctx;
        let node = BTreeNode::read(ctx, addr, BTreeKind::Group)?;
        for entry in node.entries {
            match entry {
                BTreeEntry::Group { key, child } => {
                    if node.level == 0 {
                        self.read_snod(child, heap, out)?;
                    } else {
                        let _ = key;
                        self.walk_group_btree(child, heap, out)?;
                    }
                }
                BTreeEntry::Chunk { .. } => {
                    return Err(SppError::Corrupt(format!(
                        "group b-tree at {addr}: chunk entry in a group index"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Read a `SNOD` symbol-table node (§6.3).
    fn read_snod(
        &mut self,
        addr: u64,
        heap: &[u8],
        out: &mut Vec<(String, u64)>,
    ) -> Result<(), SppError> {
        let ctx = self.ctx;
        let head = bytes_at(ctx.data, addr, 8)?;
        if head[0..4] != *b"SNOD" {
            return Err(SppError::Corrupt(format!("node at {addr}: not SNOD")));
        }
        if head[4] != 1 {
            return Err(SppError::Corrupt(format!(
                "node at {addr}: SNOD version {}",
                head[4]
            )));
        }
        let count = u16::from_le_bytes([head[6], head[7]]) as u64;
        let stride = 28 + ctx.off_size as u64;
        let area = count
            .checked_mul(stride)
            .ok_or_else(|| SppError::Corrupt("SNOD entry count overflows".into()))?;
        let base = addr + 8;
        bytes_at(ctx.data, base, area)?;
        for i in 0..count {
            let at = base + i * stride;
            let name_off = uint_at(ctx.data, at, 4)? as usize;
            let header_stored = uint_at(ctx.data, at + 4, ctx.off_size)?;
            let target = resolve(ctx, header_stored, "symbol entry")?
                .ok_or_else(|| SppError::Corrupt("symbol entry without object header".into()))?;
            let name = heap_c_string(heap, name_off)?;
            out.push((name, target));
        }
        Ok(())
    }

    /// Decode one dataset object header into raw element bytes (§6.4).
    fn decode_dataset(&mut self, path: &str, messages: &[Message]) -> Result<(), SppError> {
        let space = messages.iter().find_map(|m| match m {
            Message::Dataspace(space) => Some(space.clone()),
            _ => None,
        });
        if messages.iter().any(|m| matches!(m, Message::NullDataspace)) {
            let note_path = path.to_string();
            self.tree.notes.push(format!(
                "{note_path}: null dataspace (no elements, skipped)"
            ));
            return Ok(());
        }
        let space = space.ok_or_else(|| {
            let missing = path.to_string();
            SppError::Unsupported(format!("{missing}: dataset without inline dataspace"))
        })?;
        let dtype = messages
            .iter()
            .find_map(|m| match m {
                Message::Datatype(dtype) => Some(*dtype),
                _ => None,
            })
            .ok_or_else(|| {
                let missing = path.to_string();
                SppError::Unsupported(format!("{missing}: named/committed datatype"))
            })?;
        let layout = messages.iter().find_map(|m| match m {
            Message::Layout(layout) => Some(layout),
            _ => None,
        });
        let layout = layout.ok_or_else(|| {
            let missing = path.to_string();
            SppError::Unsupported(format!("{missing}: dataset without layout"))
        })?;
        let pipeline: Vec<FilterKind> = messages
            .iter()
            .flat_map(|m| match m {
                Message::Pipeline(filters) => filters.clone(),
                _ => Vec::new(),
            })
            .collect();
        let mut attr_count = 0;
        let mut has_m3 = false;
        for msg in messages {
            if let Message::Attribute { name } = msg {
                attr_count += 1;
                has_m3 |= name == "m3_x64_128";
            }
        }
        let elem = dtype.byte_size() as u64;
        let total_elems = space
            .dims
            .iter()
            .try_fold(1u64, |acc, d| acc.checked_mul(*d))
            .ok_or_else(|| {
                let bad = path.to_string();
                SppError::Corrupt(format!("{bad}: dataspace size overflows"))
            })?;
        let total_bytes = total_elems.checked_mul(elem).ok_or_else(|| {
            let bad = path.to_string();
            SppError::Corrupt(format!("{bad}: dataset size overflows"))
        })?;
        if total_bytes > MAX_DATASET_BYTES {
            let big = path.to_string();
            return Err(SppError::Unsupported(format!(
                "{big}: dataset payload {total_bytes} bytes exceeds the 256 MiB cap"
            )));
        }
        let bytes = match layout {
            Layout::Compact(inline) => {
                let inline_len = inline.len() as u64;
                if inline_len != total_bytes {
                    let bad = path.to_string();
                    return Err(SppError::Corrupt(format!(
                        "{bad}: compact size {inline_len} != dataspace {total_bytes}"
                    )));
                }
                self.run_pipeline(
                    path,
                    inline.clone(),
                    &pipeline,
                    total_bytes as usize,
                    elem as usize,
                )?
            }
            Layout::Contiguous { addr, size } => {
                if *size != total_bytes {
                    let bad = path.to_string();
                    return Err(SppError::Corrupt(format!(
                        "{bad}: contiguous size {size} != dataspace {total_bytes}"
                    )));
                }
                let stored = bytes_at(self.ctx.data, *addr, *size)?.to_vec();
                self.run_pipeline(path, stored, &pipeline, total_bytes as usize, elem as usize)?
            }
            Layout::Chunked(chunked) => {
                self.decode_chunked(path, &space, dtype, chunked, &pipeline, total_bytes)?
            }
        };
        let (layout_name, chunk_dims) = match layout {
            Layout::Compact(_) => ("compact", None),
            Layout::Contiguous { .. } => ("contiguous", None),
            Layout::Chunked(chunked) => ("chunked", Some(chunked.chunk_dims.clone())),
        };
        self.tree.datasets.push(RawDataset {
            path: path.to_string(),
            shape: space.dims.clone(),
            dtype,
            layout_name,
            chunk_dims,
            filters: pipeline,
            attr_count,
            has_m3,
            bytes,
        });
        Ok(())
    }

    /// Decode chunked data through the v1 B-tree chunk index (§6.4).
    #[allow(clippy::too_many_arguments)]
    fn decode_chunked(
        &mut self,
        path: &str,
        space: &Space,
        dtype: Dtype,
        chunked: &Chunked,
        pipeline: &[FilterKind],
        total_bytes: u64,
    ) -> Result<Vec<u8>, SppError> {
        if chunked.rank != space.rank {
            let bad = path.to_string();
            return Err(SppError::Unsupported(format!(
                "{bad}: chunked rank {} != dataspace rank {}",
                chunked.rank, space.rank
            )));
        }
        if chunked.elem != dtype.byte_size() {
            let bad = path.to_string();
            return Err(SppError::Corrupt(format!(
                "{bad}: chunk element {} != datatype {}",
                chunked.elem,
                dtype.byte_size()
            )));
        }
        for dim in &chunked.chunk_dims {
            if *dim == 0 {
                let bad = path.to_string();
                return Err(SppError::Corrupt(format!("{bad}: zero chunk dimension")));
            }
        }
        // Grid counts and strides for placement.
        let mut counts: Vec<u64> = Vec::with_capacity(space.rank);
        for (dim, chunk) in space.dims.iter().zip(chunked.chunk_dims.iter()) {
            counts.push(dim.div_ceil(*chunk));
        }
        let chunk_elems = chunked
            .chunk_dims
            .iter()
            .try_fold(1u64, |acc, d| acc.checked_mul(*d))
            .ok_or_else(|| {
                let bad = path.to_string();
                SppError::Corrupt(format!("{bad}: chunk volume overflows"))
            })?;
        let chunk_bytes = chunk_elems * chunked.elem as u64;
        // Collect the index: coordinates -> (filter mask, address, size).
        let mut index: HashMap<Vec<u64>, (u32, u64, u64)> = HashMap::new();
        self.walk_chunk_btree(chunked.btree, chunked.rank, &mut index)?;
        // Output strides (row-major, last axis fastest).
        let mut strides = vec![1u64; space.rank.max(1)];
        for i in (0..space.rank.saturating_sub(1)).rev() {
            strides[i] = strides[i + 1]
                .checked_mul(space.dims[i + 1])
                .ok_or_else(|| {
                    let bad = path.to_string();
                    SppError::Corrupt(format!("{bad}: stride overflows"))
                })?;
        }
        let mut out = vec![0u8; total_bytes as usize];
        let mut coords = vec![0u64; space.rank];
        loop {
            let expected = self.chunk_footprint(
                path,
                &coords,
                &space.dims,
                &chunked.chunk_dims,
                chunked.elem,
            )?;
            let decoded = match index.get(&coords) {
                None => vec![0u8; expected],
                Some((mask, addr, stored_size)) => {
                    let stored = bytes_at(self.ctx.data, *addr, *stored_size)?.to_vec();
                    // Mask bits past 31 cannot name a filter: those filters run.
                    let kept: Vec<FilterKind> = pipeline
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i >= 32 || mask & (1u32 << *i) == 0)
                        .map(|(_, f)| *f)
                        .collect();
                    let full = chunk_bytes as usize;
                    let bytes = self.run_pipeline(path, stored, &kept, full, chunked.elem)?;
                    if bytes.len() != expected {
                        let bad = path.to_string();
                        return Err(SppError::Corrupt(format!(
                            "{bad}: chunk at {coords:?} decoded {} != footprint {expected}",
                            bytes.len()
                        )));
                    }
                    bytes
                }
            };
            self.place_chunk(
                &decoded,
                &coords,
                &space.dims,
                &chunked.chunk_dims,
                &strides,
                chunked.elem,
                &mut out,
            )?;
            // Odometer over the grid.
            let mut carry = true;
            for axis in (0..space.rank).rev() {
                if !carry {
                    break;
                }
                coords[axis] += 1;
                if coords[axis] < counts[axis] {
                    carry = false;
                } else {
                    coords[axis] = 0;
                }
            }
            if carry {
                break;
            }
        }
        Ok(out)
    }

    /// Decoded byte count of one (possibly edge) chunk.
    fn chunk_footprint(
        &self,
        path: &str,
        coords: &[u64],
        dims: &[u64],
        chunk_dims: &[u64],
        elem: usize,
    ) -> Result<usize, SppError> {
        let mut elems = 1u64;
        for ((coord, dim), chunk) in coords.iter().zip(dims.iter()).zip(chunk_dims.iter()) {
            let start = coord.checked_mul(*chunk).ok_or_else(|| {
                let bad = path.to_string();
                SppError::Corrupt(format!("{bad}: chunk start overflows"))
            })?;
            let left = dim.saturating_sub(start);
            elems = elems.checked_mul(left.min(*chunk)).ok_or_else(|| {
                let bad = path.to_string();
                SppError::Corrupt(format!("{bad}: chunk footprint overflows"))
            })?;
        }
        elems
            .checked_mul(elem as u64)
            .and_then(|n| n.try_into().ok())
            .ok_or_else(|| {
                let bad = path.to_string();
                SppError::Corrupt(format!("{bad}: chunk footprint overflows"))
            })
    }

    /// Copy one decoded chunk into the row-major output.
    #[allow(clippy::too_many_arguments)]
    fn place_chunk(
        &self,
        chunk: &[u8],
        coords: &[u64],
        dims: &[u64],
        chunk_dims: &[u64],
        strides: &[u64],
        elem: usize,
        out: &mut [u8],
    ) -> Result<(), SppError> {
        // Element odometer inside the chunk's footprint.
        let mut extents: Vec<u64> = Vec::with_capacity(dims.len());
        let mut starts: Vec<u64> = Vec::with_capacity(dims.len());
        for ((coord, dim), chunk_dim) in coords.iter().zip(dims.iter()).zip(chunk_dims.iter()) {
            let Some(start) = coord.checked_mul(*chunk_dim) else {
                return Err(SppError::Corrupt("chunk start overflows".into()));
            };
            starts.push(start);
            extents.push(dim.saturating_sub(start).min(*chunk_dim));
        }
        // Chunk-local row-major strides over the full chunk box.
        let mut local = vec![1u64; dims.len().max(1)];
        for i in (0..dims.len().saturating_sub(1)).rev() {
            local[i] = local[i + 1].saturating_mul(chunk_dims[i + 1]);
        }
        let mut pos = vec![0u64; dims.len()];
        loop {
            let mut src_elem = 0u64;
            let mut dst_elem = 0u64;
            for axis in 0..dims.len() {
                src_elem = src_elem.saturating_add(pos[axis].saturating_mul(local[axis]));
                dst_elem = dst_elem.saturating_add(
                    starts[axis]
                        .saturating_add(pos[axis])
                        .saturating_mul(strides[axis]),
                );
            }
            let src = src_elem as usize * elem;
            let dst = dst_elem as usize * elem;
            if src + elem > chunk.len() || dst + elem > out.len() {
                return Err(SppError::Corrupt("chunk placement out of bounds".into()));
            }
            out[dst..dst + elem].copy_from_slice(&chunk[src..src + elem]);
            let mut carry = true;
            for axis in (0..dims.len()).rev() {
                if !carry {
                    break;
                }
                pos[axis] += 1;
                if pos[axis] < extents[axis] {
                    carry = false;
                } else {
                    pos[axis] = 0;
                }
            }
            if carry {
                break;
            }
        }
        Ok(())
    }

    /// Walk a chunk v1 B-tree into the coordinate index (§6.4).
    fn walk_chunk_btree(
        &mut self,
        addr: u64,
        rank: usize,
        out: &mut HashMap<Vec<u64>, (u32, u64, u64)>,
    ) -> Result<(), SppError> {
        let ctx = self.ctx;
        let node = BTreeNode::read(ctx, addr, BTreeKind::Chunk(rank))?;
        for entry in node.entries {
            match entry {
                BTreeEntry::Chunk {
                    coords,
                    mask,
                    size,
                    child,
                } => {
                    if node.level == 0 {
                        let target = resolve(ctx, child, "chunk data")?.ok_or_else(|| {
                            SppError::Corrupt("chunk without data address".into())
                        })?;
                        bytes_at(ctx.data, target, size)?;
                        out.insert(coords, (mask, target, size));
                    } else {
                        self.walk_chunk_btree(child, rank, out)?;
                    }
                }
                BTreeEntry::Group { .. } => {
                    return Err(SppError::Corrupt(format!(
                        "chunk b-tree at {addr}: group entry in a chunk index"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Run the filter pipeline in reverse (decode order), honoring masks.
    fn run_pipeline(
        &mut self,
        path: &str,
        mut bytes: Vec<u8>,
        pipeline: &[FilterKind],
        _expected: usize,
        elem: usize,
    ) -> Result<Vec<u8>, SppError> {
        for filter in pipeline.iter().rev() {
            match filter {
                FilterKind::Fletcher32 => {
                    // Recorded, never verified (§6.3).
                }
                FilterKind::Deflate => {
                    bytes = inflate_raw(path, &bytes)?;
                }
                FilterKind::Shuffle => {
                    bytes = unshuffle(path, &bytes, elem)?;
                }
            }
        }
        Ok(bytes)
    }
}

/// Resolve a soft-link target against the containing group path.
fn resolve_soft(group: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.to_string();
    }
    if group == "/" {
        return format!("/{target}");
    }
    // One `..` level is enough for the subset; deeper climbs stay literal.
    if let Some(rest) = target.strip_prefix("../") {
        if let Some(slash) = group.rfind('/') {
            let parent = if slash == 0 { "/" } else { &group[..slash] };
            return Walker::child_path(parent, rest);
        }
    }
    Walker::child_path(group, target)
}

/// Raw inflate (no zlib header): the HDF5 deflate filter stream (§6.4).
fn inflate_raw(path: &str, input: &[u8]) -> Result<Vec<u8>, SppError> {
    let mut decoder = flate2::Decompress::new(false);
    let mut out = Vec::new();
    // Grow the window until the stream ends; hostile sizes stay capped by
    // the dataset cap checked before decode (chunk footprints bound this).
    let mut produced = 0usize;
    loop {
        out.reserve(65536);
        let capacity = out.capacity() - out.len();
        // SAFETY-free growth: extend with zeros, then truncate to produced.
        out.extend(std::iter::repeat_n(0, capacity));
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        match decoder.decompress(
            &input[before_in as usize..],
            &mut out[produced..],
            flate2::FlushDecompress::Finish,
        ) {
            Ok(status) => {
                produced = decoder.total_out() as usize;
                out.truncate(produced);
                let _ = before_out;
                if status == flate2::Status::StreamEnd {
                    return Ok(out);
                }
                if produced > MAX_DATASET_BYTES as usize {
                    let big = path.to_string();
                    return Err(SppError::Unsupported(format!(
                        "{big}: inflated payload exceeds the 256 MiB cap"
                    )));
                }
                if status == flate2::Status::Ok {
                    continue;
                }
                let bad = path.to_string();
                return Err(SppError::Corrupt(format!("{bad}: deflate stream stalled")));
            }
            Err(e) => {
                let bad = path.to_string();
                return Err(SppError::Corrupt(format!("{bad}: deflate error ({e})")));
            }
        }
    }
}

/// Reverse the shuffle filter for elements of `elem` bytes (§6.4).
fn unshuffle(path: &str, input: &[u8], elem: usize) -> Result<Vec<u8>, SppError> {
    if elem == 0 {
        let bad = path.to_string();
        return Err(SppError::Corrupt(format!(
            "{bad}: shuffle with zero element size"
        )));
    }
    if input.len() % elem != 0 {
        let bad = path.to_string();
        return Err(SppError::Corrupt(format!(
            "{bad}: shuffled size {} not a multiple of {elem}",
            input.len()
        )));
    }
    let count = input.len() / elem;
    let mut out = vec![0u8; input.len()];
    for k in 0..count {
        for j in 0..elem {
            out[k * elem + j] = input[j * count + k];
        }
    }
    Ok(out)
}

/// Dataspace message v1/v2: scalar + simple rank 1–3 (§6.3).
fn parse_dataspace(ctx: Ctx<'_>, payload: &[u8]) -> Result<Option<Space>, SppError> {
    if payload.len() < 4 {
        return Err(SppError::Corrupt("dataspace message truncated".into()));
    }
    let version = payload[0];
    let rank = payload[1] as usize;
    let flags = payload[2];
    // Permutation (bit 1) reorders axes: out of subset, loudly.
    if flags & 0x02 != 0 {
        return Err(SppError::Unsupported(
            "dataspace with axis permutation".into(),
        ));
    }
    let maxdims_present = flags & 0x01 != 0;
    let dims_off: usize = match version {
        1 => {
            if payload.len() < 8 {
                return Err(SppError::Corrupt("v1 dataspace header truncated".into()));
            }
            8
        }
        2 => {
            // Type byte: 0 scalar (rank 0), 1 simple (rank 1–3), 2 null.
            match payload[3] {
                0 => {
                    if rank != 0 {
                        return Err(SppError::Corrupt("scalar dataspace with rank".into()));
                    }
                }
                1 => {
                    if rank == 0 || rank > 3 {
                        return Err(SppError::Unsupported(format!(
                            "simple dataspace rank {rank}"
                        )));
                    }
                }
                2 => return Ok(None),
                other => {
                    return Err(SppError::Unsupported(format!("dataspace type {other}")));
                }
            }
            4
        }
        other => {
            return Err(SppError::Unsupported(format!("dataspace version {other}")));
        }
    };
    if rank > 3 {
        return Err(SppError::Unsupported(format!("dataspace rank {rank}")));
    }
    let width = ctx.len_size;
    let dims_end = dims_off
        .checked_add(rank * width)
        .ok_or_else(|| SppError::Corrupt("dataspace dims overflow".into()))?;
    if payload.len() < dims_end {
        return Err(SppError::Corrupt(
            "dataspace dims overrun the message".into(),
        ));
    }
    let mut dims = Vec::with_capacity(rank);
    for i in 0..rank {
        dims.push(uint_le(payload, dims_off + i * width, width)?);
    }
    if maxdims_present {
        let max_end = dims_end
            .checked_add(rank * width)
            .ok_or_else(|| SppError::Corrupt("dataspace max-dims overflow".into()))?;
        if payload.len() < max_end {
            return Err(SppError::Corrupt(
                "dataspace max-dims overrun the message".into(),
            ));
        }
        for (i, dim) in dims.iter().take(rank).enumerate() {
            let max = uint_le(payload, dims_end + i * width, width)?;
            if max != *dim {
                return Err(SppError::Unsupported(format!(
                    "max-dims {max} != dims {dim} (unlimited/growing spaces unsupported)"
                )));
            }
        }
    }
    Ok(Some(Space { rank, dims }))
}

/// Datatype message under the strict allow-list (§6.3).
fn parse_datatype(payload: &[u8]) -> Result<Dtype, SppError> {
    if payload.len() < 8 {
        return Err(SppError::Corrupt("datatype message truncated".into()));
    }
    let class = payload[0] & 0x0F;
    let size = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]) as usize;
    match class {
        0 => {
            // Fixed-point: LE only, offset 0, full precision.
            if payload[1] & 0x01 != 0 {
                return Err(SppError::Unsupported("big-endian fixed-point".into()));
            }
            if payload.len() < 12 {
                return Err(SppError::Corrupt("fixed-point properties truncated".into()));
            }
            let offset = u16::from_le_bytes([payload[8], payload[9]]);
            let precision = u16::from_le_bytes([payload[10], payload[11]]);
            if offset != 0 {
                return Err(SppError::Unsupported(format!(
                    "fixed-point with bit offset {offset}"
                )));
            }
            if precision as usize != 8 * size {
                return Err(SppError::Unsupported(format!(
                    "fixed-point precision {precision} != 8 x size {size}"
                )));
            }
            let signed = payload[1] & 0x10 != 0;
            match (size, signed) {
                (1, false) => Ok(Dtype::U8),
                (2, false) => Ok(Dtype::U16),
                (4, true) => Ok(Dtype::I32),
                _ => Err(SppError::Unsupported(format!(
                    "fixed-point size {size} signed={signed}"
                ))),
            }
        }
        1 => {
            // Floating-point: LE IEEE-754 with exact property bytes.
            if payload[1] & 0x01 != 0 {
                return Err(SppError::Unsupported("big-endian float".into()));
            }
            if payload.len() < 20 {
                return Err(SppError::Corrupt("float properties truncated".into()));
            }
            let exp_loc = u16::from_le_bytes([payload[8], payload[9]]);
            let exp_size = u16::from_le_bytes([payload[10], payload[11]]);
            let mant_loc = u16::from_le_bytes([payload[12], payload[13]]);
            let mant_size = u16::from_le_bytes([payload[14], payload[15]]);
            let bias = u32::from_le_bytes([payload[16], payload[17], payload[18], payload[19]]);
            match (size, exp_loc, exp_size, mant_loc, mant_size, bias) {
                (4, 23, 8, 0, 23, 127) => Ok(Dtype::F32),
                (8, 52, 11, 0, 52, 1023) => Ok(Dtype::F64),
                _ => Err(SppError::Unsupported(format!(
                    "non-IEEE float (size {size} exp {exp_loc}/{exp_size} mant {mant_loc}/{mant_size} bias {bias})"
                ))),
            }
        }
        3 => {
            // Fixed-length string: the length rides the payload bytes.
            if size == 0 {
                return Err(SppError::Unsupported("zero-length string".into()));
            }
            Ok(Dtype::Str(size))
        }
        _ => Err(SppError::Unsupported(match class {
            6 => "compound datatype".to_string(),
            8 => "enum datatype".to_string(),
            9 => "variable-length datatype".to_string(),
            10 => "array datatype".to_string(),
            other => format!("datatype class {other}"),
        })),
    }
}

/// Layout message v3: compact / contiguous / chunked (§6.3).
fn parse_layout(ctx: Ctx<'_>, payload: &[u8]) -> Result<Layout, SppError> {
    if payload.len() < 2 {
        return Err(SppError::Corrupt("layout message truncated".into()));
    }
    if payload[0] != 3 {
        return Err(SppError::Unsupported(format!(
            "layout version {}",
            payload[0]
        )));
    }
    match payload[1] {
        LAYOUT_COMPACT => {
            if payload.len() < 4 {
                return Err(SppError::Corrupt("compact layout header truncated".into()));
            }
            let size = u16::from_le_bytes([payload[2], payload[3]]) as usize;
            if payload.len() < 4 + size {
                return Err(SppError::Corrupt(
                    "compact data overruns the message".into(),
                ));
            }
            Ok(Layout::Compact(payload[4..4 + size].to_vec()))
        }
        LAYOUT_CONTIGUOUS => {
            let off = ctx.off_size;
            let len = ctx.len_size;
            if payload.len() < 2 + off + len {
                return Err(SppError::Corrupt("contiguous layout truncated".into()));
            }
            let addr_stored = uint_le(payload, 2, off)?;
            let size = uint_le(payload, 2 + off, len)?;
            let addr = resolve(ctx, addr_stored, "contiguous data")?
                .ok_or_else(|| SppError::Corrupt("contiguous layout without address".into()))?;
            bytes_at(ctx.data, addr, size)?;
            Ok(Layout::Contiguous { addr, size })
        }
        LAYOUT_CHUNKED => {
            let off = ctx.off_size;
            if payload.len() < 4 {
                return Err(SppError::Corrupt("chunked layout header truncated".into()));
            }
            // 1-byte dimensionality (grid-resolved).
            let rank = payload[2] as usize;
            if rank == 0 || rank > 3 {
                return Err(SppError::Unsupported(format!("chunked rank {rank}")));
            }
            let dims_off = 3 + off;
            let dims_end = dims_off
                .checked_add(rank * 4)
                .ok_or_else(|| SppError::Corrupt("chunk dims overflow".into()))?;
            if payload.len() < dims_end + 4 {
                return Err(SppError::Corrupt("chunked layout truncated".into()));
            }
            let btree_stored = uint_le(payload, 3, off)?;
            let btree = resolve(ctx, btree_stored, "chunk b-tree")?
                .ok_or_else(|| SppError::Corrupt("chunked layout without b-tree".into()))?;
            let mut chunk_dims = Vec::with_capacity(rank);
            for i in 0..rank {
                chunk_dims.push(uint_le(payload, dims_off + 4 * i, 4)?);
            }
            let elem = uint_le(payload, dims_end, 4)?;
            if elem == 0 || elem > 64 {
                return Err(SppError::Corrupt(format!("chunk element size {elem}")));
            }
            Ok(Layout::Chunked(Chunked {
                rank,
                btree,
                chunk_dims,
                elem: elem as usize,
            }))
        }
        other => Err(SppError::Unsupported(match other {
            3 => "virtual storage".to_string(),
            _ => format!("layout class {other}"),
        })),
    }
}

/// Filter pipeline v1/v2: deflate / shuffle / fletcher32 only (§6.3).
fn parse_pipeline(payload: &[u8]) -> Result<Vec<FilterKind>, SppError> {
    if payload.len() < 4 {
        return Err(SppError::Corrupt("filter pipeline truncated".into()));
    }
    let version = payload[0];
    if version != 1 && version != 2 {
        return Err(SppError::Unsupported(format!(
            "filter pipeline version {version}"
        )));
    }
    let count = payload[1] as usize;
    let mut cursor = 4;
    let mut filters = Vec::with_capacity(count.min(16));
    for _ in 0..count {
        if payload.len() < cursor + 8 {
            return Err(SppError::Corrupt("filter entry truncated".into()));
        }
        let id = u16::from_le_bytes([payload[cursor], payload[cursor + 1]]);
        let name_len = u16::from_le_bytes([payload[cursor + 2], payload[cursor + 3]]) as usize;
        let flags = u16::from_le_bytes([payload[cursor + 4], payload[cursor + 5]]);
        let client_count = u16::from_le_bytes([payload[cursor + 6], payload[cursor + 7]]) as usize;
        cursor += 8;
        if payload.len() < cursor + name_len {
            return Err(SppError::Corrupt("filter name overruns the message".into()));
        }
        cursor += name_len;
        // v1 pads names to 8; v2 packs them.
        if version == 1 {
            cursor = (cursor + 7) & !7;
        }
        let data_end = cursor
            .checked_add(client_count * 4)
            .ok_or_else(|| SppError::Corrupt("filter client data overflows".into()))?;
        if payload.len() < data_end {
            return Err(SppError::Corrupt(
                "filter client data overruns the message".into(),
            ));
        }
        // Deflate client data is an opaque u32 array: the level is ignored.
        cursor = data_end;
        let optional = flags & 0x0001 != 0;
        match id {
            FILTER_DEFLATE => filters.push(FilterKind::Deflate),
            FILTER_SHUFFLE => filters.push(FilterKind::Shuffle),
            FILTER_FLETCHER32 => filters.push(FilterKind::Fletcher32),
            _ => {
                if optional {
                    continue;
                }
                return Err(SppError::Unsupported(format!("filter id {id}")));
            }
        }
    }
    Ok(filters)
}

/// Link message v1: hard / soft; external and user-defined reported (§6.3).
fn parse_link(ctx: Ctx<'_>, payload: &[u8]) -> Result<Link, SppError> {
    if payload.len() < 2 {
        return Err(SppError::Corrupt("link message truncated".into()));
    }
    if payload[0] != 1 {
        return Err(SppError::Unsupported(format!(
            "link version {}",
            payload[0]
        )));
    }
    let flags = payload[1];
    let width = match flags & 0x03 {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let mut cursor = 2usize;
    // Flag bit 1 carries an 8-byte creation order prefix.
    if flags & 0x04 != 0 {
        cursor += 8;
    }
    // Flag bit 3 carries the link-type byte; absent means hard.
    let mut link_type = 0u8;
    if flags & 0x08 != 0 {
        if payload.len() < cursor + 1 {
            return Err(SppError::Corrupt("link type overruns the message".into()));
        }
        link_type = payload[cursor];
        cursor += 1;
    }
    if payload.len() < cursor + width {
        return Err(SppError::Corrupt(
            "link name length overruns the message".into(),
        ));
    }
    let name_len = uint_le(payload, cursor, width)? as usize;
    cursor += width;
    if payload.len() < cursor + name_len {
        return Err(SppError::Corrupt("link name overruns the message".into()));
    }
    let name = String::from_utf8(payload[cursor..cursor + name_len].to_vec())
        .map_err(|_| SppError::Corrupt("link name is not UTF-8".into()))?;
    cursor += name_len;
    match link_type {
        0 => {
            if payload.len() < cursor + ctx.off_size {
                return Err(SppError::Corrupt("hard link address truncated".into()));
            }
            let stored = uint_le(payload, cursor, ctx.off_size)?;
            let target = resolve(ctx, stored, "hard link")?
                .ok_or_else(|| SppError::Corrupt(format!("hard link '{name}' without target")))?;
            Ok(Link {
                name,
                target: LinkTarget::Hard(target),
            })
        }
        1 => {
            if payload.len() < cursor + 2 {
                return Err(SppError::Corrupt("soft link target truncated".into()));
            }
            let target_len = u16::from_le_bytes([payload[cursor], payload[cursor + 1]]) as usize;
            cursor += 2;
            if payload.len() < cursor + target_len {
                return Err(SppError::Corrupt("soft link target overruns".into()));
            }
            let target = String::from_utf8(payload[cursor..cursor + target_len].to_vec())
                .map_err(|_| SppError::Corrupt("soft link target is not UTF-8".into()))?;
            Ok(Link {
                name,
                target: LinkTarget::Soft(target),
            })
        }
        _ => Ok(Link {
            name: name.clone(),
            target: LinkTarget::External(name),
        }),
    }
}

/// Link-info message: dense exactly when the heap address is defined (§6.5).
fn parse_link_info(ctx: Ctx<'_>, payload: &[u8]) -> Result<bool, SppError> {
    let off = ctx.off_size;
    if payload.len() < 10 + 2 * off {
        return Err(SppError::Corrupt("link info message truncated".into()));
    }
    let heap_stored = uint_le(payload, 10, off)?;
    Ok(resolve(ctx, heap_stored, "link info heap")?.is_some())
}

/// Attribute message: the name only (counted, not parsed, §6.3).
fn parse_attribute_name(payload: &[u8]) -> Result<String, SppError> {
    if payload.len() < 8 {
        return Err(SppError::Corrupt("attribute message truncated".into()));
    }
    let name_len = u16::from_le_bytes([payload[2], payload[3]]) as usize;
    if payload.len() < 8 + name_len {
        return Err(SppError::Corrupt(
            "attribute name overruns the message".into(),
        ));
    }
    let raw = &payload[8..8 + name_len];
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    String::from_utf8(raw[..end].to_vec())
        .map_err(|_| SppError::Corrupt("attribute name is not UTF-8".into()))
}

/// Little-endian integer from a message-local slice (bounds-checked: the
/// never-crash doctrine holds even if a caller miscounts).
fn uint_le(payload: &[u8], off: usize, width: usize) -> Result<u64, SppError> {
    let end = off
        .checked_add(width)
        .ok_or_else(|| SppError::Corrupt("message integer overflows".into()))?;
    if end > payload.len() {
        return Err(SppError::Corrupt(format!(
            "message integer [{off}..{end}) overruns {} bytes",
            payload.len()
        )));
    }
    let mut value: u64 = 0;
    for (i, b) in payload[off..end].iter().enumerate() {
        value |= (*b as u64) << (8 * i);
    }
    Ok(value)
}

/// NUL-terminated heap string at a heap offset (§6.3).
fn heap_c_string(heap: &[u8], off: usize) -> Result<String, SppError> {
    if off >= heap.len() {
        return Err(SppError::Corrupt("heap name offset out of bounds".into()));
    }
    let end = heap[off..]
        .iter()
        .position(|b| *b == 0)
        .map(|i| off + i)
        .ok_or_else(|| SppError::Corrupt("heap name without terminator".into()))?;
    String::from_utf8(heap[off..end].to_vec())
        .map_err(|_| SppError::Corrupt("heap name is not UTF-8".into()))
}

/// v1 B-tree index family: group keys or chunk keys.
#[derive(Debug, Clone, Copy)]
enum BTreeKind {
    Group,
    Chunk(usize),
}

/// One v1 B-tree node (`TREE`, §6.3–§6.4).
struct BTreeNode {
    level: u8,
    entries: Vec<BTreeEntry>,
}

/// One v1 B-tree entry: a group key or a chunk key + child address.
enum BTreeEntry {
    Group {
        key: u32,
        child: u64,
    },
    Chunk {
        coords: Vec<u64>,
        mask: u32,
        size: u64,
        child: u64,
    },
}

impl BTreeNode {
    /// Read and bounds-check one node; the key shape follows the family.
    fn read(ctx: Ctx<'_>, addr: u64, kind: BTreeKind) -> Result<BTreeNode, SppError> {
        let off = ctx.off_size;
        let head = bytes_at(ctx.data, addr, 8 + 2 * off as u64)?;
        if head[0..4] != *b"TREE" {
            return Err(SppError::Corrupt(format!("b-tree at {addr}: not TREE")));
        }
        let level = head[5];
        let used = u16::from_le_bytes([head[6], head[7]]) as u64;
        let key_size: u64 = match kind {
            BTreeKind::Group => 4,
            // u32 byte-size + u32 filter-mask + (D+1) u64 offsets (§6.4).
            BTreeKind::Chunk(rank) => 8 + 8 * (rank as u64 + 1),
        };
        let stride = key_size + off as u64;
        let area = used
            .checked_mul(stride)
            .ok_or_else(|| SppError::Corrupt("b-tree entry count overflows".into()))?;
        let base = addr + 8 + 2 * off as u64;
        bytes_at(ctx.data, base, area)?;
        let mut entries = Vec::with_capacity(used.min(1024) as usize);
        for i in 0..used {
            let at = base + i * stride;
            match kind {
                BTreeKind::Group => {
                    let key = uint_at(ctx.data, at, 4)? as u32;
                    let child_stored = uint_at(ctx.data, at + 4, off)?;
                    let child = resolve(ctx, child_stored, "b-tree child")?
                        .ok_or_else(|| SppError::Corrupt("b-tree entry without child".into()))?;
                    entries.push(BTreeEntry::Group { key, child });
                }
                BTreeKind::Chunk(rank) => {
                    let size = uint_at(ctx.data, at, 4)?;
                    let mask = uint_at(ctx.data, at + 4, 4)? as u32;
                    let mut coords = Vec::with_capacity(rank);
                    for k in 0..=rank {
                        let offset = uint_at(ctx.data, at + 8 + 8 * k as u64, 8)?;
                        if k < rank {
                            coords.push(offset);
                        }
                    }
                    let child_stored = uint_at(ctx.data, at + key_size, off)?;
                    // Interior addresses resolve at descent; leaf data at use.
                    // Undefined children are corrupt either way.
                    let child =
                        resolve(ctx, child_stored, "chunk index child")?.ok_or_else(|| {
                            SppError::Corrupt("chunk index entry without child".into())
                        })?;
                    entries.push(BTreeEntry::Chunk {
                        coords,
                        mask,
                        size,
                        child,
                    });
                }
            }
        }
        Ok(BTreeNode { level, entries })
    }
}
