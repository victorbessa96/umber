# Kill report — wave-materialx-roundtrip Kill A: MaterialX parse-only import + report

Date: 2026-10-14. Lock `mtlx-kill-a`. MAIN tree.
- No cargo of any kind, no commits, no git writes.
- `scripts/verify-kill.sh` and bare `rustfmt --check` were BOTH refused by
  this session's permission gate. Neither ran; see §5.

Surface:
- `crates/umber-graph/src/mtlx_import.rs` (new);
- `crates/umber-graph/src/lib.rs` (one line, `pub mod mtlx_import;` after
  `pub mod mtlx_doc;`);
- this report.

The six fixtures were read and kept UNCHANGED; none was edited and none
was added. `mtlx.rs`, `mtlx_doc.rs` and `Cargo.toml` were read, never
written.

Read before writing:
- `CLAUDE.md`, in full;
- `docs/research/materialx-roundtrip.md`, in full (§2.4–2.6, §3.5, §4,
  §5.1, §5.4, §7 per the brief);
- `mtlx.rs` L1–80 and L240–760: the writer, `esc`/`unesc`, `Elem`,
  `Cursor`, `MAX_ELEM_DEPTH`, `parse_doc`;
- `mtlx_doc.rs`, in full;
- `lib.rs`, in full;
- `topo.rs` (the `Graph` API);
- `clippy.toml`;
- the workspace `Cargo.toml` (edition 2021);
- all six fixtures;
- one prior kill report, for its format.

## 1. Built

**`crates/umber-graph/src/mtlx_import.rs`** (new). Spec: §5.1.1–5.1.2.

- **The tokenizer, `Lexer`.**
  - An explicit stack with no recursion.
  - It reuses `crate::mtlx::MAX_ELEM_DEPTH` (256) as the element guard and
    returns `ElementDepthExceeded { depth, cap, line }`.
  - It keeps attributes RAW: the escaped bytes plus the quote character,
    in source order.
  - It keeps comments, PIs, text (trailing whitespace trimmed), CDATA, and
    a DOCTYPE with an internal subset; quotes and brackets are tracked.
  - It keeps the self-closing form and a UTF-8 BOM.
  - Every slice goes through `str::get`. There is no `unwrap`, `expect` or
    indexing on the parse path.
- **The lossless IR.** All public types derive
  `Debug, Clone, PartialEq, Eq`.
  - `MtlxDocument`: BOM flag, prolog, `version`, ordered `root_attrs`,
    `items`, epilog. Helpers: `all_items`, `nodes`, `node(name)`,
    `nodedefs`, `nodegraphs`.
  - `MtlxItem`: `Node`, `Nodedef`, `Nodegraph`, `Port`, `Include`, `Other`
    (unknown elements, verbatim) and `Misc`.
  - `MtlxNode`: the verbatim element plus `category` (the tag), the free
    `name`, `ty`, a `version` request, `scope` (the ancestor chain),
    effective `fileprefix` / `colorspace`, and `inputs`.
  - `MtlxInput`: `name`, `ty` (verbatim), `value` (decoded; `<UDIM>`
    literal), `connection`, `interfacename`, `colorspace`, `member`,
    `line`, and `string_array()` (§4.5.5).
  - `MtlxConnection`: `Node { node, output }`,
    `Graph { graph, output }`, `LegacyEdge { id }`,
    `PortOnly { output }`.
  - `MtlxNodedef`: identity, `node`, `version`, `isdefaultversion`,
    `inputs`, `outputs`, and `implementations` (the names of the
    `nodedef=`-bound bodies).
  - `MtlxNodegraph`: `name`, `nodedef`, `scope`, `depth`, `attrs`,
    `items`.
  - `MtlxPort`, `MtlxInclude`, `MtlxAttr::value()`.
