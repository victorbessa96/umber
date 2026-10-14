//! MaterialX import, kill A: parse-only into a lossless IR + a typed
//! import report (`docs/research/materialx-roundtrip.md` §5.1 — the
//! contract; §2.4–2.6, §3.5, §4 are the hazards it answers).
//!
//! [`parse_mtlx`] reads the STANDARD MaterialX element shape (elements
//! named by node category, wired with `nodename=` / `nodegraph=` /
//! `output=`, free node names) into an [`MtlxDocument`]; [`emit_mtlx`]
//! writes it back. Nothing here evaluates, maps to a [`crate::Graph`],
//! consults a registry, touches the filesystem, follows an include or
//! upgrades a version (§5.1.2 "REFUSES"). Those are kills B/C, or never
//! (§5.4).
//!
//! # Why a second reader (and why no code shared with `mtlx.rs`)
//!
//! [`crate::mtlx::from_mtlx`] errors on every §1-shaped file (§2.6), and
//! its byte-identical round-trip tests forbid reshaping it (§5.1.1). The
//! duplicate-vs-reuse call for the tokenizer: DUPLICATED, and rewritten
//! rather than copied.
//! * `mtlx::Elem` cannot carry what a lossless IR needs. It stores
//!   attribute values unescaped (so `&apos;` and single-quoted values cannot
//!   re-emit byte-identically), and it drops text, comments and the
//!   self-closing form.
//! * `mtlx::Cursor` is private and recursive. Two of its behaviors would
//!   break real files here: its `<!DOCTYPE … [ … ]>` skip stops at the
//!   first `>` inside the internal subset, and its comment arm never fires,
//!   because both callers consume `<!` before `skip_bang` checks for `!--`,
//!   so a comment containing `>` ends early. This module's `Lexer`
//!   handles both.
//! * Only the element-depth constant is shared:
//!   `crate::mtlx::MAX_ELEM_DEPTH`.
//!
//! Deduplicating the two tokenizers is a deferred follow-up. It needs
//! `mtlx.rs` edits, and those are out of this kill's surface.
//!
//! # The IR (lossless by construction)
//!
//! * Attributes are kept as [`MtlxAttr`]: the RAW escaped bytes between the
//!   quotes plus the quote character, in source order. [`MtlxAttr::value`]
//!   decodes on access; emission writes the raw bytes. `value=` and file
//!   strings are therefore never parsed and reformatted. `&lt;UDIM&gt;`
//!   re-emits byte-exact, and the decoded view is the literal `<UDIM>` token
//!   (§4.3.1).
//! * Typed items ([`MtlxNode`], [`MtlxNodedef`], [`MtlxPort`],
//!   [`MtlxInclude`]) hold their verbatim [`MtlxElement`] and a parsed
//!   view (`name`, `category`, `ty`, [`MtlxInput`]s, [`MtlxConnection`]s,
//!   the scope chain). Emission reads ONLY the verbatim element. The view
//!   is a read model derived at parse time; kill A never mutates it.
//!   [`MtlxNodegraph`] owns its child items directly; its own tag is
//!   rebuilt from `attrs`.
//! * Nodes are keyed by their free `name` (§2.5.3). An `N<digits>` name is
//!   just a name. Input types are the `type=` string verbatim (§2.5.4): an
//!   untyped input has `ty: None`, never an assumed `color3`.
//! * Q6 (the scope data's home): the IR stores BOTH forms. It keeps the raw
//!   chain (`scope`: ancestor nodegraph names, outermost first), and on
//!   [`MtlxNode`] it adds the effective `fileprefix` / `colorspace` (the
//!   nearest ancestor-or-self value, unjoined and unapplied). The raw
//!   chain is what re-emit needs; the effective values are what kill C
//!   queries.
//! * Unknown elements (`<look>`, `<typedef>`, `<sticky>`, …) are kept
//!   verbatim as [`MtlxItem::Other`]. Comments, text, processing
//!   instructions and the DOCTYPE are kept as [`MtlxMisc`].
//!
//! # Canonical re-emit (what "byte-identical" means)
//!
//! [`emit_mtlx`] rebuilds the tree in the house layout both Umber writers
//! use:
//! * 2-space indentation, one element per line;
//! * one space before each attribute, in source order;
//! * ` />` for a self-closed element; an explicitly empty one (`<x></x>`)
//!   in the writers' block form (open line, then close line);
//! * an element whose only content is text stays inline
//!   (`<doc>text</doc>`);
//! * prolog items (XML declaration, comments, DOCTYPE) one per line,
//!   verbatim.
//!
//! A document already in that layout re-emits BYTE-IDENTICALLY. That
//! covers every `to_mtlx` / `to_mtlx_document` output and every
//! `tests/fixtures/mtlx` file, including the lossy hazards one.
//! Inter-tag whitespace in any other layout is normalized; no attribute,
//! value, comment or text byte ever is. Emission is idempotent.
//!
//! # Connections (§4.5.4, both `output=` spellings)
//!
//! `output=` is classified by FORM:
//! * With no `nodename=` / `nodegraph=` source, `output="N<digits>"` is
//!   Umber's legacy painter edge ([`MtlxConnection::LegacyEdge`]).
//! * With a source present, `output=` is a port select on that source,
//!   even when it reads `N1`. Port names are `[A-Za-z_][A-Za-z0-9_:]*`.
//! * A sourceless port name is [`MtlxConnection::PortOnly`] (reported).
//! * Anything else (`output="2"`) is [`MtlxImportError::BadOutputRef`].
//!
//! References resolve by name within the enclosing scope:
//! * `nodename=` against sibling nodes;
//! * `nodegraph=` against sibling or root-level nodegraphs;
//! * `interfacename=` against the enclosing nodegraph's `<input>`s plus the
//!   inputs of the nodedef it implements. The root has no interface.
//!
//! An unresolved reference is a report entry, never an error, and is never
//! dropped.
//!
//! # Resource caps (§3.5.3), each a typed error naming the limit
//!
//! * Element depth: `crate::mtlx::MAX_ELEM_DEPTH` (256), shared with the
//!   painter reader. `Lexer` is an EXPLICIT-STACK tokenizer (no
//!   recursion at all); the cap bounds memory and the depth of the later
//!   passes. Violations are [`MtlxImportError::ElementDepthExceeded`].
//! * Nodegraph nesting: [`MAX_NODEGRAPH_DEPTH`] = 64 (§4.2.2), via
//!   COUNTED recursion. The IR builder recurses once per `<nodegraph>`
//!   and checks the count before descending. Rationale: no §1 practice
//!   shape exceeds 2, and the vendored OpenPBR nests 1. A root-level
//!   nodegraph is depth 1. Violations are
//!   [`MtlxImportError::DepthExceeded`]. This cap is separate from, and
//!   stricter than, the element cap.
//! * Inputs per element: [`MAX_INPUTS_PER_ELEMENT`] = 1024 `<input>`
//!   children on one node, nodedef or nodegraph. Rationale: the vendored
//!   OpenPBR nodedef, the widest real interface in the tree, declares
//!   under 100. 1024 is an order of magnitude of headroom, and it bounds
//!   the per-node mapping work kill B will do. Violations are
//!   [`MtlxImportError::TooManyInputs`].
//! * Document size: [`MAX_DOCUMENT_BYTES`] = 16 MiB, checked before any
//!   tokenizing. That makes it the whole parse's allocation bound.
//!   Rationale: real `.mtlx` files are KB-scale, and the full stdlib
//!   libraries are hundreds of KB. Violations are
//!   [`MtlxImportError::DocumentTooLarge`].
//!
//! [`parse_mtlx`] never panics: no `unwrap` / `expect`, and every source
//! slice goes through `str::get`.
//!
//! # The import report (§3.5.2)
//!
//! [`ImportReport`] holds one [`ReportEntry`] per finding: element path,
//! source line, [`Severity`], [`Disposition`], reason and section cite. It
//! also holds a [`ReportSummary`] (element census + counts by severity and
//! disposition).
//!
//! Entries are in DOCUMENT ORDER, built in this sequence:
//! 1. prolog;
//! 2. root kind-specific entries, then root attributes;
//! 3. each element's kind-specific entries, then its attributes in source
//!    order, then one aggregated metadata entry, then its children;
//! 4. epilog.
//!
//! There are no maps with nondeterministic iteration in the walk.
//! Re-parsing an unchanged file therefore yields a byte-identical
//! [`ImportReport::render`].
//!
//! Paths are `/tag[name]` segments; an unnamed element is a bare `/tag`.
//! Root attributes report at `/materialx`; prolog and epilog items at
//! `(prolog)` and `(epilog)`.
//!
//! Kill A has two of the §3.1.2 dispositions:
//! * PRESERVE: carried verbatim, uninterpreted.
//! * REPORT: carried verbatim, with semantics this kill refuses to apply
//!   (follow, join, upgrade, resolve).
//!
//! IMPORT arrives with kill B. Severity is `info` (expected, nothing at
//! risk) or `warning` (data present that a consumer could misread).
//!
//! | construct | severity / disposition | cite |
//! |---|---|---|
//! | root `version` 1.38 / 1.39 / 1.39.x | info / report | §4.1.2 |
//! | root `version` other | warning / report | §4.1.2 |
//! | root `version` absent | warning / report | §4.1.1 |
//! | `colorspace` in {`srgb_texture`, `lin_rec709`, `none`} (any element) | info / report | §4.4.2 |
//! | `colorspace` other | warning / report | §4.4.2 |
//! | `cms`, `cmsconfig` | warning / report | §4.4.2 |
//! | `namespace` | warning / report | §4.5.2 |
//! | `xmlns`, `xmlns:*` | info / preserve | §4.5.2 |
//! | `fileprefix` | warning / report | §4.3.2 |
//! | `<xi:include>` (never followed) | warning / report | §4.5.1 |
//! | DOCTYPE (entities never expanded) | warning / preserve | §2.6(i) |
//! | non-whitespace text content | warning / preserve | §2.6(k) |
//! | unknown element (kept verbatim) | warning / preserve | §3.5.1 |
//! | duplicate name in one scope | warning / report | §2.6(e) |
//! | missing `name=` / `node=` / `type=` | warning / report | §2.5.3, §2.5.4, §2.5.6 |
//! | unresolved `nodename=` / `nodegraph=` / `interfacename=` | warning / report | §4.2.3 |
//! | both `value=` and a connection; both `nodename=` and `nodegraph=` | warning / report | §2.5.2 |
//! | legacy `output="N<id>"` edge: resolved / unresolved | info / report, warning / report | §4.5.4 |
//! | sourceless port select | warning / report | §4.5.4 |
//! | `member=` | warning / preserve | §4.5.6 |
//! | coordinate-binding node (`geompropvalue*`, `geomcolor`, `texcoord`, `UsdPrimvarReader*`) | warning / report | §4.3.4 |
//! | painter-dialect `<node type=…>` | info / preserve | §2.5.1 |
//! | nodedef `version` (+ `isdefaultversion`), node `version` request, `nodedef=` binding | info / report | §3.4.3 |
//! | nodedef implementation found / declaration only | info / report | §3.4.1 |
//! | nodegraph implementing a nodedef | info / report | §3.4.1 |
//! | inlined nodegraph (no `nodedef=`) | info / report | §1.3.2 |
//! | `defaultgeomprop` | info / report | §4.3.4 |
//! | UI/doc metadata (`doc`, `ui*`, `nodegroup`, …), one entry per element | info / preserve | §4.5.6 |
//! | any other attribute | info / preserve | §4.1.3 |
//! | processing instruction other than the XML declaration | info / preserve | §3.5.1 |
//!
//! Resolved connections, typed inputs and plain values produce no entry.
//! A shape Umber's own writers emit therefore reports only `info`; the
//! self-hosting test pins this (§5.1.3).
//!
//! The recognized colorspace set is exactly §4.4.2's: the names Umber
//! writes, plus `none`. `raw`, `acescg` and the scene-suffixed names are
//! CMS-relative strings this kill cannot vouch for, so they warn.
//!
//! # v1 boundaries / deferrals
//!
//! These are deliberately left out and named in the kill report:
//! * `[interface token]` filename substitution (kill B);
//! * validating `output=` against the named source's declared outputs
//!   (kill B; needs stdlib signatures, §7 Q2);
//! * namespace-collision analysis (no flattening happens, so no collision
//!   can occur yet);
//! * real-corpus validation (§7 Q1; no corpus is in the tree).
//!
//! `stringarray` unescaping (§4.5.5) is available through
//! [`MtlxInput::string_array`]. The float-vector path in `mtlx.rs` is
//! untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::mtlx::MAX_ELEM_DEPTH;