- **`pub fn parse_mtlx(src) -> Result<(MtlxDocument, ImportReport),
  MtlxImportError>`.** Never panics. Its error, `MtlxImportError`, is
  thiserror-derived, with no `source` field, and has 7 variants:
  - `DocumentTooLarge { bytes, cap }`;
  - `ElementDepthExceeded { depth, cap, line }`;
  - `DepthExceeded { depth, cap }`, exactly the brief's shape;
  - `TooManyInputs { element, line, count, cap }`;
  - `Syntax { line, message }`;
  - `NotMaterialx { line, found }`;
  - `BadOutputRef { input, line, value }` (§4.5.4).
- **`pub fn emit_mtlx(&MtlxDocument) -> String`.** The canonical re-emit
  (house layout). It is byte-identical for any document already in that
  layout, and idempotent otherwise.
- **`ImportReport` (§3.5.2).**
  - `entries: Vec<ReportEntry { path, line, severity, disposition, reason,
    cite }>` plus `summary: ReportSummary`. The summary carries the census
    (nodes, nodedefs, nodegraphs, ports, includes, other) and counts by
    severity and by disposition.
  - `render()` writes one line per entry plus the summary.
  - `max_severity()`.
  - `Severity { Info < Warning }`; `Disposition { Preserve, Report }`.
  - The full construct → severity/disposition/cite table is in the module
    docs.
- **Caps (§3.5.3):**
  - nodegraph depth 64, by counted recursion;
  - 1024 `<input>`s per element;
  - 16 MiB document, checked before tokenizing;
  - element depth 256, shared with `mtlx.rs`.

**Tests added: 24**, inline `#[cfg(test)] mod tests`. Fixtures come in via
`include_str!("../tests/fixtures/mtlx/…")`, CRLF-normalized (see §2.9).

1. `every_fixture_re_emits_byte_identically`: all 6, including the lossy
   hazards file.
2. `blender_flat_report_is_exact`: full render; flat scope; float-typed
   edge; UV binding verbatim.
3. `blender_standard_surface_report_is_exact`: `"0"` stays `"0"`.
4. `sp_usd_creator_report_is_exact_and_udim_is_verbatim`:
   - the decoded value is `…<UDIM>…`;
   - the raw value is `…&lt;UDIM&gt;…`;
   - the emitted text contains the raw form.
5. `usd_inlined_nodegraph_report_is_exact`: depth 1, scope chain,
   `nodegraph=`+`output=`.
6. `nodedef_instanced_report_scopes_and_identity_are_exact`:
   - identity, interface, version and body;
   - scope chains `[NG_rust_mask_float, NG_inner]`;
   - depths `[1, 2]`;
   - `interfacename` binding.
7. `hazards_report_is_exact`: 20 entries plus the summary, exact.
8. `hazards_ir_keeps_every_construct_as_data`:
   - version `1.36` and the root-attribute order;
   - the DOCTYPE raw, with its internal subset;
   - the include `href`;
   - `fileprefix` per node, unjoined;
   - the effective colorspace;
   - `LegacyEdge { id: 12 }` and `PortOnly { "outr" }`;
   - `member`;
   - the loose text.
9. `reimport_of_an_unchanged_file_yields_a_byte_identical_report`.
10. `own_material_document_parses_non_empty_and_clean` (SELF-HOSTING):
    - parses `to_mtlx_document(full set, with a <UDIM> base color)`;
    - 11 nodes;
    - `max_severity == Info`;
    - exact summary (info 7, warning 0);
    - byte-identical re-emit;
    - `<UDIM>` verbatim;
    - `float`-typed extract edge.
11. `own_painter_graph_document_round_trips_through_the_import`:
    - `to_mtlx(graph, &painter_nodedefs())` re-emits byte-identically;
    - 9 nodedefs;
    - legacy edge `{ id: 1 }`;
    - exact summary (info 13).
12. `output_attribute_is_classified_by_form`: sourceless `N7` is a legacy
    edge; `nodename`+`output="N7"` is a port select; `nodename`+`outr`;
    exact entry text.
13. `output_that_is_neither_spelling_is_a_typed_error`: `output="2"`,
    sourceless and sourced → `BadOutputRef { line: 3 }`.
14. `nodegraph_depth_cap_is_64_and_65_is_a_typed_error`: 64 → Ok (65
    entries); 65 → `DepthExceeded { depth: 65, cap: 64 }`.