/// Nodegraph nesting cap (§4.2.2). See the module docs for the rationale.
pub const MAX_NODEGRAPH_DEPTH: usize = 64;

/// `<input>` children allowed on one node / nodedef / nodegraph (§3.5.3).
pub const MAX_INPUTS_PER_ELEMENT: usize = 1024;

/// Source size cap in bytes (§3.5.3): 16 MiB.
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

/// The colorspace names this kill recognizes (§4.4.2): Umber's two
/// written names plus `none`.
const RECOGNIZED_COLORSPACES: &[&str] = &["srgb_texture", "lin_rec709", "none"];

/// Element tags that are never node instances even when they carry
/// `type=` (MaterialX document-level / look / definition elements).
const NON_NODE_ELEMENTS: &[&str] = &[
    "look",
    "lookgroup",
    "materialassign",
    "visibility",
    "collection",
    "geominfo",
    "geomprop",
    "geompropdef",
    "typedef",
    "member",
    "implementation",
    "attributedef",
    "targetdef",
    "unittypedef",
    "unitdef",
    "variant",
    "variantset",
    "variantassign",
    "propertyset",
    "propertysetassign",
    "property",
    "backdrop",
];

/// Attributes that are UI / documentation metadata (§4.5.6): preserved,
/// aggregated into one info entry per element.
const METADATA_ATTRS: &[&str] = &[
    "doc",
    "uiname",
    "uifolder",
    "uimin",
    "uimax",
    "uisoftmin",
    "uisoftmax",
    "uistep",
    "uiadvanced",
    "uivisible",
    "uicolor",
    "xpos",
    "ypos",
    "width",
    "height",
    "hint",
    "enum",
    "enumvalues",
    "unit",
    "unittype",
    "uniform",
    "nodegroup",
    "target",
    "inherit",
];

/// Errors from [`parse_mtlx`]. Every resource cap names its limit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MtlxImportError {
    /// The source is over [`MAX_DOCUMENT_BYTES`].
    #[error("mtlx import: document is {bytes} bytes; the cap is {cap}")]
    DocumentTooLarge { bytes: usize, cap: usize },
    /// Element nesting over the tokenizer cap (`crate::mtlx::MAX_ELEM_DEPTH`,
    /// 256).
    #[error("mtlx import: element nesting depth {depth} at line {line} exceeds the cap of {cap}")]
    ElementDepthExceeded {
        depth: usize,
        cap: usize,
        line: usize,
    },
    /// Nodegraph nesting over [`MAX_NODEGRAPH_DEPTH`] (§4.2.2).
    #[error("mtlx import: nodegraph nesting depth {depth} exceeds the cap of {cap}")]
    DepthExceeded { depth: usize, cap: usize },
    /// More than [`MAX_INPUTS_PER_ELEMENT`] `<input>` children.
    #[error("mtlx import: {element} (line {line}) has {count} inputs; the cap is {cap}")]
    TooManyInputs {
        element: String,
        line: usize,
        count: usize,
        cap: usize,
    },
    /// Malformed XML, with the 1-based line of the failure.
    #[error("mtlx import: syntax error at line {line}: {message}")]
    Syntax { line: usize, message: String },
    /// The root element is not `<materialx>`.
    #[error("mtlx import: expected a <materialx> root at line {line}, found <{found}>")]
    NotMaterialx { line: usize, found: String },
    /// An `output=` that is neither a legacy `N<digits>` edge nor a port
    /// name (§4.5.4).
    #[error(
        "mtlx import: input '{input}' at line {line}: output=\"{value}\" is neither N<digits> nor a port name"
    )]
    BadOutputRef {
        input: String,
        line: usize,
        value: String,
    },
}

/// One attribute, verbatim: `raw` is the escaped text between the quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxAttr {
    pub name: String,
    pub raw: String,
    /// `'"'` or `'\''`, as written.
    pub quote: char,
}

impl MtlxAttr {
    /// The decoded value (`&lt;` → `<`, numeric references, …; an unknown
    /// entity stays literal — DOCTYPE entities are never expanded).
    pub fn value(&self) -> String {
        decode_entities(&self.raw)
    }
}

/// What a [`MtlxMisc`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscKind {
    /// `<!-- … -->`.
    Comment,
    /// Non-whitespace character data (trailing whitespace trimmed), or a
    /// CDATA section verbatim.
    Text,
    /// `<? … ?>`.
    Pi,
    /// `<!DOCTYPE … >` including its internal subset (prolog only).
    Doctype,
}

/// Non-element content, verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxMisc {
    pub kind: MiscKind,
    pub raw: String,
    pub line: usize,
}

/// A generic element, verbatim (attributes raw, children in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxElement {
    pub tag: String,
    pub attrs: Vec<MtlxAttr>,
    pub children: Vec<MtlxContent>,
    /// `<x />` (true) vs an open/close pair (false); meaningful only when
    /// empty.
    pub self_closing: bool,
    pub line: usize,
}

impl MtlxElement {
    /// The first attribute named `key`.
    pub fn attr(&self, key: &str) -> Option<&MtlxAttr> {
        self.attrs.iter().find(|a| a.name == key)
    }

    /// The decoded value of the first attribute named `key`.
    pub fn attr_value(&self, key: &str) -> Option<String> {
        self.attr(key).map(MtlxAttr::value)
    }
}

/// Element content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtlxContent {
    Element(MtlxElement),
    Misc(MtlxMisc),
}

/// How an input is wired (§2.5.1, §4.5.4). Names are kept as names —
/// nothing is resolved to an index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtlxConnection {
    /// `nodename="node"`, optionally with `output="port"`.
    Node {
        node: String,
        output: Option<String>,
    },
    /// `nodegraph="graph"`, optionally with `output="port"`.
    Graph {
        graph: String,
        output: Option<String>,
    },
    /// Umber's painter-dialect edge: `output="N<id>"` with no source.
    LegacyEdge { id: u64 },
    /// A port select with no `nodename=` / `nodegraph=` source.
    PortOnly { output: String },
}

/// The parsed view of an `<input>` / `<output>` / `<token>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxInput {
    /// `name=` decoded (`""` when absent — reported).
    pub name: String,
    /// `type=` verbatim (decoded); `None` = unknown, never assumed.
    pub ty: Option<String>,
    /// `value=` decoded (a `<UDIM>` token reads back literally).
    pub value: Option<String>,
    pub connection: Option<MtlxConnection>,
    pub interfacename: Option<String>,
    pub colorspace: Option<String>,
    pub member: Option<String>,
    pub line: usize,
}

impl MtlxInput {
    /// The §4.5.5 split of a `stringarray` value: elements separate on `,`;
    /// `\,` and `\;` are literal. `None` unless `type="stringarray"` with a
    /// value.
    pub fn string_array(&self) -> Option<Vec<String>> {
        if self.ty.as_deref() != Some("stringarray") {
            return None;
        }
        self.value.as_deref().map(split_string_array)
    }
}

/// A node instance (any category element carrying `type=`, or the
/// painter dialect's `<node>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxNode {
    /// The source element — what [`emit_mtlx`] writes.
    pub element: MtlxElement,
    /// The element tag verbatim (`image`, `open_pbr_surface`, `node`, …).
    pub category: String,
    /// The free `name=` (§2.5.3); `""` when absent (reported).
    pub name: String,
    /// The output `type=`.
    pub ty: Option<String>,
    /// A requested nodedef `version=` (§3.4.3), kept as data.
    pub version: Option<String>,
    /// Ancestor nodegraph names, outermost first (§4.2.3).
    pub scope: Vec<String>,
    /// Effective (nearest ancestor-or-self) `fileprefix`, never joined.
    pub fileprefix: Option<String>,
    /// Effective (nearest ancestor-or-self) `colorspace`, never applied.
    pub colorspace: Option<String>,
    pub inputs: Vec<MtlxInput>,
}

/// A `<nodedef>`: identity + interface + version + body (§2.5.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxNodedef {
    pub element: MtlxElement,
    pub name: String,
    /// The `node=` category it defines.
    pub node: Option<String>,
    pub version: Option<String>,
    pub isdefaultversion: Option<String>,
    pub inputs: Vec<MtlxInput>,
    pub outputs: Vec<MtlxInput>,
    /// Names of the root-level `<nodegraph>` / `<implementation>` elements
    /// whose `nodedef=` names this nodedef, in document order — the body,
    /// preserved opaque (those items live in the document tree).
    pub implementations: Vec<String>,
}

/// A `<nodegraph>` scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxNodegraph {
    pub name: String,
    /// The nodedef it implements (`None` = inlined group contents, §1.3.2).
    pub nodedef: Option<String>,
    /// Ancestor nodegraph names (not including this one).
    pub scope: Vec<String>,
    /// 1 for a root-level nodegraph.
    pub depth: usize,
    pub attrs: Vec<MtlxAttr>,
    pub items: Vec<MtlxItem>,
    pub self_closing: bool,
    pub line: usize,
}

/// A scope-level `<input>` / `<output>` / `<token>` (a nodegraph's
/// interface or outputs, or a document-level output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxPort {
    pub element: MtlxElement,
    pub scope: Vec<String>,
    pub port: MtlxInput,
}

/// An `<xi:include>` — recorded, never followed (§4.5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxInclude {
    pub element: MtlxElement,
    pub href: Option<String>,
}

/// One child of the root or of a nodegraph, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtlxItem {
    Node(MtlxNode),
    Nodedef(MtlxNodedef),
    Nodegraph(MtlxNodegraph),
    Port(MtlxPort),
    Include(MtlxInclude),
    /// An element this kill does not interpret, verbatim.
    Other(MtlxElement),
    Misc(MtlxMisc),
}

/// The lossless IR of one `.mtlx` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlxDocument {
    /// The source began with a UTF-8 byte-order mark.
    pub bom: bool,
    /// Items before the root (XML declaration, comments, DOCTYPE).
    pub prolog: Vec<MtlxMisc>,
    /// The root `version=` decoded (also present in `root_attrs`).
    pub version: Option<String>,
    /// Every root attribute, in source order, verbatim.
    pub root_attrs: Vec<MtlxAttr>,
    pub root_self_closing: bool,
    pub root_line: usize,
    pub items: Vec<MtlxItem>,
    /// Comments / processing instructions after the root.
    pub epilog: Vec<MtlxMisc>,
}

impl MtlxDocument {
    /// Every item, pre-order (a nodegraph precedes its contents).
    pub fn all_items(&self) -> Vec<&MtlxItem> {
        let mut out = Vec::new();
        flatten_items(&self.items, &mut out);
        out
    }

    /// Every node in document order, across all nodegraph scopes.
    pub fn nodes(&self) -> Vec<&MtlxNode> {
        self.all_items()
            .into_iter()
            .filter_map(|i| match i {
                MtlxItem::Node(n) => Some(n),
                _ => None,
            })
            .collect()
    }

    /// The first node (document order) named `name`.
    pub fn node(&self, name: &str) -> Option<&MtlxNode> {
        self.nodes().into_iter().find(|n| n.name == name)
    }

    /// Every nodedef in document order.
    pub fn nodedefs(&self) -> Vec<&MtlxNodedef> {
        self.all_items()
            .into_iter()
            .filter_map(|i| match i {
                MtlxItem::Nodedef(d) => Some(d),
                _ => None,
            })
            .collect()
    }

    /// Every nodegraph, pre-order.
    pub fn nodegraphs(&self) -> Vec<&MtlxNodegraph> {
        self.all_items()
            .into_iter()
            .filter_map(|i| match i {
                MtlxItem::Nodegraph(g) => Some(g),
                _ => None,
            })
            .collect()
    }
}

fn flatten_items<'a>(items: &'a [MtlxItem], out: &mut Vec<&'a MtlxItem>) {
    for item in items {
        out.push(item);
        if let MtlxItem::Nodegraph(g) = item {
            flatten_items(&g.items, out);
        }
    }
}

// ---------------------------------------------------------------------------
// The import report (§3.5.2).
// ---------------------------------------------------------------------------

/// Entry severity. `Info < Warning`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
        }
    }
}

/// The kill-A dispositions (§3.1.2; IMPORT lands with kill B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Disposition {
    /// Carried verbatim, uninterpreted.
    Preserve,
    /// Carried verbatim; its semantics are named and refused.
    Report,
}

impl Disposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Report => "report",
        }
    }
}

/// One report line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportEntry {
    /// `/tag[name]/…`, `/materialx`, `(prolog)` or `(epilog)`.
    pub path: String,
    pub line: usize,
    pub severity: Severity,
    pub disposition: Disposition,
    pub reason: String,
    /// The `docs/research/materialx-roundtrip.md` section.
    pub cite: &'static str,
}

impl fmt::Display for ReportEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} L{} {}: {} [{}]",
            self.severity.as_str(),
            self.disposition.as_str(),
            self.line,
            self.path,
            self.reason,
            self.cite
        )
    }
}

/// Element census + entry counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportSummary {
    pub nodes: usize,
    pub nodedefs: usize,
    pub nodegraphs: usize,
    pub ports: usize,
    pub includes: usize,
    pub other: usize,
    pub info: usize,
    pub warning: usize,
    pub preserve: usize,
    pub report: usize,
}

impl fmt::Display for ReportSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "summary: nodes {}, nodedefs {}, nodegraphs {}, ports {}, includes {}, other {}; \
             info {}, warning {}; preserve {}, report {}",
            self.nodes,
            self.nodedefs,
            self.nodegraphs,
            self.ports,
            self.includes,
            self.other,
            self.info,
            self.warning,
            self.preserve,
            self.report
        )
    }
}

/// The machine-readable import report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub entries: Vec<ReportEntry>,
    pub summary: ReportSummary,
}

impl ImportReport {
    /// One line per entry, then the summary line; `\n`-terminated.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&e.to_string());
            out.push('\n');
        }
        out.push_str(&self.summary.to_string());
        out.push('\n');
        out
    }

    /// The highest entry severity (`None` for an empty report).
    pub fn max_severity(&self) -> Option<Severity> {
        self.entries.iter().map(|e| e.severity).max()
    }
}

// ---------------------------------------------------------------------------
// Entry points.
// ---------------------------------------------------------------------------

/// Parse a MaterialX document into the lossless IR plus its import
/// report. Never panics; every failure is a typed [`MtlxImportError`].
pub fn parse_mtlx(src: &str) -> Result<(MtlxDocument, ImportReport), MtlxImportError> {
    if src.len() > MAX_DOCUMENT_BYTES {
        return Err(MtlxImportError::DocumentTooLarge {
            bytes: src.len(),
            cap: MAX_DOCUMENT_BYTES,
        });
    }
    let lexed = lex(src)?;
    let mut b = Builder::default();
    for m in &lexed.prolog {
        b.outer_misc_entry(m, "(prolog)");
    }
    let root = lexed.root;
    let root_path = "/materialx";
    b.prescan_root(&root.children);
    let version = root.attr_value("version");
    b.version_entry(version.as_deref(), root.line);
    b.attr_entries(root_path, root.line, &root.attrs, &["version"]);
    let (nodes, graphs, _) = prescan_scope(&root.children);
    let scope = ScopeCtx {
        path: String::new(),
        chain: Vec::new(),
        depth: 0,
        nodes,
        graphs,
        interface: None,
        fileprefix: root.attr_value("fileprefix"),
        colorspace: root.attr_value("colorspace"),
    };
    let MtlxElement {
        attrs,
        children,
        self_closing,
        line,
        ..
    } = root;
    let items = b.build_items(children, &scope, root_path)?;
    for m in &lexed.epilog {
        b.outer_misc_entry(m, "(epilog)");
    }
    let doc = MtlxDocument {
        bom: lexed.bom,
        prolog: lexed.prolog,
        version,
        root_attrs: attrs,
        root_self_closing: self_closing,
        root_line: line,
        items,
        epilog: lexed.epilog,
    };
    Ok((doc, b.finish()))
}

/// Re-emit the IR (canonical layout — see the module docs). Byte-identical
/// to the source for documents already in the house layout.
pub fn emit_mtlx(doc: &MtlxDocument) -> String {
    let mut out = String::new();
    if doc.bom {
        out.push('\u{feff}');
    }
    for m in &doc.prolog {
        out.push_str(&m.raw);
        out.push('\n');
    }
    emit_container(
        &mut out,
        0,
        "materialx",
        &doc.root_attrs,
        doc.root_self_closing,
        &doc.items,
    );
    for m in &doc.epilog {
        out.push_str(&m.raw);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Emission.
// ---------------------------------------------------------------------------

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn open_tag(out: &mut String, depth: usize, tag: &str, attrs: &[MtlxAttr]) {
    indent(out, depth);
    out.push('<');
    out.push_str(tag);
    for a in attrs {
        out.push(' ');
        out.push_str(&a.name);
        out.push('=');
        out.push(a.quote);
        out.push_str(&a.raw);
        out.push(a.quote);
    }
}

/// An element with no content: ` />` when self-closed, else the house
/// writers' block form (open line + close line, as `to_mtlx` writes an
/// empty graph / param-less node / input-less nodedef).
fn close_empty(out: &mut String, depth: usize, tag: &str, self_closing: bool) {
    if self_closing {
        out.push_str(" />\n");
    } else {
        out.push_str(">\n");
        close_block(out, depth, tag);
    }
}

fn close_inline_text(out: &mut String, tag: &str, text: &str) {
    out.push('>');
    out.push_str(text);
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

fn close_block(out: &mut String, depth: usize, tag: &str) {
    indent(out, depth);
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

fn emit_misc(out: &mut String, depth: usize, m: &MtlxMisc) {
    indent(out, depth);
    out.push_str(&m.raw);
    out.push('\n');
}

fn emit_container(
    out: &mut String,
    depth: usize,
    tag: &str,
    attrs: &[MtlxAttr],
    self_closing: bool,
    items: &[MtlxItem],
) {
    open_tag(out, depth, tag, attrs);
    match items {
        [] => close_empty(out, depth, tag, self_closing),
        [MtlxItem::Misc(m)] if m.kind == MiscKind::Text => close_inline_text(out, tag, &m.raw),
        _ => {
            out.push_str(">\n");
            for item in items {
                emit_item(out, depth + 1, item);
            }
            close_block(out, depth, tag);
        }
    }
}

fn emit_item(out: &mut String, depth: usize, item: &MtlxItem) {
    match item {
        MtlxItem::Node(n) => emit_element(out, depth, &n.element),
        MtlxItem::Nodedef(d) => emit_element(out, depth, &d.element),
        MtlxItem::Nodegraph(g) => {
            emit_container(out, depth, "nodegraph", &g.attrs, g.self_closing, &g.items)
        }
        MtlxItem::Port(p) => emit_element(out, depth, &p.element),
        MtlxItem::Include(i) => emit_element(out, depth, &i.element),
        MtlxItem::Other(e) => emit_element(out, depth, e),
        MtlxItem::Misc(m) => emit_misc(out, depth, m),
    }
}

fn emit_element(out: &mut String, depth: usize, e: &MtlxElement) {
    open_tag(out, depth, &e.tag, &e.attrs);
    match e.children.as_slice() {
        [] => close_empty(out, depth, &e.tag, e.self_closing),
        [MtlxContent::Misc(m)] if m.kind == MiscKind::Text => {
            close_inline_text(out, &e.tag, &m.raw)
        }
        children => {
            out.push_str(">\n");
            for c in children {
                match c {
                    MtlxContent::Element(child) => emit_element(out, depth + 1, child),
                    MtlxContent::Misc(m) => emit_misc(out, depth + 1, m),
                }
            }
            close_block(out, depth, &e.tag);
        }
    }
}

// ---------------------------------------------------------------------------
// Value helpers.
// ---------------------------------------------------------------------------

/// Decode the five predefined XML entities and numeric character
/// references; anything else (incl. DOCTYPE-declared entities) stays
/// literal.
fn decode_entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(rest.get(..amp).unwrap_or_default());
        let tail = rest.get(amp..).unwrap_or_default();
        let decoded = tail
            .find(';')
            .filter(|&semi| semi <= 12)
            .and_then(|semi| entity(tail.get(1..semi)?).map(|c| (c, semi)));
        match decoded {
            Some((c, semi)) => {
                out.push(c);
                rest = tail.get(semi + 1..).unwrap_or_default();
            }
            None => {
                out.push('&');
                rest = tail.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => num.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// §4.5.5: split on `,`; `\,` / `\;` are literal; elements are trimmed.
fn split_string_array(value: &str) -> Vec<String> {
    if value.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some(',' | ';')) => {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
            ',' => {
                out.push(cur.trim().to_owned());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_owned());
    out
}

/// `N<digits>` → the legacy painter id.
fn legacy_id(output: &str) -> Option<u64> {
    let digits = output.strip_prefix('N')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// `[A-Za-z_][A-Za-z0-9_:]*`.
fn is_port_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b':')
}

fn is_coordinate_binding(category: &str) -> bool {
    category.starts_with("geompropvalue")
        || category == "geomcolor"
        || category == "texcoord"
        || category.starts_with("UsdPrimvarReader")
}

fn segment(tag: &str, name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{tag}[{n}]"),
        None => tag.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// The tokenizer: explicit stack, never recursive, never panics.
// ---------------------------------------------------------------------------

/// The tokenizer's output: the root element plus the out-of-root misc.
struct Lexed {
    bom: bool,
    prolog: Vec<MtlxMisc>,
    root: MtlxElement,
    epilog: Vec<MtlxMisc>,
}

struct Lexer<'a> {
    src: &'a str,
    b: &'a [u8],
    pos: usize,
    /// 1-based line of `pos`.
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str, start: usize) -> Self {
        Self {
            src,
            b: src.as_bytes(),
            pos: start,
            line: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.b.get(self.pos).copied()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn at(&self, s: &str) -> bool {
        self.b
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(s.as_bytes()))
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.bump();
        }
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, MtlxImportError> {
        Err(MtlxImportError::Syntax {
            line: self.line,
            message: message.into(),
        })
    }

    fn slice(&self, start: usize, end: usize) -> Result<String, MtlxImportError> {
        match self.src.get(start..end) {
            Some(s) => Ok(s.to_owned()),
            None => self.err("slice is not on a UTF-8 boundary"),
        }
    }

    fn expect(&mut self, s: &str, what: &str) -> Result<(), MtlxImportError> {
        if self.at(s) {
            self.advance(s.len());
            Ok(())
        } else {
            self.err(format!("expected {what}"))
        }
    }

    fn read_name(&mut self) -> Result<String, MtlxImportError> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':')
        ) {
            self.bump();
        }
        if self.pos == start {
            return self.err("expected a name");
        }
        self.slice(start, self.pos)
    }

    /// The raw (still escaped) text between the quotes, and the quote.
    fn read_attr_value(&mut self) -> Result<(String, char), MtlxImportError> {
        let (qb, quote) = match self.peek() {
            Some(b'"') => (b'"', '"'),
            Some(b'\'') => (b'\'', '\''),
            _ => return self.err("expected a quoted attribute value"),
        };
        self.bump();
        let start = self.pos;
        loop {
            match self.bump() {
                None => return self.err("unterminated attribute value"),
                Some(c) if c == qb => break,
                Some(_) => {}
            }
        }
        Ok((self.slice(start, self.pos - 1)?, quote))
    }

    /// Consume through `terminator`; returns `src[start..]` up to and
    /// including it.
    fn read_until(
        &mut self,
        start: usize,
        terminator: &str,
        what: &str,
    ) -> Result<String, MtlxImportError> {
        loop {
            if self.at(terminator) {
                self.advance(terminator.len());
                return self.slice(start, self.pos);
            }
            if self.bump().is_none() {
                return self.err(format!("unterminated {what}"));
            }
        }
    }

    /// `<!DOCTYPE … [ … ] >`: `>` ends it only outside quotes and outside
    /// the bracketed internal subset.
    fn read_doctype(&mut self, start: usize) -> Result<String, MtlxImportError> {
        self.advance(2);
        let mut brackets = 0usize;
        let mut quote: Option<u8> = None;
        loop {
            let Some(c) = self.bump() else {
                return self.err("unterminated DOCTYPE");
            };
            match quote {
                Some(q) => {
                    if c == q {
                        quote = None;
                    }
                }
                None => match c {
                    b'"' | b'\'' => quote = Some(c),
                    b'[' => brackets += 1,
                    b']' => brackets = brackets.saturating_sub(1),
                    b'>' if brackets == 0 => return self.slice(start, self.pos),
                    _ => {}
                },
            }
        }
    }

    /// At `<?` or `<!`: a processing instruction, comment, or (when
    /// `allow_doctype`) the DOCTYPE.
    fn read_markup(&mut self, allow_doctype: bool) -> Result<MtlxMisc, MtlxImportError> {
        let start = self.pos;
        let line = self.line;
        let (kind, raw) = if self.at("<?") {
            self.advance(2);
            (
                MiscKind::Pi,
                self.read_until(start, "?>", "processing instruction")?,
            )
        } else if self.at("<!--") {
            self.advance(4);
            (MiscKind::Comment, self.read_until(start, "-->", "comment")?)
        } else if self.at("<![CDATA[") {
            self.advance(9);
            (
                MiscKind::Text,
                self.read_until(start, "]]>", "CDATA section")?,
            )
        } else if allow_doctype && self.at("<!DOCTYPE") {
            (MiscKind::Doctype, self.read_doctype(start)?)
        } else {
            return self.err("unsupported markup declaration");
        };
        Ok(MtlxMisc { kind, raw, line })
    }

    /// At `<name`: the tag and its attributes; `self_closing` set for `/>`.
    fn read_open_tag(&mut self) -> Result<MtlxElement, MtlxImportError> {
        let line = self.line;
        self.advance(1);
        let tag = self.read_name()?;
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return self.err(format!("unterminated <{tag}>")),
                Some(b'/') => {
                    self.bump();
                    if self.peek() != Some(b'>') {
                        return self.err("expected '/>'");
                    }
                    self.bump();
                    return Ok(MtlxElement {
                        tag,
                        attrs,
                        children: Vec::new(),
                        self_closing: true,
                        line,
                    });
                }
                Some(b'>') => {
                    self.bump();
                    return Ok(MtlxElement {
                        tag,
                        attrs,
                        children: Vec::new(),
                        self_closing: false,
                        line,
                    });
                }
                Some(_) => {
                    let name = self.read_name()?;
                    self.skip_ws();
                    self.expect("=", "'='")?;
                    self.skip_ws();
                    let (raw, quote) = self.read_attr_value()?;
                    attrs.push(MtlxAttr { name, raw, quote });
                }
            }
        }
    }
}