15. `input_count_cap_is_1024_per_element`: 1024 → Ok; 1025 →
    `TooManyInputs { "/image[big]", 2, 1025, 1024 }`.
16. `document_size_cap_is_16_mib`: exactly 16 MiB passes the cap (→
    "empty document"); +1 byte → `DocumentTooLarge`.
17. `garbage_inputs_are_typed_errors`:
    - empty input;
    - truncated tag (L2);
    - unbalanced quote (L3);
    - mismatched close (L3);
    - wrong root (L2);
    - 10^5 nested `<a>` → `ElementDepthExceeded { 257, 256, 1 }`;
    - 10^5 nested nodegraphs → the same guard.
18. `every_prefix_of_a_fixture_parses_or_errs_without_panicking`: a
    truncation sweep; exactly 2 prefixes are Ok.
19. `root_without_version_warns`: exact render (§4.1.1).
20. `non_canonical_layout_normalizes_but_values_stay_verbatim`:
    - a single-quoted value containing `"` and `&apos;`;
    - a comment containing `>`;
    - an inline text element;
    - `<x></x>`, which re-emits in the block form;
    - exact canonical output, and idempotence.
21. `entity_decoding_is_exact_and_unknown_entities_stay_literal`.
22. `stringarray_splits_with_backslash_escapes` (§4.5.5).
23. `bom_is_kept_and_re_emitted`.
24. `own_writer_empty_elements_re_emit_byte_identically`: `to_mtlx` of an
    empty graph (exact string pinned), a param-less unconnected node and an
    input-less nodedef. Each writes an open line plus a close line, and
    each re-emits byte-identically.

Test 4's UDIM check and test 10's self-hosting check each pin several
§5.1.3 items. Every count, line and
string was derived by hand from the fixture bytes and the module-doc
table; none is a range check.

**`crates/umber-graph/src/lib.rs`**: `pub mod mtlx_import;` (L23). No
other edit.

## 2. Judgment calls (all also recorded in the module docs)

1. **The tokenizer is DUPLICATED and rewritten; neither `Elem` nor
   `Cursor` is reused.**
   - `Elem` unescapes values, so `&apos;` and single-quoted values can't
     re-emit. It also drops text, comments and the self-closing form.
   - Reading `Cursor` turned up two defects, which I left as-is because
     they are out of surface:
     1. Its DOCTYPE skip stops at the first `>` inside an internal subset,
        so `hazards_lossy.mtlx` would fail with "unexpected text outside
        the root element".
     2. Its comment arm is dead code: both callers consume `<!` before
        `skip_bang` tests `!--`, so a comment containing `>` ends early.
   - Both are flagged for the dedupe follow-up (§4).
   - The only thing shared is the `MAX_ELEM_DEPTH` constant.
2. **Typed items carry their verbatim `MtlxElement` and a derived parsed
   view.** Emission reads only the element, which makes it lossless by
   construction. The view is a read model; nothing mutates it in kill A.
3. **Canonical re-emit, not whitespace-preserving.** "Byte-identical"
   holds for the house layout, which covers both Umber writers and all
   six fixtures. Other layouts normalize inter-tag whitespace only.
   - A self-closed element re-emits as ` />`.
   - A non-self-closed EMPTY element re-emits in the writers' block form:
     an open line, then a close line at the same indent. That is how
     `to_mtlx` writes an empty graph, a param-less node, or an input-less
     nodedef.
   - A source `<x></x>` written on one line is therefore normalized onto
     two lines.
   - Advisor review caught this: the first draft collapsed every empty
     element to `<x></x>`, which broke the `to_mtlx` round-trip for those
     shapes. Test 24 pins the fix.
4. **Q6 (where scope data lives): both.** Nodes and nodegraphs keep the
   raw chain (`scope`, outermost first). Nodes also keep the effective
   `fileprefix` / `colorspace`, unjoined and unapplied.