fn push_child(stack: &mut [MtlxElement], content: MtlxContent) {
    if let Some(top) = stack.last_mut() {
        top.children.push(content);
    }
}

fn lex(src: &str) -> Result<Lexed, MtlxImportError> {
    let bom = src.starts_with('\u{feff}');
    let mut lx = Lexer::new(src, if bom { '\u{feff}'.len_utf8() } else { 0 });
    let mut prolog = Vec::new();
    let mut epilog = Vec::new();
    let mut root: Option<MtlxElement> = None;
    let mut stack: Vec<MtlxElement> = Vec::new();
    loop {
        lx.skip_ws();
        if stack.is_empty() {
            // Outside the root element.
            let Some(c) = lx.peek() else { break };
            if c != b'<' {
                return lx.err(if root.is_some() {
                    "text after the root element"
                } else {
                    "text before the root element"
                });
            }
            if lx.at("<?") || lx.at("<!") {
                if lx.at("<![CDATA[") {
                    return lx.err("CDATA outside the root element");
                }
                let misc = lx.read_markup(root.is_none())?;
                if root.is_none() {
                    prolog.push(misc);
                } else {
                    epilog.push(misc);
                }
                continue;
            }
            if lx.at("</") {
                return lx.err("closing tag with no open element");
            }
            if root.is_some() {
                return lx.err("multiple root elements");
            }
            let elem = lx.read_open_tag()?;
            if elem.tag != "materialx" {
                return Err(MtlxImportError::NotMaterialx {
                    line: elem.line,
                    found: elem.tag,
                });
            }
            if elem.self_closing {
                root = Some(elem);
            } else {
                stack.push(elem);
            }
            continue;
        }
        // Inside an element.
        let start = lx.pos;
        let line = lx.line;
        match lx.peek() {
            None => {
                let tag = stack.last().map(|e| e.tag.as_str()).unwrap_or_default();
                return lx.err(format!("unclosed <{tag}> (reached end of input)"));
            }
            Some(b'<') if lx.at("</") => {
                lx.advance(2);
                let close = lx.read_name()?;
                lx.skip_ws();
                lx.expect(">", "'>'")?;
                let Some(done) = stack.pop() else {
                    return lx.err("closing tag with no open element");
                };
                if done.tag != close {
                    let open = done.tag;
                    return lx.err(format!("mismatched close </{close}> for <{open}>"));
                }
                match stack.last_mut() {
                    Some(parent) => parent.children.push(MtlxContent::Element(done)),
                    None => root = Some(done),
                }
            }
            Some(b'<') if lx.at("<?") || lx.at("<!--") || lx.at("<![CDATA[") => {
                let misc = lx.read_markup(false)?;
                push_child(&mut stack, MtlxContent::Misc(misc));
            }
            Some(b'<') if lx.at("<!") => {
                return lx.err("markup declaration inside an element");
            }
            Some(b'<') => {
                let elem = lx.read_open_tag()?;
                let depth = stack.len() + 1;
                if depth > MAX_ELEM_DEPTH {
                    return Err(MtlxImportError::ElementDepthExceeded {
                        depth,
                        cap: MAX_ELEM_DEPTH,
                        line: elem.line,
                    });
                }
                if elem.self_closing {
                    push_child(&mut stack, MtlxContent::Element(elem));
                } else {
                    stack.push(elem);
                }
            }
            Some(_) => {
                while !matches!(lx.peek(), Some(b'<') | None) {
                    lx.bump();
                }
                let raw = lx.slice(start, lx.pos)?.trim_end().to_owned();
                let misc = MtlxMisc {
                    kind: MiscKind::Text,
                    raw,
                    line,
                };
                push_child(&mut stack, MtlxContent::Misc(misc));
            }
        }
    }
    match root {
        Some(root) => Ok(Lexed {
            bom,
            prolog,
            root,
            epilog,
        }),
        None => Err(MtlxImportError::Syntax {
            line: lx.line,
            message: "empty document".into(),
        }),
    }
}

// ---------------------------------------------------------------------------
// The IR builder: counted recursion per nodegraph, entries in document
// order.
// ---------------------------------------------------------------------------

/// How a root / nodegraph child element is read.
enum Kind {
    Nodegraph,
    Nodedef,
    Port,
    Include,
    Other,
    Node,
}

fn classify(e: &MtlxElement) -> Kind {
    match e.tag.as_str() {
        "nodegraph" => Kind::Nodegraph,
        "nodedef" => Kind::Nodedef,
        "input" | "output" | "token" => Kind::Port,
        "xi:include" => Kind::Include,
        t if NON_NODE_ELEMENTS.contains(&t) => Kind::Other,
        _ if e.attr("type").is_some() => Kind::Node,
        _ => Kind::Other,
    }
}

/// One scope's names: (nodes, nodegraphs, interface `<input>`s).
fn prescan_scope(
    children: &[MtlxContent],
) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    let mut nodes = BTreeSet::new();
    let mut graphs = BTreeSet::new();
    let mut interface = BTreeSet::new();
    for child in children {
        let MtlxContent::Element(e) = child else {
            continue;
        };
        let Some(name) = e.attr_value("name") else {
            continue;
        };
        match classify(e) {
            Kind::Node => {
                nodes.insert(name);
            }
            Kind::Nodegraph => {
                graphs.insert(name);
            }
            Kind::Port if e.tag == "input" => {
                interface.insert(name);
            }
            _ => {}
        }
    }
    (nodes, graphs, interface)
}

/// The resolution context of one scope (the root or a nodegraph).
struct ScopeCtx {
    /// Path prefix of this scope's children (`""` at the root).
    path: String,
    chain: Vec<String>,
    depth: usize,
    nodes: BTreeSet<String>,
    graphs: BTreeSet<String>,
    /// `None` at the root (no interface to bind).
    interface: Option<BTreeSet<String>>,
    fileprefix: Option<String>,
    colorspace: Option<String>,
}

impl ScopeCtx {
    /// The empty scope a nodedef's declarations are read in.
    fn declaration(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            chain: Vec::new(),
            depth: 0,
            nodes: BTreeSet::new(),
            graphs: BTreeSet::new(),
            interface: None,
            fileprefix: None,
            colorspace: None,
        }
    }
}

#[derive(Default)]
struct Builder {
    entries: Vec<ReportEntry>,
    summary: ReportSummary,
    /// nodedef name → implementing root-level nodegraph / implementation
    /// names, document order.
    implementations: BTreeMap<String, Vec<String>>,
    /// Root-level nodedef name → its `<input>` names.
    nodedef_inputs: BTreeMap<String, Vec<String>>,
    root_graphs: BTreeSet<String>,
}

impl Builder {
    fn push(
        &mut self,
        at: (&str, usize),
        severity: Severity,
        disposition: Disposition,
        reason: impl Into<String>,
        cite: &'static str,
    ) {
        self.entries.push(ReportEntry {
            path: at.0.to_owned(),
            line: at.1,
            severity,
            disposition,
            reason: reason.into(),
            cite,
        });
    }

    fn finish(mut self) -> ImportReport {
        for e in &self.entries {
            match e.severity {
                Severity::Info => self.summary.info += 1,
                Severity::Warning => self.summary.warning += 1,
            }
            match e.disposition {
                Disposition::Preserve => self.summary.preserve += 1,
                Disposition::Report => self.summary.report += 1,
            }
        }
        ImportReport {
            entries: self.entries,
            summary: self.summary,
        }
    }

    fn prescan_root(&mut self, children: &[MtlxContent]) {
        for child in children {
            let MtlxContent::Element(e) = child else {
                continue;
            };
            let Some(name) = e.attr_value("name") else {
                continue;
            };
            match e.tag.as_str() {
                "nodedef" => {
                    let inputs = e
                        .children
                        .iter()
                        .filter_map(|c| match c {
                            MtlxContent::Element(i) if i.tag == "input" => i.attr_value("name"),
                            _ => None,
                        })
                        .collect();
                    self.nodedef_inputs.insert(name, inputs);
                }
                "nodegraph" | "implementation" => {
                    if e.tag == "nodegraph" {
                        self.root_graphs.insert(name.clone());
                    }
                    if let Some(nd) = e.attr_value("nodedef") {
                        self.implementations.entry(nd).or_default().push(name);
                    }
                }
                _ => {}
            }
        }
    }