5. **`output=` classification by form.**
   - A sourceless `N<digits>` is `LegacyEdge`.
   - With a `nodename=` or `nodegraph=` source, `output=` is always a port
     select, even `N7`.
   - Port names are `[A-Za-z_][A-Za-z0-9_:]*`.
   - A sourceless port name is `PortOnly` (reported).
   - Anything else is `Err(BadOutputRef)`. That makes it a hard error for
     the whole document, per §4.5.4's "typed error"; I did not downgrade it
     to a report entry.
6. **Severity and disposition.**
   - Only two severities: `Info < Warning`. Kill A has no use for a third.
     "Zero entries above info" is `max_severity() == Some(Info)`.
   - Two dispositions: PRESERVE and REPORT. IMPORT arrives with kill B.
7. **Recognized colorspaces: exactly `srgb_texture`, `lin_rec709`,
   `none`** (§4.4.2). `raw` (three times in the SP fixture) and `acescg`
   therefore WARN. This is deliberate: they are CMS-relative names this
   kill can't vouch for. Revisit when kill C lands the mapping.
8. **Version: 1.38, 1.39 and 1.39.x are info; anything else warns;
   absent warns** (§4.1.1, §4.1.2). The hazards fixture's 1.36 warns.
9. **Windows CRLF.** The repo has no `.gitattributes`, so a Windows CI
   checkout may hand `include_str!` CRLF text. The test helper
   `fixture()` normalizes `\r\n` → `\n` before parsing. The parser itself
   accepts `\r` as whitespace. A cleaner fix is a `.gitattributes` rule
   (`*.mtlx text eol=lf`), which is out of surface (§4).
10. **Caps.**
    - Inputs: 1024 per element. The widest real interface, the vendored
      OpenPBR nodedef, has under 100.
    - Document: 16 MiB. Real files are KB-scale; full stdlib libraries are
      hundreds of KB. The cap is checked first, so it is the parse's
      allocation bound.
    - Nodegraph depth: 64, per §4.2.2.
    - Recursion: the tokenizer uses an explicit stack. The IR builder uses
      counted recursion, checked before descending.
11. **Classification rules.**
    - A root/nodegraph child carrying `type=` is a node, unless its tag is
      a known non-node element (`look`, `typedef`, `implementation`, …;
      the list is in `NON_NODE_ELEMENTS`).
    - The painter dialect's `<node type=…>` is a node with
      `category = "node"` (verbatim) plus an info entry.
    - `input`, `output` and `token` directly under root or a nodegraph are
      ports.
12. **Resolution scope.**
    - `nodename=` resolves against sibling nodes.
    - `nodegraph=` resolves against sibling or root-level graphs.
    - `interfacename=` resolves against the enclosing nodegraph's
      `<input>`s plus the inputs of the nodedef it implements.
    - Duplicate names are checked among all named element children of one
      scope.
13. **Metadata** (`doc`, `ui*`, `nodegroup`, …) is aggregated into ONE
    info entry per element (§4.5.6, "verbose mode"). Any other
    unrecognized attribute gets an info / preserve entry (§4.1.3).

## 3. Unverified (no cargo; nothing below has been compiled or run)

- **The whole module.** Re-read against the CLAUDE.md trap list:
  - `matches!` uses 2 arguments, guards inline;
  - no `source` fields;
  - no struct/enum inside `impl`;
  - every format-string interpolation is a bare local (`{v}`, `{tag}`) or
    positional;
  - one `mod` line;
  - no new deps.
- **Borrow spots I'm least sure of**, in order of doubt:
  - `Builder::attr_entries`: `meta: Vec<&str>` borrows `attrs` while
    calling `self.push`. They are disjoint, since `attrs` is not part of
    `self`.
  - `build_items`: `path` is moved in the `Nodegraph` arm and borrowed in
    the others.
  - `build_input`: `match (legacy, nodename, nodegraph)` moves the
    `Option<String>`s after `has_source` / `conflicting` were computed.
  - The match-arm guard `!self.root_graphs.contains(graph)`, followed by a
    `self.push` in the arm body.
  - `lex`: `stack.last().map(|e| e.tag.as_str()).unwrap_or_default()`
    (`Option<&str>`).