    fn version_entry(&mut self, version: Option<&str>, line: usize) {
        let at = ("/materialx", line);
        match version {
            None => self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "no version= on <materialx> (the attribute is required); nothing assumed",
                "§4.1.1",
            ),
            Some(v) if v == "1.38" || v == "1.39" || v.starts_with("1.39.") => self.push(
                at,
                Severity::Info,
                Disposition::Report,
                format!("version \"{v}\" recorded; no upgrade semantics"),
                "§4.1.2",
            ),
            Some(v) => self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                format!(
                    "version \"{v}\" outside the 1.38-1.39 range this reader targets; \
                     recorded, never upgraded"
                ),
                "§4.1.2",
            ),
        }
    }

    /// Prolog / epilog items: the DOCTYPE and non-declaration PIs.
    fn outer_misc_entry(&mut self, m: &MtlxMisc, path: &str) {
        match m.kind {
            MiscKind::Doctype => self.push(
                (path, m.line),
                Severity::Warning,
                Disposition::Preserve,
                "DOCTYPE preserved verbatim; entity declarations never expanded",
                "§2.6(i)",
            ),
            MiscKind::Pi if !(m.raw.starts_with("<?xml ") || m.raw == "<?xml?>") => {
                self.misc_entry(m, path)
            }
            _ => {}
        }
    }

    /// In-element text / processing instructions (comments: no entry).
    fn misc_entry(&mut self, m: &MtlxMisc, path: &str) {
        match m.kind {
            MiscKind::Text => self.push(
                (path, m.line),
                Severity::Warning,
                Disposition::Preserve,
                "text content preserved, not interpreted",
                "§2.6(k)",
            ),
            MiscKind::Pi => self.push(
                (path, m.line),
                Severity::Info,
                Disposition::Preserve,
                "processing instruction preserved, not interpreted",
                "§3.5.1",
            ),
            MiscKind::Comment | MiscKind::Doctype => {}
        }
    }

    fn colorspace_entry(&mut self, at: (&str, usize), v: &str) {
        if RECOGNIZED_COLORSPACES.contains(&v) {
            self.push(
                at,
                Severity::Info,
                Disposition::Report,
                format!("colorspace \"{v}\" recorded (recognized)"),
                "§4.4.2",
            );
        } else {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                format!("colorspace \"{v}\" unrecognized; tag preserved, never guessed"),
                "§4.4.2",
            );
        }
    }

    /// Entries for every attribute not in `consumed`, in source order,
    /// then one aggregated metadata entry.
    fn attr_entries(&mut self, path: &str, line: usize, attrs: &[MtlxAttr], consumed: &[&str]) {
        let at = (path, line);
        let mut meta: Vec<&str> = Vec::new();
        for a in attrs {
            let key = a.name.as_str();
            if consumed.contains(&key) {
                continue;
            }
            let v = a.value();
            match key {
                "fileprefix" => self.push(
                    at,
                    Severity::Warning,
                    Disposition::Report,
                    format!("fileprefix \"{v}\" recorded, never joined"),
                    "§4.3.2",
                ),
                "colorspace" => self.colorspace_entry(at, &v),
                "namespace" => self.push(
                    at,
                    Severity::Warning,
                    Disposition::Report,
                    format!("namespace \"{v}\" preserved; never stripped or flattened"),
                    "§4.5.2",
                ),
                "cms" | "cmsconfig" => self.push(
                    at,
                    Severity::Warning,
                    Disposition::Report,
                    format!(
                        "{key} \"{v}\" preserved as document metadata; no CMS transform at import"
                    ),
                    "§4.4.2",
                ),
                "defaultgeomprop" => self.push(
                    at,
                    Severity::Info,
                    Disposition::Report,
                    format!("defaultgeomprop \"{v}\" kept as data, never normalized"),
                    "§4.3.4",
                ),
                "nodedef" => self.push(
                    at,
                    Severity::Info,
                    Disposition::Report,
                    format!("explicit nodedef binding \"{v}\" kept as data, never resolved"),
                    "§3.4.3",
                ),
                "version" => self.push(
                    at,
                    Severity::Info,
                    Disposition::Report,
                    format!("version request \"{v}\" kept as data, never resolved"),
                    "§3.4.3",
                ),
                "member" => self.push(
                    at,
                    Severity::Warning,
                    Disposition::Preserve,
                    format!(
                        "member selection \"{v}\" kept; Umber's value space has no custom types"
                    ),
                    "§4.5.6",
                ),
                k if k == "xmlns" || k.starts_with("xmlns:") => self.push(
                    at,
                    Severity::Info,
                    Disposition::Preserve,
                    format!("XML namespace declaration {k} preserved"),
                    "§4.5.2",
                ),
                k if METADATA_ATTRS.contains(&k) => meta.push(k),
                k => self.push(
                    at,
                    Severity::Info,
                    Disposition::Preserve,
                    format!("attribute {k}=\"{v}\" preserved, not interpreted"),
                    "§4.1.3",
                ),
            }
        }
        if !meta.is_empty() {
            let keys = meta.join(", ");
            self.push(
                at,
                Severity::Info,
                Disposition::Preserve,
                format!("metadata preserved: {keys}"),
                "§4.5.6",
            );
        }
    }

    fn check_input_cap(e: &MtlxElement, path: &str) -> Result<(), MtlxImportError> {
        let count = e
            .children
            .iter()
            .filter(|c| matches!(c, MtlxContent::Element(i) if i.tag == "input"))
            .count();
        if count > MAX_INPUTS_PER_ELEMENT {
            return Err(MtlxImportError::TooManyInputs {
                element: path.to_owned(),
                line: e.line,
                count,
                cap: MAX_INPUTS_PER_ELEMENT,
            });
        }
        Ok(())
    }

    fn build_items(
        &mut self,
        children: Vec<MtlxContent>,
        scope: &ScopeCtx,
        container: &str,
    ) -> Result<Vec<MtlxItem>, MtlxImportError> {
        let mut items = Vec::with_capacity(children.len());
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for child in children {
            let e = match child {
                MtlxContent::Misc(m) => {
                    self.misc_entry(&m, container);
                    items.push(MtlxItem::Misc(m));
                    continue;
                }
                MtlxContent::Element(e) => e,
            };
            let name = e.attr_value("name");
            let path = format!("{}/{}", scope.path, segment(&e.tag, name.as_deref()));
            if let Some(n) = &name {
                if !seen.insert(n.clone()) {
                    self.push(
                        (path.as_str(), e.line),
                        Severity::Warning,
                        Disposition::Report,
                        format!(
                            "duplicate name \"{n}\" in this scope; references to it are ambiguous"
                        ),
                        "§2.6(e)",
                    );
                }
            }
            let item = match classify(&e) {
                Kind::Nodegraph => MtlxItem::Nodegraph(self.build_nodegraph(e, path, scope)?),
                Kind::Nodedef => MtlxItem::Nodedef(self.build_nodedef(e, &path)?),
                Kind::Port => {
                    self.summary.ports += 1;
                    let port = self.build_input(&e, &path, scope)?;
                    MtlxItem::Port(MtlxPort {
                        element: e,
                        scope: scope.chain.clone(),
                        port,
                    })
                }
                Kind::Include => {
                    self.summary.includes += 1;
                    let href = e.attr_value("href");
                    let reason = match &href {
                        Some(h) => format!(
                            "XInclude href \"{h}\" recorded, never followed; agreement checks not run"
                        ),
                        None => "XInclude without href= recorded, never followed".to_owned(),
                    };
                    self.push(
                        (path.as_str(), e.line),
                        Severity::Warning,
                        Disposition::Report,
                        reason,
                        "§4.5.1",
                    );
                    self.attr_entries(&path, e.line, &e.attrs, &["href"]);
                    MtlxItem::Include(MtlxInclude { element: e, href })
                }
                Kind::Other => {
                    self.summary.other += 1;
                    let tag = e.tag.as_str();
                    self.push(
                        (path.as_str(), e.line),
                        Severity::Warning,
                        Disposition::Preserve,
                        format!("element <{tag}> preserved verbatim, not interpreted"),
                        "§3.5.1",
                    );
                    MtlxItem::Other(e)
                }
                Kind::Node => MtlxItem::Node(self.build_node(e, &path, scope)?),
            };
            items.push(item);
        }
        Ok(items)
    }

    /// `<input>` / `<output>` children → typed views; entries for every
    /// other child (unexpected elements, text, PIs).
    fn child_ports(
        &mut self,
        e: &MtlxElement,
        path: &str,
        scope: &ScopeCtx,
    ) -> Result<(Vec<MtlxInput>, Vec<MtlxInput>), MtlxImportError> {
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for c in &e.children {
            match c {
                MtlxContent::Element(child) => {
                    let name = child.attr_value("name");
                    let child_path = format!("{path}/{}", segment(&child.tag, name.as_deref()));
                    match child.tag.as_str() {
                        "input" => inputs.push(self.build_input(child, &child_path, scope)?),
                        "output" => outputs.push(self.build_input(child, &child_path, scope)?),
                        tag => self.push(
                            (child_path.as_str(), child.line),
                            Severity::Warning,
                            Disposition::Preserve,
                            format!("unexpected child <{tag}> preserved verbatim, not interpreted"),
                            "§3.5.1",
                        ),
                    }
                }
                MtlxContent::Misc(m) => self.misc_entry(m, path),
            }
        }
        Ok((inputs, outputs))
    }

    fn build_node(
        &mut self,
        e: MtlxElement,
        path: &str,
        scope: &ScopeCtx,
    ) -> Result<MtlxNode, MtlxImportError> {
        Self::check_input_cap(&e, path)?;
        self.summary.nodes += 1;
        let at = (path, e.line);
        let name = e.attr_value("name");
        if name.is_none() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "node without name=; no nodename= can address it",
                "§2.5.3",
            );
        }
        if e.tag == "node" {
            self.push(
                at,
                Severity::Info,
                Disposition::Preserve,
                "Umber painter-dialect <node>: its category rides type=, kept verbatim",
                "§2.5.1",
            );
        }
        if is_coordinate_binding(&e.tag) {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "coordinate-binding node kept as data; UV-set name never normalized",
                "§4.3.4",
            );
        }
        let version = e.attr_value("version");
        if let Some(v) = &version {
            self.push(
                at,
                Severity::Info,
                Disposition::Report,
                format!("nodedef version request \"{v}\" kept as data, never resolved"),
                "§3.4.3",
            );
        }
        self.attr_entries(path, e.line, &e.attrs, &["name", "type", "version"]);
        let (inputs, _outputs) = self.child_ports(&e, path, scope)?;
        Ok(MtlxNode {
            category: e.tag.clone(),
            name: name.unwrap_or_default(),
            ty: e.attr_value("type"),
            version,
            scope: scope.chain.clone(),
            fileprefix: e
                .attr_value("fileprefix")
                .or_else(|| scope.fileprefix.clone()),
            colorspace: e
                .attr_value("colorspace")
                .or_else(|| scope.colorspace.clone()),
            inputs,
            element: e,
        })
    }

    fn build_nodedef(
        &mut self,
        e: MtlxElement,
        path: &str,
    ) -> Result<MtlxNodedef, MtlxImportError> {
        Self::check_input_cap(&e, path)?;
        self.summary.nodedefs += 1;
        let at = (path, e.line);
        let name = e.attr_value("name");
        let node = e.attr_value("node");
        let version = e.attr_value("version");
        let isdefaultversion = e.attr_value("isdefaultversion");
        if name.is_none() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "nodedef without name=",
                "§2.5.6",
            );
        }
        if node.is_none() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "nodedef without node= category",
                "§2.5.6",
            );
        }
        if let Some(v) = &version {
            let reason = match &isdefaultversion {
                Some(d) => format!(
                    "nodedef version \"{v}\" (isdefaultversion \"{d}\") preserved; \
                     version resolution never performed"
                ),
                None => {
                    format!("nodedef version \"{v}\" preserved; version resolution never performed")
                }
            };
            self.push(at, Severity::Info, Disposition::Report, reason, "§3.4.3");
        }
        let implementations = name
            .as_ref()
            .and_then(|n| self.implementations.get(n))
            .cloned()
            .unwrap_or_default();
        let reason = if implementations.is_empty() {
            "declaration only: no implementation in this document".to_owned()
        } else {
            let names = implementations
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("implementation {names} preserved opaque, never evaluated")
        };
        self.push(at, Severity::Info, Disposition::Report, reason, "§3.4.1");
        self.attr_entries(
            path,
            e.line,
            &e.attrs,
            &["name", "node", "version", "isdefaultversion"],
        );
        let (inputs, outputs) = self.child_ports(&e, path, &ScopeCtx::declaration(path))?;
        Ok(MtlxNodedef {
            name: name.unwrap_or_default(),
            node,
            version,
            isdefaultversion,
            inputs,
            outputs,
            implementations,
            element: e,
        })
    }

    fn build_nodegraph(
        &mut self,
        e: MtlxElement,
        path: String,
        scope: &ScopeCtx,
    ) -> Result<MtlxNodegraph, MtlxImportError> {
        // Counted recursion (§4.2.2): checked BEFORE descending.
        let depth = scope.depth + 1;
        if depth > MAX_NODEGRAPH_DEPTH {
            return Err(MtlxImportError::DepthExceeded {
                depth,
                cap: MAX_NODEGRAPH_DEPTH,
            });
        }
        Self::check_input_cap(&e, &path)?;
        self.summary.nodegraphs += 1;
        let at = (path.as_str(), e.line);
        let name = e.attr_value("name");
        let nodedef = e.attr_value("nodedef");
        if name.is_none() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "nodegraph without name=",
                "§4.2.3",
            );
        }
        let reason = match &nodedef {
            Some(nd) if self.nodedef_inputs.contains_key(nd) => {
                format!("implements nodedef \"{nd}\"; body preserved opaque, never evaluated")
            }
            Some(nd) => format!(
                "implements nodedef \"{nd}\" (not declared in this document); \
                 body preserved opaque, never evaluated"
            ),
            None => "inlined nodegraph without nodedef identity; contents preserved".to_owned(),
        };
        let cite = if nodedef.is_some() {
            "§3.4.1"
        } else {
            "§1.3.2"
        };
        self.push(at, Severity::Info, Disposition::Report, reason, cite);
        self.attr_entries(&path, e.line, &e.attrs, &["name", "nodedef"]);

        let (nodes, graphs, mut interface) = prescan_scope(&e.children);
        if let Some(inputs) = nodedef.as_ref().and_then(|n| self.nodedef_inputs.get(n)) {
            interface.extend(inputs.iter().cloned());
        }
        let mut chain = scope.chain.clone();
        chain.push(name.clone().unwrap_or_default());
        let child_scope = ScopeCtx {
            path: path.clone(),
            chain,
            depth,
            nodes,
            graphs,
            interface: Some(interface),
            fileprefix: e
                .attr_value("fileprefix")
                .or_else(|| scope.fileprefix.clone()),
            colorspace: e
                .attr_value("colorspace")
                .or_else(|| scope.colorspace.clone()),
        };
        let MtlxElement {
            attrs,
            children,
            self_closing,
            line,
            ..
        } = e;
        let items = self.build_items(children, &child_scope, &path)?;
        Ok(MtlxNodegraph {
            name: name.unwrap_or_default(),
            nodedef,
            scope: scope.chain.clone(),
            depth,
            attrs,
            items,
            self_closing,
            line,
        })
    }

    fn build_input(
        &mut self,
        e: &MtlxElement,
        path: &str,
        scope: &ScopeCtx,
    ) -> Result<MtlxInput, MtlxImportError> {
        let at = (path, e.line);
        let tag = e.tag.as_str();
        let name = e.attr_value("name");
        let ty = e.attr_value("type");
        let value = e.attr_value("value");
        let nodename = e.attr_value("nodename");
        let nodegraph = e.attr_value("nodegraph");
        let output = e.attr_value("output");
        let interfacename = e.attr_value("interfacename");
        if name.is_none() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                format!("<{tag}> without name="),
                "§2.5.4",
            );
        }
        if ty.is_none() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                format!("untyped <{tag}>: its edge type is unknown, never assumed color3"),
                "§2.5.4",
            );
        }

        // §4.5.4: classify output= by form.
        let has_source = nodename.is_some() || nodegraph.is_some();
        let legacy = output
            .as_deref()
            .filter(|_| !has_source)
            .and_then(legacy_id);
        if legacy.is_none() {
            if let Some(o) = output.as_deref().filter(|o| !is_port_name(o)) {
                return Err(MtlxImportError::BadOutputRef {
                    input: name.clone().unwrap_or_default(),
                    line: e.line,
                    value: o.to_owned(),
                });
            }
        }
        let conflicting = nodename.is_some() && nodegraph.is_some();
        let connection = match (legacy, nodename, nodegraph) {
            (Some(id), _, _) => Some(MtlxConnection::LegacyEdge { id }),
            (None, Some(node), _) => Some(MtlxConnection::Node {
                node,
                output: output.clone(),
            }),
            (None, None, Some(graph)) => Some(MtlxConnection::Graph {
                graph,
                output: output.clone(),
            }),
            (None, None, None) => output
                .clone()
                .map(|output| MtlxConnection::PortOnly { output }),
        };

        if value.is_some() && connection.is_some() {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "carries both value= and a connection; both preserved, precedence not decided",
                "§2.5.2",
            );
        }
        if conflicting {
            self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                "carries both nodename= and nodegraph=; nodename= taken, both preserved",
                "§2.5.2",
            );
        }
        match &connection {
            Some(MtlxConnection::Node { node, .. }) if !scope.nodes.contains(node) => self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                format!("nodename \"{node}\" names no node in this scope"),
                "§4.2.3",
            ),
            Some(MtlxConnection::Graph { graph, .. })
                if !scope.graphs.contains(graph) && !self.root_graphs.contains(graph) =>
            {
                self.push(
                    at,
                    Severity::Warning,
                    Disposition::Report,
                    format!("nodegraph \"{graph}\" names no nodegraph in this scope"),
                    "§4.2.3",
                )
            }
            Some(MtlxConnection::LegacyEdge { id }) => {
                let target = format!("N{id}");
                if scope.nodes.contains(&target) {
                    self.push(
                        at,
                        Severity::Info,
                        Disposition::Report,
                        format!("Umber painter-dialect edge output=\"{target}\""),
                        "§4.5.4",
                    );
                } else {
                    self.push(
                        at,
                        Severity::Warning,
                        Disposition::Report,
                        format!(
                            "legacy Umber edge output=\"{target}\" names no node in this scope"
                        ),
                        "§4.5.4",
                    );
                }
            }
            Some(MtlxConnection::PortOnly { output }) => self.push(
                at,
                Severity::Warning,
                Disposition::Report,
                format!("output=\"{output}\" selects a port with no nodename=/nodegraph= source"),
                "§4.5.4",
            ),
            _ => {}
        }
        if let Some(i) = &interfacename {
            if !scope.interface.as_ref().is_some_and(|s| s.contains(i)) {
                self.push(
                    at,
                    Severity::Warning,
                    Disposition::Report,
                    format!(
                        "interfacename \"{i}\" binds no input of an enclosing nodegraph interface"
                    ),
                    "§4.2.3",
                );
            }
        }
        self.attr_entries(
            path,
            e.line,
            &e.attrs,
            &[
                "name",
                "type",
                "value",
                "nodename",
                "nodegraph",
                "output",
                "interfacename",
            ],
        );
        Ok(MtlxInput {
            name: name.unwrap_or_default(),
            ty,
            value,
            connection,
            interfacename,
            colorspace: e.attr_value("colorspace"),
            member: e.attr_value("member"),
            line: e.line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mtlx::{painter_nodedefs, to_mtlx, NodedefDecl};
    use crate::mtlx_doc::{to_mtlx_document, MtlxTexture, SurfaceParams, TextureMap};
    use crate::{Edge, Graph, Node, ParamValue};

    /// Fixtures are read through here: a Windows checkout without a
    /// `.gitattributes` may hand `include_str!` CRLF text, and the
    /// canonical emitter writes `\n`.
    fn fixture(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    const BLENDER_FLAT: &str = include_str!("../tests/fixtures/mtlx/blender_flat_openpbr.mtlx");
    const BLENDER_STANDARD: &str =
        include_str!("../tests/fixtures/mtlx/blender_standard_surface.mtlx");
    const HAZARDS: &str = include_str!("../tests/fixtures/mtlx/hazards_lossy.mtlx");
    const NODEDEF_INSTANCED: &str = include_str!("../tests/fixtures/mtlx/nodedef_instanced.mtlx");
    const SP_USD_UDIM: &str = include_str!("../tests/fixtures/mtlx/sp_usd_creator_udim.mtlx");
    const USD_INLINED: &str = include_str!("../tests/fixtures/mtlx/usd_inlined_nodegraph.mtlx");

    fn parse(src: &str) -> (MtlxDocument, ImportReport) {
        parse_mtlx(src).expect("parses")
    }

    fn input<'a>(node: &'a MtlxNode, name: &str) -> &'a MtlxInput {
        node.inputs
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("no input {name}"))
    }

    // -- byte-identical re-emit (§5.1.3) -----------------------------------

    #[test]
    fn every_fixture_re_emits_byte_identically() {
        for (name, raw) in [
            ("blender_flat_openpbr", BLENDER_FLAT),
            ("blender_standard_surface", BLENDER_STANDARD),
            ("hazards_lossy", HAZARDS),
            ("nodedef_instanced", NODEDEF_INSTANCED),
            ("sp_usd_creator_udim", SP_USD_UDIM),
            ("usd_inlined_nodegraph", USD_INLINED),
        ] {
            let src = fixture(raw);
            let (doc, _) = parse(&src);
            assert_eq!(emit_mtlx(&doc), src, "{name}");
        }
    }

    // -- exact report text per fixture -------------------------------------

    #[test]
    fn blender_flat_report_is_exact() {
        let (doc, report) = parse(&fixture(BLENDER_FLAT));
        let expected = "\
info report L12 /materialx: version \"1.39\" recorded; no upgrade semantics [§4.1.2]
warning report L13 /geompropvalue[uv_UVMap]: coordinate-binding node kept as data; UV-set name never normalized [§4.3.4]
info report L17 /image[Image_Texture]/input[file]: colorspace \"srgb_texture\" recorded (recognized) [§4.4.2]
summary: nodes 7, nodedefs 0, nodegraphs 0, ports 0, includes 0, other 0; info 2, warning 1; preserve 0, report 3
";
        assert_eq!(report.render(), expected);
        assert_eq!(doc.version.as_deref(), Some("1.39"));
        // Flat (§1.2.3): every node at the root scope, in document order.
        let names: Vec<&str> = doc.nodes().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "uv_UVMap",
                "Image_Texture",
                "Noise_Texture",
                "Invert",
                "Mix",
                "Principled_BSDF",
                "Material",
            ]
        );
        assert!(doc.nodes().iter().all(|n| n.scope.is_empty()));
        // Typed edges from the declared socket type (§2.5.4), not color3.
        let invert = doc.node("Invert").unwrap();
        let wire = input(invert, "in");
        assert_eq!(wire.ty.as_deref(), Some("float"));
        assert_eq!(
            wire.connection,
            Some(MtlxConnection::Node {
                node: "Noise_Texture".into(),
                output: None
            })
        );
        // The UV binding is data, the set name verbatim (§1.2.5).
        let uv = doc.node("uv_UVMap").unwrap();
        assert_eq!(uv.category, "geompropvalue");
        assert_eq!(input(uv, "geomprop").value.as_deref(), Some("UVMap"));
    }

    #[test]
    fn blender_standard_surface_report_is_exact() {
        let (doc, report) = parse(&fixture(BLENDER_STANDARD));
        let expected = "\
info report L11 /materialx: version \"1.38\" recorded; no upgrade semantics [§4.1.2]
info report L13 /image[Image_Texture]/input[file]: colorspace \"srgb_texture\" recorded (recognized) [§4.4.2]
summary: nodes 4, nodedefs 0, nodegraphs 0, ports 0, includes 0, other 0; info 2, warning 0; preserve 0, report 2
";
        assert_eq!(report.render(), expected);
        let shader = doc.node("Principled_BSDF").unwrap();
        assert_eq!(shader.category, "standard_surface");
        assert_eq!(shader.ty.as_deref(), Some("surfaceshader"));
        // Value strings verbatim: "0", not a reformatted "0.0".
        assert_eq!(input(shader, "metalness").value.as_deref(), Some("0"));
    }

    #[test]
    fn sp_usd_creator_report_is_exact_and_udim_is_verbatim() {
        let src = fixture(SP_USD_UDIM);
        let (doc, report) = parse(&src);
        let expected = "\
info report L13 /materialx: version \"1.39\" recorded; no upgrade semantics [§4.1.2]
info report L15 /image[Sword_basecolor]/input[file]: colorspace \"srgb_texture\" recorded (recognized) [§4.4.2]
warning report L18 /image[Sword_metalness]/input[file]: colorspace \"raw\" unrecognized; tag preserved, never guessed [§4.4.2]
warning report L21 /image[Sword_roughness]/input[file]: colorspace \"raw\" unrecognized; tag preserved, never guessed [§4.4.2]
warning report L24 /image[Sword_normal]/input[file]: colorspace \"raw\" unrecognized; tag preserved, never guessed [§4.4.2]
summary: nodes 7, nodedefs 0, nodegraphs 0, ports 0, includes 0, other 0; info 2, warning 3; preserve 0, report 5
";
        assert_eq!(report.render(), expected);
        // §4.3.1: the decoded value IS the token; the raw bytes are the
        // escaped form and re-emit untouched.
        let base = doc.node("Sword_basecolor").unwrap();
        let file = input(base, "file");
        assert_eq!(
            file.value.as_deref(),
            Some("textures/Sword_BaseColor.<UDIM>.png")
        );
        assert_eq!(file.ty.as_deref(), Some("filename"));
        let raw = base.element.children.iter().find_map(|c| match c {
            MtlxContent::Element(e) => e.attr("value").map(|a| a.raw.clone()),
            MtlxContent::Misc(_) => None,
        });
        assert_eq!(
            raw.as_deref(),
            Some("textures/Sword_BaseColor.&lt;UDIM&gt;.png")
        );
        assert!(emit_mtlx(&doc).contains("value=\"textures/Sword_BaseColor.&lt;UDIM&gt;.png\""));
        // Float inputs end-to-end (§1.1.4).
        let shader = doc.node("Sword_openpbr").unwrap();
        assert_eq!(input(shader, "base_metalness").ty.as_deref(), Some("float"));
        assert_eq!(
            input(shader, "geometry_normal").connection,
            Some(MtlxConnection::Node {
                node: "Sword_normalmap".into(),
                output: None
            })
        );
    }

    #[test]
    fn usd_inlined_nodegraph_report_is_exact() {
        let (doc, report) = parse(&fixture(USD_INLINED));
        let expected = "\
info report L11 /materialx: version \"1.39\" recorded; no upgrade semantics [§4.1.2]
info report L12 /nodegraph[NG_Group]: inlined nodegraph without nodedef identity; contents preserved [§1.3.2]
summary: nodes 4, nodedefs 0, nodegraphs 1, ports 1, includes 0, other 0; info 2, warning 0; preserve 0, report 2
";
        assert_eq!(report.render(), expected);
        let graphs = doc.nodegraphs();
        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].nodedef, None);
        assert_eq!(graphs[0].depth, 1);
        assert_eq!(doc.node("mask_invert").unwrap().scope, ["NG_Group"]);
        let shader = doc.node("Shader").unwrap();
        assert_eq!(
            input(shader, "specular_roughness").connection,
            Some(MtlxConnection::Graph {
                graph: "NG_Group".into(),
                output: Some("out".into())
            })
        );
    }

    #[test]
    fn nodedef_instanced_report_scopes_and_identity_are_exact() {
        let (doc, report) = parse(&fixture(NODEDEF_INSTANCED));
        let expected = "\
info report L14 /materialx: version \"1.39\" recorded; no upgrade semantics [§4.1.2]
info report L15 /nodedef[ND_rust_mask_float]: nodedef version \"2.0\" (isdefaultversion \"true\") preserved; version resolution never performed [§3.4.3]
info report L15 /nodedef[ND_rust_mask_float]: implementation \"NG_rust_mask_float\" preserved opaque, never evaluated [§3.4.1]
info preserve L15 /nodedef[ND_rust_mask_float]: metadata preserved: nodegroup, doc [§4.5.6]
info preserve L16 /nodedef[ND_rust_mask_float]/input[amount]: metadata preserved: uimin, uimax, uiname, uifolder [§4.5.6]
info report L20 /nodegraph[NG_rust_mask_float]: implements nodedef \"ND_rust_mask_float\"; body preserved opaque, never evaluated [§3.4.1]
info report L21 /nodegraph[NG_rust_mask_float]/nodegraph[NG_inner]: inlined nodegraph without nodedef identity; contents preserved [§1.3.2]
info report L33 /rust_mask[rust1]: nodedef version request \"2.0\" kept as data, never resolved [§3.4.3]
summary: nodes 4, nodedefs 1, nodegraphs 2, ports 3, includes 0, other 0; info 8, warning 0; preserve 2, report 6
";
        assert_eq!(report.render(), expected);
        // Identity + interface + version + body (§2.5.6).
        let nd = doc.nodedefs()[0];
        assert_eq!(nd.name, "ND_rust_mask_float");
        assert_eq!(nd.node.as_deref(), Some("rust_mask"));
        assert_eq!(nd.version.as_deref(), Some("2.0"));
        assert_eq!(nd.isdefaultversion.as_deref(), Some("true"));
        let ins: Vec<(&str, Option<&str>)> = nd
            .inputs
            .iter()
            .map(|i| (i.name.as_str(), i.ty.as_deref()))
            .collect();
        assert_eq!(ins, [("amount", Some("float")), ("seed", Some("integer"))]);
        assert_eq!(nd.outputs.len(), 1);
        assert_eq!(nd.implementations, ["NG_rust_mask_float"]);
        // Scope chains (§4.2.3), outermost first.
        assert_eq!(
            doc.node("inner_noise").unwrap().scope,
            ["NG_rust_mask_float", "NG_inner"]
        );
        assert_eq!(doc.node("clamped").unwrap().scope, ["NG_rust_mask_float"]);
        assert!(doc.node("rust1").unwrap().scope.is_empty());
        let depths: Vec<usize> = doc.nodegraphs().iter().map(|g| g.depth).collect();
        assert_eq!(depths, [1, 2]);
        // nodegraph= + output= is a port select on the named graph.
        let clamp_in = input(doc.node("clamped").unwrap(), "in");
        assert_eq!(
            clamp_in.connection,
            Some(MtlxConnection::Graph {
                graph: "NG_inner".into(),
                output: Some("out".into())
            })
        );
        let amp = input(doc.node("inner_noise").unwrap(), "amplitude");
        assert_eq!(amp.interfacename.as_deref(), Some("amount"));
        assert_eq!(amp.connection, None);
        // The version request is data (§3.4.3).
        assert_eq!(doc.node("rust1").unwrap().version.as_deref(), Some("2.0"));
    }

    #[test]
    fn hazards_report_is_exact() {
        let (_, report) = parse(&fixture(HAZARDS));
        let expected = "\
warning preserve L15 (prolog): DOCTYPE preserved verbatim; entity declarations never expanded [§2.6(i)]
warning report L18 /materialx: version \"1.36\" outside the 1.38-1.39 range this reader targets; recorded, never upgraded [§4.1.2]
info report L18 /materialx: colorspace \"lin_rec709\" recorded (recognized) [§4.4.2]
warning report L18 /materialx: cms \"ocio\" preserved as document metadata; no CMS transform at import [§4.4.2]
warning report L18 /materialx: cmsconfig \"studio_config.ocio\" preserved as document metadata; no CMS transform at import [§4.4.2]
warning report L18 /materialx: namespace \"studio\" preserved; never stripped or flattened [§4.5.2]
info preserve L18 /materialx: XML namespace declaration xmlns:xi preserved [§4.5.2]
warning report L19 /xi:include: XInclude href \"stdlib_custom.mtlx\" recorded, never followed; agreement checks not run [§4.5.1]
warning preserve L20 /look[default_look]: element <look> preserved verbatim, not interpreted [§3.5.1]
warning report L23 /image[tex_a]: fileprefix \"textures/\" recorded, never joined [§4.3.2]
warning report L24 /image[tex_a]/input[file]: colorspace \"acescg\" unrecognized; tag preserved, never guessed [§4.4.2]
warning report L26 /image[tex_a]: duplicate name \"tex_a\" in this scope; references to it are ambiguous [§2.6(e)]
warning report L30 /dot[legacy_wire]/input[in]: legacy Umber edge output=\"N12\" names no node in this scope [§4.5.4]
warning report L33 /swizzle[port_only]/input[in]: output=\"outr\" selects a port with no nodename=/nodegraph= source [§4.5.4]
warning report L36 /mix[dangling]/input[fg]: nodename \"missing_node\" names no node in this scope [§4.2.3]
warning report L37 /mix[dangling]/input[bg]: carries both value= and a connection; both preserved, precedence not decided [§2.5.2]
warning report L38 /mix[dangling]/input[mix]: interfacename \"amount\" binds no input of an enclosing nodegraph interface [§4.2.3]
warning preserve L41 /split[multi]/input[in]: member selection \"r\" kept; Umber's value space has no custom types [§4.5.6]
warning preserve L43 /sticky: element <sticky> preserved verbatim, not interpreted [§3.5.1]
warning preserve L44 /materialx: text content preserved, not interpreted [§2.6(k)]
summary: nodes 6, nodedefs 0, nodegraphs 0, ports 0, includes 1, other 2; info 2, warning 18; preserve 6, report 14
";
        assert_eq!(report.render(), expected);
        assert_eq!(report.max_severity(), Some(Severity::Warning));
    }

    #[test]
    fn hazards_ir_keeps_every_construct_as_data() {
        let (doc, _) = parse(&fixture(HAZARDS));
        // §4.1.2: the version is recorded, not upgraded.
        assert_eq!(doc.version.as_deref(), Some("1.36"));
        let keys: Vec<&str> = doc.root_attrs.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            keys,
            [
                "version",
                "colorspace",
                "cms",
                "cmsconfig",
                "namespace",
                "xmlns:xi",
            ]
        );
        // The DOCTYPE survives with its internal subset.
        assert_eq!(doc.prolog.len(), 3);
        assert_eq!(doc.prolog[2].kind, MiscKind::Doctype);
        assert_eq!(
            doc.prolog[2].raw,
            "<!DOCTYPE materialx [\n  <!ENTITY tex \"textures\">\n]>"
        );
        // XInclude recorded, never followed (§4.5.1).
        let include = doc.all_items().into_iter().find_map(|i| match i {
            MtlxItem::Include(inc) => Some(inc.href.clone()),
            _ => None,
        });
        assert_eq!(include, Some(Some("stdlib_custom.mtlx".into())));
        // fileprefix recorded per node, never joined (§4.3.2).
        let nodes = doc.nodes();
        assert_eq!(nodes[0].name, "tex_a");
        assert_eq!(nodes[0].fileprefix.as_deref(), Some("textures/"));
        assert_eq!(
            input(nodes[0], "file").value.as_deref(),
            Some("a_<UDIM>.png")
        );
        assert_eq!(nodes[1].name, "tex_a");
        assert_eq!(nodes[1].fileprefix, None);
        // The document working colorspace is the effective one below it.
        assert_eq!(nodes[1].colorspace.as_deref(), Some("lin_rec709"));
        // Both output= spellings (§4.5.4).
        assert_eq!(
            input(doc.node("legacy_wire").unwrap(), "in").connection,
            Some(MtlxConnection::LegacyEdge { id: 12 })
        );
        assert_eq!(
            input(doc.node("port_only").unwrap(), "in").connection,
            Some(MtlxConnection::PortOnly {
                output: "outr".into()
            })
        );
        let member = input(doc.node("multi").unwrap(), "in");
        assert_eq!(member.member.as_deref(), Some("r"));
        // Text content is kept, not skipped (§2.6(k)).
        let texts: Vec<&str> = doc
            .items
            .iter()
            .filter_map(|i| match i {
                MtlxItem::Misc(m) if m.kind == MiscKind::Text => Some(m.raw.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["loose text"]);
    }

    #[test]
    fn reimport_of_an_unchanged_file_yields_a_byte_identical_report() {
        let src = fixture(HAZARDS);
        let first = parse(&src).1.render();
        let second = parse(&src).1.render();
        assert_eq!(first, second);
        // And through the re-emit (byte-identical source → same report).
        let (doc, _) = parse(&src);
        assert_eq!(parse(&emit_mtlx(&doc)).1.render(), first);
    }

    // -- self-hosting (§5.1.3: §2.6(a) inverted) ----------------------------

    fn full_set() -> Vec<MtlxTexture> {
        let tex = |map: TextureMap, file: &str, channel: Option<u8>| MtlxTexture {
            map,
            file: file.into(),
            colorspace: map.default_colorspace(),
            channel,
        };
        vec![
            tex(TextureMap::BaseColor, "Sword_baseColor.<UDIM>.png", None),
            tex(TextureMap::Metallic, "Sword_metallicRoughness.png", Some(1)),
            tex(
                TextureMap::Roughness,
                "Sword_metallicRoughness.png",
                Some(2),
            ),
            tex(TextureMap::Normal, "Sword_normal.png", None),
            tex(TextureMap::Opacity, "Sword_opacity.png", None),
            tex(TextureMap::Emissive, "Sword_emissive.png", None),
            tex(TextureMap::AmbientOcclusion, "Sword_ao.png", None),
        ]
    }

    #[test]
    fn own_material_document_parses_non_empty_and_clean() {
        let src = to_mtlx_document("Sword", &SurfaceParams::default(), &full_set());
        let (doc, report) = parse(&src);
        // Non-empty (from_mtlx yields an EMPTY graph here, §2.6(a)): 6
        // images + 2 extracts + 1 normalmap + the shader + the material.
        assert_eq!(doc.nodes().len(), 11);
        // Zero entries above info: the version + 6 recognized colorspaces.
        assert_eq!(report.max_severity(), Some(Severity::Info));
        assert_eq!(
            report.summary.to_string(),
            "summary: nodes 11, nodedefs 0, nodegraphs 0, ports 0, includes 0, other 0; \
             info 7, warning 0; preserve 0, report 7"
        );
        // Lossless: byte-identical re-emit.
        assert_eq!(emit_mtlx(&doc), src);
        // The <UDIM> token reads back verbatim (§4.3.1).
        let image = doc.node("Sword_base_color_image").unwrap();
        assert_eq!(
            input(image, "file").value.as_deref(),
            Some("Sword_baseColor.<UDIM>.png")
        );
        // Extract chains are typed per socket, not color3 (§2.5.4).
        let shader = doc.node("Sword_shader").unwrap();
        let metal = input(shader, "base_metalness");
        assert_eq!(metal.ty.as_deref(), Some("float"));
        assert_eq!(
            metal.connection,
            Some(MtlxConnection::Node {
                node: "Sword_base_metalness_extract".into(),
                output: None
            })
        );
        assert_eq!(input(shader, "specular_ior").value.as_deref(), Some("1.5"));
    }

    #[test]
    fn own_painter_graph_document_round_trips_through_the_import() {
        let mut g = Graph::new();
        g.add_node(Node {
            id: 1,
            node_def: "noise_perlin".into(),
            params: vec![("scale".into(), ParamValue::Float(4.0))],
            canvas: None,
        });
        g.add_node(Node {
            id: 2,
            node_def: "flood_fill".into(),
            params: vec![],
            canvas: None,
        });
        g.add_edge(Edge {
            from: 1,
            to: 2,
            input: "mask".into(),
        });
        let src = to_mtlx(&g, &painter_nodedefs());
        let (doc, report) = parse(&src);
        assert_eq!(emit_mtlx(&doc), src);
        assert_eq!(doc.nodedefs().len(), 9);
        let n2 = doc.node("N2").unwrap();
        assert_eq!(n2.category, "node");
        assert_eq!(n2.ty.as_deref(), Some("flood_fill"));
        assert_eq!(
            input(n2, "mask").connection,
            Some(MtlxConnection::LegacyEdge { id: 1 })
        );
        // version + 9 declaration-only nodedefs + 2 painter <node>s + the
        // resolved legacy edge — all info.
        assert_eq!(report.max_severity(), Some(Severity::Info));
        assert_eq!(
            report.summary.to_string(),
            "summary: nodes 2, nodedefs 9, nodegraphs 0, ports 0, includes 0, other 0; \
             info 13, warning 0; preserve 2, report 11"
        );
    }

    #[test]
    fn own_writer_empty_elements_re_emit_byte_identically() {
        // to_mtlx writes an empty element as an open line + a close line:
        // the empty graph, a param-less unconnected node, an input-less
        // nodedef. The block form must survive (not collapse to <x></x>).
        let empty = to_mtlx(&Graph::new(), &[]);
        assert_eq!(
            empty,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<materialx version=\"1.38\">\n</materialx>\n"
        );
        assert_eq!(emit_mtlx(&parse(&empty).0), empty);

        let mut g = Graph::new();
        g.add_node(Node {
            id: 1,
            node_def: "passthrough".into(),
            params: vec![],
            canvas: None,
        });
        let bare = NodedefDecl {
            name: "bare".into(),
            category: "bare".into(),
            inputs: vec![],
        };
        let src = to_mtlx(&g, &[bare]);
        assert!(src.contains("  <node name=\"N1\" type=\"passthrough\">\n  </node>\n"));
        assert!(src.contains("  <nodedef name=\"bare\" node=\"bare\">\n  </nodedef>\n"));
        let (doc, _) = parse(&src);
        assert_eq!(emit_mtlx(&doc), src);
        assert!(doc.node("N1").unwrap().inputs.is_empty());
        assert!(doc.nodedefs()[0].inputs.is_empty());
    }

    // -- output= both spellings (§4.5.4) ------------------------------------

    #[test]
    fn output_attribute_is_classified_by_form() {
        let src = "<materialx version=\"1.39\">
  <image name=\"N7\" type=\"color3\" />
  <dot name=\"a\" type=\"color3\">
    <input name=\"in\" type=\"color3\" output=\"N7\" />
  </dot>
  <dot name=\"b\" type=\"color3\">
    <input name=\"in\" type=\"color3\" nodename=\"N7\" output=\"N7\" />
  </dot>
  <dot name=\"c\" type=\"color3\">
    <input name=\"in\" type=\"color3\" nodename=\"a\" output=\"outr\" />
  </dot>
</materialx>
";
        let (doc, report) = parse(src);
        // Sourceless N<digits>: the legacy painter edge, resolved by name.
        assert_eq!(
            input(doc.node("a").unwrap(), "in").connection,
            Some(MtlxConnection::LegacyEdge { id: 7 })
        );
        // With a source, output= is a port select even when it reads N7.
        assert_eq!(
            input(doc.node("b").unwrap(), "in").connection,
            Some(MtlxConnection::Node {
                node: "N7".into(),
                output: Some("N7".into())
            })
        );
        assert_eq!(
            input(doc.node("c").unwrap(), "in").connection,
            Some(MtlxConnection::Node {
                node: "a".into(),
                output: Some("outr".into())
            })
        );
        assert_eq!(
            report.entries[1].to_string(),
            "info report L4 /dot[a]/input[in]: Umber painter-dialect edge output=\"N7\" [§4.5.4]"
        );
        assert_eq!(report.entries.len(), 2);
        assert_eq!(emit_mtlx(&doc), src);
    }

    #[test]
    fn output_that_is_neither_spelling_is_a_typed_error() {
        let sourceless = "<materialx version=\"1.39\">\n  <dot name=\"a\" type=\"float\">\n    \
                          <input name=\"in\" type=\"float\" output=\"2\" />\n  </dot>\n</materialx>\n";
        assert_eq!(
            parse_mtlx(sourceless).unwrap_err(),
            MtlxImportError::BadOutputRef {
                input: "in".into(),
                line: 3,
                value: "2".into()
            }
        );
        let sourced = "<materialx version=\"1.39\">\n  <dot name=\"a\" type=\"float\">\n    \
                       <input name=\"in\" type=\"float\" nodename=\"x\" output=\"2\" />\n  \
                       </dot>\n</materialx>\n";
        assert_eq!(
            parse_mtlx(sourced).unwrap_err(),
            MtlxImportError::BadOutputRef {
                input: "in".into(),
                line: 3,
                value: "2".into()
            }
        );
    }

    // -- resource caps (§3.5.3) ----------------------------------------------

    fn nested_nodegraphs(n: usize) -> String {
        let mut s = String::from("<materialx version=\"1.39\">\n");
        for i in 0..n {
            s.push_str(&format!("<nodegraph name=\"g{i}\">\n"));
        }
        for _ in 0..n {
            s.push_str("</nodegraph>\n");
        }
        s.push_str("</materialx>\n");
        s
    }

    #[test]
    fn nodegraph_depth_cap_is_64_and_65_is_a_typed_error() {
        let (doc, report) = parse(&nested_nodegraphs(64));
        assert_eq!(doc.nodegraphs().len(), 64);
        assert_eq!(doc.nodegraphs()[63].depth, 64);
        // version + one inlined-nodegraph line per level.
        assert_eq!(report.entries.len(), 65);
        assert_eq!(
            parse_mtlx(&nested_nodegraphs(65)).unwrap_err(),
            MtlxImportError::DepthExceeded { depth: 65, cap: 64 }
        );
    }

    #[test]
    fn input_count_cap_is_1024_per_element() {
        let node = |n: usize| {
            let mut s = String::from(
                "<materialx version=\"1.39\">\n  <image name=\"big\" type=\"color3\">\n",
            );
            for i in 0..n {
                s.push_str(&format!(
                    "    <input name=\"i{i}\" type=\"float\" value=\"0\" />\n"
                ));
            }
            s.push_str("  </image>\n</materialx>\n");
            s
        };
        let (doc, _) = parse(&node(1024));
        assert_eq!(doc.node("big").unwrap().inputs.len(), 1024);
        assert_eq!(
            parse_mtlx(&node(1025)).unwrap_err(),
            MtlxImportError::TooManyInputs {
                element: "/image[big]".into(),
                line: 2,
                count: 1025,
                cap: 1024
            }
        );
    }

    #[test]
    fn document_size_cap_is_16_mib() {
        assert_eq!(MAX_DOCUMENT_BYTES, 16_777_216);
        // At the cap the size check passes (and the all-whitespace source
        // is then an empty document); one byte over is the typed error.
        let at_cap = " ".repeat(16_777_216);
        assert_eq!(
            parse_mtlx(&at_cap).unwrap_err(),
            MtlxImportError::Syntax {
                line: 1,
                message: "empty document".into()
            }
        );
        let over = " ".repeat(16_777_217);
        assert_eq!(
            parse_mtlx(&over).unwrap_err(),
            MtlxImportError::DocumentTooLarge {
                bytes: 16_777_217,
                cap: 16_777_216
            }
        );
    }

    // -- garbage: Err or report, never panic ---------------------------------

    #[test]
    fn garbage_inputs_are_typed_errors() {
        assert_eq!(
            parse_mtlx("").unwrap_err(),
            MtlxImportError::Syntax {
                line: 1,
                message: "empty document".into()
            }
        );
        // Truncated tag.
        assert_eq!(
            parse_mtlx("<materialx version=\"1.38\">\n  <image").unwrap_err(),
            MtlxImportError::Syntax {
                line: 2,
                message: "unterminated <image>".into()
            }
        );
        // Unbalanced quote: the value runs to end of input.
        assert_eq!(
            parse_mtlx("<materialx version=\"1.38>\n</materialx>\n").unwrap_err(),
            MtlxImportError::Syntax {
                line: 3,
                message: "unterminated attribute value".into()
            }
        );
        // Mismatched close.
        assert_eq!(
            parse_mtlx("<materialx>\n  <a>\n  </b>\n</materialx>").unwrap_err(),
            MtlxImportError::Syntax {
                line: 3,
                message: "mismatched close </b> for <a>".into()
            }
        );
        // Wrong root.
        assert_eq!(
            parse_mtlx("<?xml version=\"1.0\"?>\n<foo />").unwrap_err(),
            MtlxImportError::NotMaterialx {
                line: 2,
                found: "foo".into()
            }
        );
        // 10^5 nested elements: the tokenizer's explicit stack hits the
        // shared element cap at the 257th open element — no recursion, no
        // overflow.
        let deep = format!("<materialx version=\"1.39\">{}", "<a>".repeat(100_000));
        assert_eq!(
            parse_mtlx(&deep).unwrap_err(),
            MtlxImportError::ElementDepthExceeded {
                depth: 257,
                cap: 256,
                line: 1
            }
        );
        // 10^5 nested nodegraphs: same guard first.
        let deep_graphs = format!(
            "<materialx version=\"1.39\">{}",
            "<nodegraph name=\"g\">".repeat(100_000)
        );
        assert!(matches!(
            parse_mtlx(&deep_graphs),
            Err(MtlxImportError::ElementDepthExceeded { depth: 257, .. })
        ));
    }

    #[test]
    fn every_prefix_of_a_fixture_parses_or_errs_without_panicking() {
        // Truncation at every char boundary: only the two prefixes that
        // end at or after `</materialx>` (with / without the final newline)
        // are complete documents.
        let src = fixture(SP_USD_UDIM);
        let mut oks = 0;
        for cut in 0..=src.len() {
            if let Some(prefix) = src.get(..cut) {
                if parse_mtlx(prefix).is_ok() {
                    oks += 1;
                }
            }
        }
        assert_eq!(oks, 2);
    }

    #[test]
    fn root_without_version_warns() {
        let (doc, report) = parse("<materialx />\n");
        assert_eq!(doc.version, None);
        assert_eq!(
            report.render(),
            "warning report L1 /materialx: no version= on <materialx> (the attribute is \
             required); nothing assumed [§4.1.1]\nsummary: nodes 0, nodedefs 0, nodegraphs 0, \
             ports 0, includes 0, other 0; info 0, warning 1; preserve 0, report 1\n"
        );
        assert_eq!(emit_mtlx(&doc), "<materialx />\n");
    }

    // -- canonical layout + verbatim values ----------------------------------

    #[test]
    fn non_canonical_layout_normalizes_but_values_stay_verbatim() {
        let src = "<materialx version=\"1.39\"><image name=\"a\"   type=\"color3\"><input \
                   name=\"file\" type=\"filename\" value='x\"y&apos;.png'/></image><!-- c > d \
                   --><doc>  note  </doc><empty></empty></materialx>";
        let (doc, _) = parse(src);
        let expected = "<materialx version=\"1.39\">
  <image name=\"a\" type=\"color3\">
    <input name=\"file\" type=\"filename\" value='x\"y&apos;.png' />
  </image>
  <!-- c > d -->
  <doc>note</doc>
  <empty>
  </empty>
</materialx>
";
        let emitted = emit_mtlx(&doc);
        assert_eq!(emitted, expected);
        // Idempotent.
        assert_eq!(emit_mtlx(&parse(&emitted).0), expected);
        // The decoded view of the single-quoted value.
        assert_eq!(
            input(doc.node("a").unwrap(), "file").value.as_deref(),
            Some("x\"y'.png")
        );
    }

    #[test]
    fn entity_decoding_is_exact_and_unknown_entities_stay_literal() {
        assert_eq!(decode_entities("a&lt;UDIM&gt;b"), "a<UDIM>b");
        assert_eq!(decode_entities("&#65;&#x42;&amp;&quot;&apos;"), "AB&\"'");
        assert_eq!(decode_entities("&tex;/x & y"), "&tex;/x & y");
        assert_eq!(decode_entities("&#xZZ;"), "&#xZZ;");
    }

    #[test]
    fn stringarray_splits_with_backslash_escapes() {
        let src = "<materialx version=\"1.39\">
  <constant name=\"k\" type=\"stringarray\">
    <input name=\"value\" type=\"stringarray\" value=\"a\\,b, c\\;d,e\" />
    <input name=\"plain\" type=\"string\" value=\"a,b\" />
  </constant>
</materialx>
";
        let (doc, _) = parse(src);
        let k = doc.node("k").unwrap();
        assert_eq!(
            input(k, "value").string_array(),
            Some(vec!["a,b".to_string(), "c;d".to_string(), "e".to_string()])
        );
        assert_eq!(input(k, "plain").string_array(), None);
        assert_eq!(split_string_array(""), Vec::<String>::new());
    }

    #[test]
    fn bom_is_kept_and_re_emitted() {
        let src = "\u{feff}<materialx version=\"1.38\" />\n";
        let (doc, _) = parse(src);
        assert!(doc.bom);
        assert_eq!(emit_mtlx(&doc), src);
    }
}