- **API shapes assumed.** `Option<&str>::unwrap_or_default` (`&str:
  Default`). The tuple-coercion concern is avoided by passing
  `path.as_str()`.
- **Report pins.** The exact report text in tests 2–8, 10, 11, 12 and 19
  is HAND-DERIVED. This is the likeliest red in the battery:
  - an off-by-one fixture line;
  - an entry order I mis-walked;
  - a wording mismatch between a test literal and the `format!` that
    produces it.
  - If a pin fails, the module-doc table is the spec. Fix the side that
    disagrees with it.
- **rustfmt.** Not run; the gate was refused. I hand-applied the default
  rules: width 100, `fn_call_width` 60, `struct_variant_width` 35, chain
  width 60, trailing commas on vertical lists. Borderline spots:
  - the test literal `MtlxImportError::DepthExceeded { depth: 65, cap: 64 }`
    sits at exactly `struct_lit_width` 18;
  - the `emit_item` `Nodegraph` arm has a 59-wide argument list.
  - `cargo fmt --all` may still move a line or two.
- **clippy.** Not run.
  - `large_enum_variant` on `MtlxItem` was size-estimated (largest `Port`
    ≈320 B vs `Node` ≈280 B; under the 200 B gap) but not measured.
  - Edition 2021 means nested `if let { if … }` does NOT trip
    `collapsible_if`.
- **rustdoc.** Public docs avoid intra-doc links to the private `Lexer`
  and the `pub(crate)` `MAX_ELEM_DEPTH` (`private_intra_doc_links`).
- **Runtime cost.** The 16 MiB test allocates and scans 32 MiB of spaces
  across two strings, a fraction of a second even in a debug build. The
  prefix sweep parses about 1.9 k prefixes of a 1.9 KB fixture.

## 4. Deferred

- **Tokenizer dedupe** between `mtlx.rs` and `mtlx_import.rs`, including
  fixing `mtlx.rs`'s two `Cursor` defects (§2.1). That needs `mtlx.rs`
  edits, which are out of surface; recorded in the module docs.
- **`[interface token]` filename substitution, and `output=` validation
  against the source's declared outputs.** Kill B / kill S, which needs
  stdlib signatures (§7 Q2). Recorded in the module docs, "v1
  boundaries".
- **Namespace-collision analysis.** Nothing is flattened yet, so no
  collision can occur. Module docs.
- **§7 Q1 real-corpus validation.** There is no `.mtlx` corpus in the
  tree, so the IR is frozen against the six hand-authored fixtures only.
  The first real Blender, Designer or SP-USD export should be run through
  `parse_mtlx` before kill B builds on the IR.
- **A `.gitattributes` `*.mtlx text eol=lf` rule.** Out of surface; the
  test helper covers it in the meantime.
- **Requirements row.** Kill A doesn't complete a `requirements.md` row
  (the §4 engine row needs kills B and C), and the file is out of surface
  anyway. Nothing was marked.

## 5. Gates run

- `bash scripts/verify-kill.sh crates/umber-graph/src/mtlx_import.rs
  crates/umber-graph/src/lib.rs`: **NOT RUN.** This session's permission
  gate refused the command ("This command requires approval").
- `rustfmt --edition 2021 --check …`: **NOT RUN.** Refused the same way.
- `cargo fmt` / `clippy` / `test`: **NOT RUN.** Cargo is forbidden by the
  brief.
- Manual checks that did run:
  - **Fixture bytes:** all six are LF-only, with no tabs, no trailing
    whitespace, no blank lines, and one final `\n` (`grep`, `od`). The
    byte-identity pins depend on this.
  - **Intra-tag spacing:** all six are canonical. Each of these greps
    returned nothing:
    - `grep '"  '` (no double space after a value);
    - `grep ' >'` (no space before `>`);
    - `grep '"/>'` (every self-close is ` />`).
  - **Long lines** (over 100 columns): the remaining ones are only
    unbreakable string literals, doc-table rows and report-text lines
    inside raw test literals.
- The dragon's Gate 1 (`bash scripts/wave-battery.sh umber-graph`) is the
  first real compile.
