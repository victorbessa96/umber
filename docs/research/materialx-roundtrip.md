# MaterialX round-trip — what the import actually has to consume

Date: 2026-10-14. Kill: WAVE-5 RESEARCH KILL (materialx-research,
import research doc).

**Scope box (binding).** This doc is the kill's contract: the future
import kills implement what this doc documents, nothing more. The
research question is narrow — not "what is MaterialX" but "what do
real DCC-exported `.mtlx` files contain in practice, what does Umber
already write and parse, and what is the staged path between the
two." Every numbered claim cites its source inline. Citation keys
are resolved in §6. In-tree claims cite `path:line` read LIVE from
this tree on 2026-10-14; nothing below is recalled from an older
revision. External claims were read on 2026-10-14 through the
sources named in §6; where only a search extract (not the full
document) was read, the claim is marked `(extract-read)` and the
exact quoted wording is reproduced so the implementing kill can
verify it against the full text. Anything I could not verify is not
a claim at all — it is an Open Question in §7.

**Standing context the roadmap gives this doc.** The requirements
row for the procedural engine (`docs/specs/requirements.md:65`,
[REQ]) contracts that graphs serialize as MaterialX documents,
standard-equivalent nodes map to standard nodes, painter-specific
nodes ship as declared custom nodedefs, and "no automatic
full-fidelity interchange is claimed." The next-gen bar ([04] §14)
names the consuming direction as the differentiator: author-time
graphs serialize as MaterialX documents with "free interchange with
Houdini/UE/UsdMtlx." The brief for this kill states the roadmap row
lives at "wave 5." Honest record: `docs/loop/wave-plan.md` as read
live on 2026-10-14 plans waves 1–3 only and names no wave-5 row;
the "wave 5" label is the brief's, and the differentiator sentence
is [04]'s. The declared-custom-nodedef contract the brief says
"already ships" is `crates/umber-graph/src/mtlx.rs` (§2 below).

**The one-sentence finding.** Umber's writer and reader today speak
a private painter dialect of MaterialX XML (`<node>` +
`output="N{id}"`, `mtlx.rs:18-36`); every real DCC export speaks the
standard element shape (category-named elements + `nodename=`
wiring, §1). Today's `from_mtlx` does not degrade gracefully on a
single real DCC file — it *errors* (§2.6). The import is therefore
not an extension of the reader but a second reader sharing the
diagnostic doctrine, staged parse-first (§5).

---

## 1. What real DCC-exported `.mtlx` files contain (practice, not spec)

### 1.1. Substance Painter exports no MaterialX natively — the shapes come from Designer, plugins, and USD

1.1.1. Painter's own import/export matrix has no MaterialX row:
"Materials import: SBSAR (in), MDL (in/out), GLSL shader files
(in). MaterialX: not in Painter (Designer only)" [01 §7]. Any
statement of the form "Painter writes .mtlx shaped like X" is
therefore false for stock Painter as inventoried; the practice
shapes below are what Painter-adjacent tooling actually emits.

1.1.2. Substance Designer has a MaterialX graph category and
export path: the Designer 10.1 (2020.1) release notes announce a
"New MaterialX Plugin (Beta)" demonstrated at SIGGRAPH 2019 that
"allows to create MaterialX materials to be used in other
compatible DCC application," with follow-up fix lines for a
"MaterialX Graph category" that "does not display all the available
nodes" [DES-101]. Practice consequence: Designer-authored exports
are node-graph documents built from the standard library palette
the plugin exposes — i.e. standard-shape documents (§1.4), not
painter-dialect documents (§2).

1.1.3. The earliest Painter-side exporter is the community plugin
AllegorithmicSAS/materialx_utils, whose README scopes itself
exactly: "This plugin adds support for simple MaterialX support
for Substance Painter. It currently only support arnold and
Metallic/Roughness workflows" [MATXUTILS] (MaterialX 1.36 era,
Python 2.7). Practice consequence: Painter-origin .mtlx in the
wild, where it exists at all, is a *baked-texture wiring*
document (image nodes feeding a fixed shader), not a procedural
graph — the same shape Umber's own `mtlx_doc.rs` emits (§2.4),
arrived at independently.

1.1.4. The current third-party Painter→MaterialX path is the SP
USD Creator plugin, which "exports material networks for USD
Preview, Arnold, MaterialX Standard Surface, and OpenPBR" as part
of a layered USD publish (`Asset.usd`, `payload.usdc`,
`mtl.usdc`, optional `geo.usdc`, textures under `textures/`)
[SPUSD-USER]. Its developer guide records three practice facts
that constrain any Umber consumer [SPUSD-DEV]:
(a) texture slots are normalized out of Painter's
`on_post_export` payload, which is shaped
`Dict[Tuple[material_name, texture_set], List[absolute paths]]`
— filenames carry the semantics, parsed by token, and "the parser
ignores unknown texture tokens";
(b) "Metalness and roughness textures are wired as float inputs
end-to-end" (the scalar-map convention Umber's `<extract>`
chains also implement, `mtlx_doc.rs:303-318`);
(c) "When OpenPBR is enabled, the exporter skips MaterialX
Standard Surface to avoid duplicate outputs" — real exports pick
ONE surface model per material, so an importer must accept both
`standard_surface` and `open_pbr_surface` terminals (§3.3).

1.1.5. Adobe's own portability doctrine bounds what "supporting
DCC .mtlx" can promise. The OpenPBR FAQ states that
implementations vary ("some cover [the] full spec, [others] only
a subset"), that "portability should be actively validated, not
assumed," that "the most robust exchange path for shared material
libraries is via USD with MaterialX integration," and that
"workflows for exporting materials as standalone assets (without
an associated model, for use in a shared library) are still
under active development and not yet fully supported
everywhere" [OPENPBR-FAQ] `(extract-read)`. Practice consequence
for §3 and §5: the import's promise is *typed reports over
silent evaluation* even where both sides say "OpenPBR," because
two OpenPBR implementations need not evaluate the same subset.

### 1.2. Blender exports MaterialX through the Hydra path — the best-documented practice corpus

1.2.1. Blender's MaterialX support exists to feed Hydra render
delegates, not to write standalone `.mtlx` files: PR #111765
("Export material to MaterialX for Hydra render") adds
`bl_use_materialx` to the render-engine property so that
"MaterialX is utilized for exporting materials to Hydra," moving
material export out of the Hydra code path [BL-111765]
`(extract-read)`. The first export set is small and named:
"BSDFPrincipled, Invert, Math, TexChecker, TexEnvironment,
TexImage, TexNoise" [BL-111765] `(extract-read)`. Practice
consequence: the most common real-world generator of
MaterialX-shaped Blender content emits a *closed vocabulary* —
principled shader + image/checker/noise + math/invert — which is
exactly the subset an honest Umber importer should target first
(§3.2).

1.2.2. The surface model moved from Standard Surface to OpenPBR:
issue #138164 ("Hydra: Update MaterialX to export OpenPBR
surface") records "Switching from Standard Surface to OpenPBR as
the exported MaterialX surface in the USD exporter. OpenPBR is
the new industry standard uber shader and is now supported in
USD via MaterialX 1.39.3," closed by Brecht Van Lommel's
"MaterialX: Export Principled BSDF as OpenPBR Surface"
[BL-138164] `(extract-read)`. Practice consequence: post-2025
Blender-origin documents terminate in `open_pbr_surface` — the
same terminal Umber writes (`mtlx_doc.rs:337-340`) — while older
ones terminate in `standard_surface`. Both terminals must parse
(§3.3); neither needs full evaluation (§5, "never").

1.2.3. Blender's export does NOT use `<nodegraph>` wrappers by
default, for a UsdMtlx reason: group-node export exists in
`materialx/group_nodes.cc/.h`, but it sits behind
`USE_MATERIALX_NODEGRAPH`, "Disabled by default because
`pxr::UsdMtlxRead()` doesn't perform nodegraphs" [BL-111765]
`(extract-read)`. Practice consequence (two of them): first,
Blender-origin files are flat node lists under the root, which
is the one foreign topology today's `from_mtlx` could most
cheaply learn to walk (§2.6, §5 kill B); second, any nested
`<nodegraph>` in the wild is disproportionately likely to come
from hand-authored or non-Blender tooling and to exercise the
nesting hazard (§4.2) — depth cannot be assumed to be 0-or-1.

1.2.4. Blender tracks an explicit per-node support matrix with
four states, and it is the honest-subset precedent §3 follows.
Issue #112864 ("MaterialX export from Blender shader nodes")
lists, inter alia: Mix Color partial ("TODO: Apply blend type"),
RGB Curves unimplemented ("TODO: could be implemented"), Color
Ramp NOT implemented, Script NOT implemented ("TODO: can be
implemented"), Tangent / Layer Weight / Particle Info / Point
Info NOT implemented ("No correspondent node in MatX" /
"MatX doesn't have an implementation for this node"), Material
Output surface-only ("Volume and Displacement aren't"), White
Noise and Map Range implemented [BL-112864] `(extract-read)`.
Practice consequence: even the reference exporter ships a
partial mapping with per-node states (implemented / partial /
default-value / not-implemented) and an open ask for "some kind
of indicator in the UI as to which nodes are supported and not"
[BL-112864] `(extract-read)` — that indicator is precisely the
typed-report doctrine §3.5 requires of the Umber importer.

1.2.5. Texture-coordinate binding is a live interop hazard, not
settled convention. Issue #122800 ("USD MaterialX export missing
textures in Storm") traces missing textures to a UV-primvar
naming mismatch: Preview Surface export always emits a
`UsdPrimvarReader_float2` for the user's named UV map, while the
MaterialX path needs the coordinates under names consuming
renderers agree on; the discussion weighs renaming the active map
to `st` on export (reverted once as #112234 because the shader
exporter renamed the primvar but not the reader references),
exporting dual primvars, or duplicating materials per UV-set
name [BL-122800] `(extract-read)`. Practice consequence: an
importer cannot assume any fixed UV-set name on `texcoord` /
`UsdPrimvarReader` / `geompropvalue` nodes, and must treat the
coordinate binding as data to preserve (report), never as a name
to normalize silently (§4.3.4).

1.2.6. The import direction in Blender is explicitly *younger*
than export and its design notes are import-kill requirements
in miniature. Issue #158138 ("MaterialX Integration Overview")
states the intended layering: "supporting importing from an
in-memory MaterialX Document object using the core MaterialX C++
library. This can then be re-used … for either loading from an
`.mtlx` file or from USD after a conversion (most of which
already has code for)" [BL-158138] `(extract-read)` — i.e.
USD-embedded materials and standalone files converge on ONE
document reader, which is the architecture §5 kill A adopts
(parse to an IR first; USD-extracted and file-loaded documents
enter through the same gate). The same issue notes native
MaterialX editing needs socket types Blender lacks (`color4`,
`matrix33`, `matrix44`, `filename`, arrays) [BL-158138]
`(extract-read)` — a checklist Umber's reader must compare
against its own value space (`ParamValue`, `lib.rs:79-94`)
before claiming any type (§2.5.5).

1.2.7. Translation-based import loses graph identity — the
failure mode that justifies §3.4's custom-nodedef contract. The
GSoC 2026 MaterialX-authoring proposal observes that the
Activision `io_data_mtlx` addon "flattens via
`mx.flattenSubgraphs()` and map[s] onto Blender shader nodes,"
so "every MaterialX node is dissolved into Blender primitives,
… the graph loses its MaterialX identity and cannot
round-trip," and proposes nodedef-identity-preserving native
nodes instead [BL-GSOC26] `(extract-read)`. Practice
consequence: Umber's importer must keep the same discipline —
a foreign custom node that has no Umber analogue is preserved
opaque with its identity (name + params) intact, exactly as
`from_mtlx` already does for unknown types (§2.2.6), and is
never dissolved into a lookalike primitive.

### 1.3. UsdMtlx: the container most DCC MaterialX actually travels in

1.3.1. The pipeline spine is USD with embedded MaterialX, not
standalone files. The next-gen bar states OpenUSD 26.08
"integrates MaterialX natively" and frames
"UsdShade → OpenPBR ↔ .mtlx" round-trip as "the film-pipeline
door-opener," with the pragmatic Umber path "usda text
import/export first, binary usdc via the C++ SDK FFI later"
[04 §11]. The OpenPBR FAQ concurs that USD-with-MaterialX is
the robust exchange path [OPENPBR-FAQ] `(extract-read)`.

1.3.2. UsdMtlx has a known nodegraph-shape gap that constrains
what embedded documents look like. Per §1.2.3,
`pxr::UsdMtlxRead()` "doesn't perform nodegraphs," which is why
Blender disables nodegraph emission for the USD path
[BL-111765] `(extract-read)`; the GSoC proposal adds that
"USD has trouble with nodedef/nodegraph pairs when they are
inlined in documents, so the export just writes the contents of
each nodegroup as a nodegraph without fancy instancing"
[BL-GSOC26] `(extract-read)`. Practice consequence: USD-embedded
MaterialX skews FLAT (node lists, group contents inlined as
bare nodegraphs), while standalone `.mtlx` may carry the full
nodedef+nodegraph-instancing shape. One reader must accept both
(§5 kill A), and the report must distinguish "inlined group
contents" from "nodedef-backed instances" because only the
latter carries reusable identity (§3.4).

1.3.3. Umber's USD surface today is mesh-first: the export side
writes "textures + .usda + material binding via UsdShade→OpenPBR"
([REQ] §6 row, line 103) and the import side covers usda
hand-parsing plus usdc/usdz ([REQ] §7 rows, lines 111–112;
`docs/claw-artifacts/openusd/README.md:13-19` on the crate's
usda/usdc/usdz coverage). No in-tree code reads MaterialX out
of a USD layer today — that extraction (find the
`NodeGraph`/`Shader` prims, lift the network into the §5-kill-A
IR) is staged explicitly as kill C's prerequisite, never as a
drive-by inside the mesh importer.

### 1.4. The practice topology census (what the shapes have in common)

1.4.1. Terminals are one of three: `standard_surface` (older
Blender, SP USD Creator default-off path), `open_pbr_surface`
(current Blender, SP USD Creator OpenPBR path, Umber's own
export), or preview-surface (`UsdPreviewSurface` in USD-embedded
practice). All three terminate a `surfacematerial`/`material`
element. An importer that cannot name all three terminals
cannot report on any real file (§3.3).

1.4.2. Upstream of the terminal sits a shallow chain, not a deep
graph: image → (extract | normalmap | primvar-reader) →
shader input, per the SP USD Creator "float inputs end-to-end"
rule [SPUSD-DEV] and Blender's closed export vocabulary
(§1.2.1). Procedural subgraphs (noise/checker driving a shader
input) appear in Blender-origin files; Substance-origin files
are baked-texture wirings (§1.1.3). Depth assumption for the
import: handle arbitrary depth (§4.2), optimize for depth ≤ 4.

1.4.3. Filenames carry substitution tokens that must survive,
not resolve. The spec's Filename Substitutions table defines
`<UDIM>` (Mari-style `1001 + U + V*10`), `<UVTILE>`
(Mudbox-style), `[interface token]`, `{hostattr}`, `{frame}`,
`{0Nframe}`, and states "Implementations are expected to retain
substitution strings within filenames upon export rather than
'baking them out'" [MTLX-SPEC §Filename Substitutions]
`(extract-read)`. Umber's writer already emits `<UDIM>` for
tiled sets (`mtlx_doc.rs:54-58`, UDIM_TOKEN) and its test pins
the escaped round-trip (`mtlx_doc.rs:590-606`). The importer
must preserve these tokens byte-identical on re-emit — a token
resolved to `1001` at import is data loss (§4.3.1).

1.4.4. Color tagging rides per-input `colorspace` attributes
against a document working space, with names the document does
NOT define. The spec makes `colorspace` the working space for
an element and all descendants, allows per-input override "for
a filename or value," and leaves name semantics to the external
CMS configuration (`cms` + `cmsconfig`, OCIO named)
[MTLX-SPEC §Color Spaces] `(extract-read)`; the v1.38 PDF text
adds that the distribution "links to the OpenColorIO
configuration file" for ACES and that documents may name "any
specific custom configuration" [MTLX-SPEC-138 §Color Spaces]
`(extract-read)`. The worked example both texts quote tags an
image `srgb_texture` with a default in `lin_rec709`
[MTLX-SPEC-138 §Color Spaces] `(extract-read)` — the same two
names Umber's writer emits (`mtlx_doc.rs:110-129`). Practice
consequence: every real file's color names are CMS-relative
strings; the importer resolves them against Umber's own
color-managed/data channel rule and reports the unresolvable,
never guesses (§4.4).

---

## 2. Umber's own export contract today (read live 2026-10-14)

### 2.1. Two writers, two dialects, one crate

2.1.1. `crates/umber-graph/src/mtlx.rs` is the painter-graph
writer/reader (`to_mtlx` at `mtlx.rs:304`, `from_mtlx` at
`mtlx.rs:792`). `crates/umber-graph/src/mtlx_doc.rs` is the
texture-set document writer (`to_mtlx_document` at
`mtlx_doc.rs:262`) with NO reader — its module docs state that
`from_mtlx` "skips every element here (none is a `<node>`), so
it parses a document to an EMPTY graph" (`mtlx_doc.rs:13-15`).
Both writers share the header (`<?xml …?>`, `<materialx
version="1.38">`), 2-space indentation, attribute escaping
(`esc`, `mtlx.rs:266-278`) and float formatting
(`mtlx.rs:288-297`), per `mtlx_doc.rs:11-12`.

2.1.2. The version string is fixed `1.38` in both writers
(`mtlx.rs:307`, `mtlx_doc.rs:280`) while the vendored OpenPBR
nodedef is a 1.39 document (`open_pbr_surface.mtlx:2`,
`<materialx version="1.39">`). The honest-scope note in
`mtlx_doc.rs:46-50` records the justification: `open_pbr_surface`
"ships in the 1.39 stdlib, and 1.39 loaders upgrade 1.38
documents on read." The import must therefore accept ≥1.38
document versions without equating "parses" with "evaluates"
(§4.1).

### 2.2. What `to_mtlx` writes (the painter dialect)

2.2.1. Declared custom nodedefs FIRST: every `NodedefDecl`
emits a `<nodedef name=… node=…>` block with `<input name
type />` children before any `<node>` (`mtlx.rs:308-322`),
per the module-doc determinism contract (`mtlx.rs:15-17`,
"the spec's ordering rule / the no-false-interchange
contract"). The ordering is pinned by test
(`mtlx.rs:1072-1089`, `nodedefs_emit_before_nodes`).

2.2.2. Nodes emit as `<node name="N{id}" type="{node_def}">`,
sorted by id (`mtlx.rs:323-330`). Names are synthetic —
`Node` has no name field (`lib.rs:57-75`), so identity is the
integer id, a fact the dynamic-materials design already
records (`docs/specs/dynamic-materials-design.md:103-108`,
cited from the live tree: "mtlx writes node names as `N{id}`").

2.2.3. Connections emit painter-idiomatically: the connected
input carries `output="N{from}"`, ALWAYS typed `color3`
(`mtlx.rs:344-349`: `"type=\"color3\" output=\"N{}\""`), with
connected inputs first (sorted by name) then value params in
graph order, edges sorted by `(to, input)` (`mtlx.rs:32-36`).
Byte-identical output for byte-identical graphs regardless of
edge insertion order is pinned by test
(`mtlx.rs:1133-1162`).

2.2.4. Value params emit as `<input name type value />` with
the `ParamValue` mapping `Float→float`, `Int→integer`,
`Vec2→vector2`, `Vec3→vector3`, `Color→color3`,
`Bool→boolean`, `Asset→string`, `Anchor→anchor`
(`{layer_id}:{channel}`, `mtlx.rs:22-26,390-398`). A stray
`NodeRef` param with no edge is normalized to a connection on
emit; a value param shadowed by an edge on the same input is
dropped in favour of the edge (`mtlx.rs:33-37`). Vectors are
`", "`-joined floats (`mtlx.rs:292-297`); floats use
`f32::to_string` (`mtlx.rs:288-290`); params are assumed
finite (`mtlx.rs:40`).

2.2.5. The reader half of the contract: `from_mtlx` returns
`(Graph, Vec<NodedefDecl>, Vec<String>)` where the warnings
vec holds deduped first-seen type names of nodes "neither
declared in-document nor in `KNOWN_STANDARD_NODES`"
(`mtlx.rs:785-791`). Unknown non-node elements are skipped,
never fatal (`mtlx.rs:905-906` — `<nodegraph>` wrappers ARE
descended into, `mtlx.rs:905`; everything else ignored).
Byte-identical write→read→write is pinned by test
(`mtlx.rs:987-996`, `double_round_trip_is_byte_identical`).

2.2.6. Unknown node types are preserved fully with params
intact and re-emit losslessly (`mtlx.rs:49-52`,
`mtlx.rs:845-850`, test `mtlx.rs:1017-1048`). Unknown input
VALUE types fall back to `Asset(raw)` so no data is lost
(`mtlx.rs:46-48,779-781`) — with the documented spelling
caveat (re-emits as `string`). Malformed anchors are a parse
ERROR, never a silent string (`mtlx.rs:754-778`, test
`mtlx.rs:1091-1131`): "an anchor that loaded as a plain
string would never resolve."

2.2.7. Structural errors are typed with line numbers:
`Parse{line, message}`, `BadId`, `TypeMismatch`,
`DuplicateId`, `DanglingRef` (`mtlx.rs:242-264`). An `<input>`
with both or neither of `value`/`output` is `TypeMismatch`
(`mtlx.rs:862-874`); a duplicate node id is `DuplicateId`
(`mtlx.rs:914-919`); an `output=` reference to a missing node
is `DanglingRef` (`mtlx.rs:921-930`); truncation errors carry
a line within ±2 of the cut (test `mtlx.rs:1050-1070`).

### 2.3. The nodedef machinery: declared customs and the known-standard list

2.3.1. `NodedefDecl{name, category, inputs}`
(`mtlx.rs:75-80`): `name` is `<nodedef name=…>`, `category`
is `<nodedef node=…>`. `painter_nodedefs()` declares nine
customs (`mtlx.rs:83-156`): `flood_fill(mask)`,
`edge_detect(mask, radius)`, `histogram_match(input,
target)`, `brick_pattern(scale, mortar)`,
`direction_warp(input, vector)`,
`mesh_map_generator(map_name)`, `anchor(layer_id, channel)`,
`material_output(input, channel)`, `synth(example, map_mode,
tiling_mode, randomness, seed, mask)`. Semantics per family
are tabled in the module docs (`mtlx.rs:56-68`).

2.3.2. `KNOWN_STANDARD_NODES` (`mtlx.rs:163-240`) is a
heuristic v1 list, and it has three properties the import
kills must know. FIRST, it contains DUPLICATE entries read
live: `color_correct` (lines 205 and 229), `levels` (181 and
237), `invert` (177 and 238), `curves` (204 and 232),
`sharpen` (203 and 233), `dots` (201 and 234), `uniform`
(206 and 235), `image_asset` (207 and 236),
`triplanar_blend` (211 and 231) — harmless today (membership
test only) but evidence the list grew by accretion without a
stdlib cite. SECOND, it lists Umber-custom names as if they
were standard: `flood_fill`, `edge_detect`,
`histogram_match`, `brick_pattern`, `direction_warp`,
`mesh_map_generator`, `noise_perlin`, `noise_value`,
`noise_worley`, `synth` (lines 217-239) alongside genuine
stdlib-shaped names (`noise2d`, `fractal3d`, `place2d`,
`separate2d`, …). THIRD, it lists near-duplicate pairs
without recording which spelling is canonical: `checker`
vs `checkerboard`, `noise` vs `noise2d/noise3d/perlin/worley`,
`mesh_map` vs `mesh_map_generator`, `hsvadjust` vs
`hsv_adjust`, `linear/radial/angular` (stdlib gradient
children?) vs Umber's single `gradient` node. Kill B must
resolve every entry against the stdlib document (§7 Q2);
until then the list is a parse-warning suppressor, NOT a
mapping table, and §3.2 treats it as such.

2.3.3. The engine side that any mapping must dispatch
through: `NodeRegistry::seeded()` registers exactly
`uniform` and `passthrough` (`eval.rs:206-210`) plus
`image_asset` (`eval.rs:209` — three seeds read live);
`full_registry()` composes seed + generator + filter/color +
mask/spatial + anchor + synth + material nodes
(`lib.rs:44-53`), with the dispatch-everything test at
`lib.rs:115-137` (`uniform`, `passthrough`,
`noise_perlin`, `mix`, `flood_fill`,
`mesh_map_generator`, `anchor`, `synth`,
`material_output`). Unknown `node_def` at eval is
`EvalError::UnknownNode` (`eval.rs:29-35`) — loud, typed,
never silent.

### 2.4. What `to_mtlx_document` writes (the standard-shape document)

2.4.1. One `<surfacematerial>` + one `<open_pbr_surface>`
shader + `<image>` nodes per map, wired with `nodename=`
(`mtlx_doc.rs:250-397`) — the STANDARD MaterialX element
shape (`mtlx_doc.rs:8-10` names the difference explicitly:
"elements named by node category … wired with `nodename=`").
The six wired maps and their inputs are tabled against
vendored nodedef lines in `mtlx_doc.rs:22-29`: base_color
L10, base_metalness L14, specular_roughness L20,
emission_color L76, geometry_opacity L78, geometry_normal
L82 (via stdlib `<normalmap>`, NOT in the vendored file,
`mtlx_doc.rs:31-35`). AO and height have NO OpenPBR input
and are never wired — "never an invented input name"
(`mtlx_doc.rs:31-33`).

2.4.2. Packed scalar channels read through `<extract>`
(`mtlx_doc.rs:303-318`); channel-3 (alpha) reads a `color4`
image (`mtlx_doc.rs:287-291`, test `mtlx_doc.rs:708-716`).
Normals go through `<normalmap>` (`mtlx_doc.rs:319-330`);
DirectX (+Y-down) normals are deliberately NOT wired
(`mtlx-export-design.md:87`). Colorspaces `srgb_texture` /
`lin_rec709` (`mtlx_doc.rs:110-129`) with the caller-supplied
(bytes-actually-written) value winning over the §9 default
(test `mtlx_doc.rs:559-585`). Tiled sets emit `<UDIM>`
(`mtlx_doc.rs:54-58`); lone-1001 sets emit concrete names.
First-texture-per-input wins; AO/height and bad extracts are
skipped (test `mtlx_doc.rs:692-706`); untextured inputs carry
`SurfaceParams` values in nodedef order
(`mtlx_doc.rs:349-384`), with the textured-emission
zero-luminance→1 rule (`mtlx_doc.rs:258-261`, test
`mtlx_doc.rs:685-690`).

2.4.3. Every cited input name/type is verified against the
vendored nodedef by test (`mtlx_doc.rs:519-556`), defaults
match the nodedef's values (test `mtlx_doc.rs:632-658`),
and the connection graph is closed and typed (test
`mtlx_doc.rs:476-517`: 6 images, 2 extracts, 1 normalmap,
15 shader inputs for the full set). This is the most
spec-faithful writer in the tree — and the import's FIRST
consumable target (kill C consumes exactly this shape).

### 2.5. The delta: "what Umber writes" vs "what DCCs write"

2.5.1. Element shape. Umber painter graphs: `<node
name="N{id}" type="…">` with connection inputs
`type="color3" output="N{from}"` (§2.2.2–2.2.3). DCCs:
category-named elements (`<image>`, `<open_pbr_surface>`,
`<normalmap>`, `<extract>`, `<surfacematerial>`,
`<standard_surface>`, `<nodegraph>`) with connections
`<input … nodename="…" [output="port"] />` per the spec's
input-connection rule [MTLX-SPEC §Node Elements]
`(extract-read)`. Umber's OWN standard-shape writer
(§2.4) already proves the tree can emit the DCC shape;
the reader cannot yet read it back — not even ours.

2.5.2. Connection spelling is a hard incompatibility, not a
gap. `from_mtlx` requires every `<input>` to carry exactly
one of `value` / `output` (`mtlx.rs:860-874`) and parses
`output` as `N<digits>` (`mtlx.rs:876-877,691-696`). A
standard-shape connection (`nodename=`, no `value`, no
`output`) hits the `(false, false)` arm → `TypeMismatch`
ERROR. A multi-output disambiguation (`nodename="x"
output="out"`) hits `(false, true)` → `parse_node_id("out")`
→ `BadId` ERROR. Consequence: today's reader errors on
100% of §1-shaped files (Blender export, Designer export,
SP USD Creator output, AND Umber's own `to_mtlx_document`
output — which parses only to an empty graph with zero
warnings, test `mtlx_doc.rs:483-485`, precisely because it
contains no `<node>` elements at all).

2.5.3. Name space. Umber requires `N<digits>`
(`mtlx.rs:691-696`); DCCs use free names (`Sword_shader`,
`coat_roughness_to_power_4` as seen live in the vendored
file at `open_pbr_surface.mtlx:99`). Any standard-shape
import must key nodes by name-string, with `N{id}` accepted
as a legacy subset — never the reverse (renaming foreign
nodes to `N{id}` would destroy re-emit fidelity, cf.
§1.2.7).

2.5.4. Connection typing. Umber emits every connection as
`color3` (`mtlx.rs:344-349`) because "single-output nodes
emit images" (`mtlx.rs:29-30`). DCC connections are typed
per-socket (`float` metalness/roughness end-to-end,
§1.1.4; `vector3` normals; `surfaceshader` terminal feeds).
The import must type edges from the DECLARED socket types
(nodedef inputs + stdlib signatures), not from a constant —
and where no signature is known, the edge type is
`unknown`, carried honestly, never assumed `color3`.

2.5.5. Type space. `ParamValue` covers float / integer /
vector2 / vector3 / color3 / boolean / string(Asset) /
anchor / NodeRef (`lib.rs:79-94`); `from_mtlx` additionally
tolerates unknown value types as `Asset(raw)` (§2.2.6).
MaterialX practice needs, at minimum: `filename`
(distinct from `string` — §1.2.6 lists it as a type Blender
itself lacks), `color4`, `vector4`, `matrix33/matrix44`,
`surfaceshader`/`material`/`lightshader` closures,
`integer` arrays, `stringarray` (with the backslash-escape
convention [MTLX-SPEC §MTLX File Format Definition]
`(extract-read)`), `boolean` vectors, and `uniform`
qualifiers. None of these evaluate in Umber's engine; all
of them must PARSE into the kill-A IR and REPORT (§3.5).

2.5.6. Nodedef backing. Umber's `<nodedef>` blocks are
declarations only (name/node/inputs, `mtlx.rs:308-322`);
foreign nodedefs may carry full nodegraph implementations
(`open_pbr_surface.mtlx:96-100` opens
`<nodegraph name="NG_open_pbr_surface_surfaceshader"
nodedef="ND_open_pbr_surface_surfaceshader">` with
implementation nodes inside), version attributes
(`open_pbr_surface.mtlx:6`: `version="1.1.1"
isdefaultversion="true"`), UI metadata (`uiname`,
`uifolder`, `doc`, `uimin/uimax` throughout
`open_pbr_surface.mtlx:8-89`), and `defaultgeomprop`
bindings (`open_pbr_surface.mtlx:82-88`: `Nworld`,
`Tworld`). The importer keeps identity + interface +
version, evaluates nothing inside, and reports the rest
(§3.4).

### 2.6. The complete foreign-shape rejection inventory (what errors today, line by line)

For the kill-A author, the exhaustive list of `from_mtlx`
behaviors on §1-shaped input, each read live:
(a) `<image>`/`<open_pbr_surface>`/`<surfacematerial>`/
`<standard_surface>`/`<extract>`/`<normalmap>` elements:
SKIPPED silently (the `_ => {}` arm, `mtlx.rs:906`) —
a whole material document yields an empty graph, zero
warnings (test `mtlx_doc.rs:483-485` proves the silence);
(b) `nodename=` connections: `TypeMismatch` error
(`mtlx.rs:869-874`); (c) free node names: `BadId` error
(`mtlx.rs:691-696`); (d) `output="port"` disambiguation:
`BadId` error (§2.5.2); (e) duplicate foreign node names:
accepted (no check — only numeric ids dedupe,
`mtlx.rs:914-919`); (f) `version="1.39"` root: accepted
and IGNORED (no version read anywhere in `from_mtlx`);
(g) `colorspace`/`cms`/`cmsconfig`/`fileprefix`/
`namespace` attributes: ignored (attribute lookup is
by-name per use, `Elem::attr`, `mtlx.rs:419-426`);
(h) XInclude `<xi:include>`: unknown element → skipped
silently (`mtlx.rs:906`) — the include is DROPPED, not
followed, not reported; (i) `<!DOCTYPE …>`: consumed by
the generic bang-skipper (`mtlx.rs:531-557`) — entity
definitions silently discarded; (j) nested `<nodegraph>`
depth N: recursed without limit (`mtlx.rs:905`) —
arbitrary depth is a stack-overflow vector (§4.2);
(k) text content: skipped (`mtlx.rs:636-640`).
The never-crash doctrine (§3.5) requires (a) and (h) to
become REPORTS, (j) to gain a depth cap, and (b)–(d) to
become the standard-shape reader — i.e. kill A.

---

## 3. The consumption strategy (the honest subset)

### 3.1. The doctrine (non-negotiable, from the requirements row)

3.1.1. "No automatic full-fidelity interchange is claimed"
[REQ] (line 65); the node-graph design repeats it: "NO full
MaterialX fidelity claim — the custom nodedefs make the
painter nodes legible to consuming DCCs that implement them;
the standard nodes interoperate for real"
(`docs/specs/node-graph-design.md:74-77`). The import is the
mirror image: foreign standard nodes with Umber analogues
evaluate; everything else is a typed report. "Unsupported
nodes are typed reports, never silent" (this kill's brief)
is the import half of the export's "tolerated, never fatal"
(`mtlx.rs:6-8`) — extended from silent-skip to loud-report
per §2.6(a)/(h).

3.1.2. Three dispositions, no fourth. Every foreign element
lands in exactly one: IMPORT (evaluates through the
registry with byte-pinned tests), PRESERVE (identity +
params carried opaque through round-trip, evaluation
refused typed — the existing unknown-type mechanism,
§2.2.6), or REPORT (named in a machine-readable import
report; never evaluated, never dropped silently). There is
no "approximate" disposition: §1.2.7 forbids dissolving a
foreign node into a lookalike primitive, and §1.1.5
forbids assuming two OpenPBR implementations agree.

### 3.2. The honest subset table (stdlib → Umber analogue, with the evidence grade of each row)

Grades: A = Umber eval exists AND semantics match by test;
B = Umber eval exists, semantic equivalence UNVERIFIED
against the stdlib definition (needs the stdlib-doc kill,
§7 Q2); C = no Umber analogue, PRESERVE-or-REPORT only.
The Umber side is cited to the registry functions read
live; the stdlib side to Blender's documented export
vocabulary (§1.2) and the spec extracts — NOT to a stdlib
document this kill did not read (that gap is §7 Q2, not a
silent assumption).

| Foreign (practice name) | Umber analogue (live cite) | Grade | Disposition |
|---|---|---|---|
| `image` (+ `file`, `default`) | `image_asset` (`eval.rs:209`; full_registry `lib.rs:44-53`) | B | IMPORT as `image_asset` binding; `default` preserved as a param; unresolved-file behavior REPORTED (stdlib defaults to 0.0 §1.4 fnote — Umber's loader errors; the difference is reported per node) |
| `tiledimage` | — (no Umber node; UDIM understood only as a filename token, `mtlx_doc.rs:54-58`) | C | PRESERVE opaque; token kept byte-identical (§1.4.3) |
| `normalmap` | — (Umber consumes normals as data maps; no eval node) | C | kill C wires it structurally (image→shader), never evaluates it |
| `extract` (+ `index`) | — (channel unpacking lives in export packing, not the graph) | C | kill C wires it structurally; generic graphs PRESERVE |
| `checkerboard`/`checker` | `checkerboard` (`nodes/mod.rs:8-16`; `pattern::register_pattern_nodes`, `nodes/pattern.rs:239`) | B | IMPORT; the spelling pair from §2.3.2 recorded, canonical TBD (§7 Q2) |
| `noise` family (`noise2d/3d`, `fractal3d`, `cellnoise`, `turbulence`) | `noise_perlin/noise_value/noise_worley` (`nodes/noise.rs:314`) | B | IMPORT where the variant maps (perlin/value/worley); `fractal`/`turbulence`/`cellnoise` PRESERVE (no analogue evaluated — Umber has no fractal-octave or cellular node) |
| `mix` | `mix` (`nodes/color.rs:338`) | B | IMPORT; Blender flags blend-type application TODO (§1.2.4) so non-trivial mix modes REPORT |
| `invert`, `contrast`, `levels`, `range`, `smoothstep`, `clamp`, `remap` | `invert`, `levels` (`nodes/filter.rs:397`); NO contrast/range/smoothstep/clamp/remap nodes | B/C split | invert+levels IMPORT; the rest PRESERVE (candidate follow-up nodes, NOT silent lookalikes) |
| `blur`, `sharpen` | `blur`, `sharpen` (`nodes/filter.rs:397`) | B | IMPORT (Umber blur is separable-box v1 per `node-graph-design.md:52` — kernel mismatch REPORTED per node) |
| `hsvadjust`/`hsv_adjust` | `hsv_adjust` (`nodes/color.rs:338`) | B | IMPORT; spelling pair recorded (§7 Q2) |
| `color_correct` | `color_correct` (`nodes/color.rs:338`) | B | IMPORT |
| `curves` (RGB/Float) | `curves` (`nodes/filter.rs:397`, 4-point `curve_p0..p3`, `nodes/mod.rs:18-22`) | B | IMPORT where 4-point; Blender RGB-Curves unimplemented upstream (§1.2.4) so foreign curve payloads outside 4-point PRESERVE |
| `place2d`/`rotate2d` | — (no transform nodes in Umber's set) | C | PRESERVE |
| `separate/combine2d/3d/4d`, `convert`, `swizzle` | — | C | PRESERVE (swizzle-adjacent wiring is kill-C structural work, never eval) |
| `triplanar` (+ `triplanar_blend`?) | `triplanar_blend` (`nodes/spatial.rs:330`) | C | PRESERVE — Umber's node is a three-input blender, NOT the stdlib projector; §1.2.7 forbids equating them → grade C, not B |
| `warp` | `direction_warp` (`nodes/spatial.rs:330`) | C | PRESERVE — same reason: Umber's warp is UV-offset-by-vector-input; equivalence UNVERIFIED → C |
| `brick` | `brick_pattern` (`nodes/pattern.rs:239`) | C | PRESERVE — Umber's brick is "Substance-style" (`mtlx.rs:62`); stdlib brick semantics unverified → C |
| `dots`, `constant`/`uniform`, `value` | `dots`, `uniform` (`eval.rs:208`), `passthrough` (`eval.rs:210`) | B | IMPORT |
| `gradient` (`linear/radial/angular` stdlib children?) | `gradient` (`nodes/gradient.rs:177`) | B | IMPORT with the structural caveat: if stdlib gradients are three nodes and Umber's is one type-param node, the mapping is a REWRITE (recorded, tested) not an identity |
| `flood_fill`, `edge_detect`, `histogram_match`, `mesh_map_generator`, `anchor`, `material_output`, `synth` | identical (customs, §2.3.1) | A | IMPORT — the round-trip the tree already pins (§2.2.5) |
| `geompropvalue*`, `geomcolor`, `UsdPrimvarReader_*`, `texcoord` | — | C | REPORT always (§4.3.4): coordinate bindings are preserved data, never evaluated |
| `open_pbr_surface`, `standard_surface`, `UsdPreviewSurface`, `surfacematerial` | `SurfaceParams` Shape, NOT graph nodes (kill C) | C | structural import only (§3.3) |
| `constant`→uniform-value inputs, `dot` no-ops | — | B | `dot` chains collapsed structurally (spec's uniform-connection rule names them [MTLX-SPEC §Node Elements] `(extract-read)`); constants fold to values |
| everything else (script, volumetrics, lights, particles, hair, closures) | — | C | REPORT per §3.5; volumes/displacement explicitly out per Blender's own surface-only precedent (§1.2.4) |

3.2.1. Why so much C: the B grade means "evaluates but
unverified," and promoting any B to A requires the stdlib-doc
kill (§7 Q2) to pin per-node math against Umber's eval tests.
Until then every B import emits a per-node REPORT line
("imported as X — semantic equivalence unverified") alongside
the evaluation. That line is the difference between honest
interchange and Blender's feared silent approximation
(§1.2.7).

3.2.2. The `KNOWN_STANDARD_NODES` cleanup is kill-B
prerequisite work, not optional tidiness: dedupe the nine
double entries (§2.3.2), split the list into TRUE-stdlib
(verified §7 Q2) vs Umber-custom, and record the canonical
spelling of each pair (`checker`, `hsvadjust`,
`noise`/`perlin`/`worley`, `mesh_map`). Until the split,
the list stays a warning suppressor and no new name may be
added to it.

### 3.3. Material terminals: structural import, never evaluation

3.3.1. `surfacematerial` + `open_pbr_surface` in the exact
shape `to_mtlx_document` writes (§2.4) imports STRUCTURALLY
in kill C: images → texture refs (file + colorspace +
UDIM-token preserved), extracts → channel selections,
normalmap → normal binding, untextured values →
`SurfaceParams`-shaped data, AO/height absence → absence
(the "never an invented input name" rule, §2.4.1, applies
to the importer with equal force).

3.3.2. `standard_surface` terminals import to the SAME
structure with a per-material REPORT line (model
substitution, not evaluation). `UsdPreviewSurface`
likewise. No kill evaluates a BSDF; the viewport's OpenPBR
subset (`umber-gpu/src/material.rs` per
`mtlx-export-design.md:16-18`) is the only evaluation
target, fed by explicit mapping, never by executing
foreign shader semantics.

3.3.3. Untextured-value coverage is bounded by
`SurfaceParams`' fourteen fields (`mtlx_doc.rs:151-166`).
Foreign inputs outside the fourteen (anisotropy rotation
as tangent-vector per §1.2.2, subsurface/fuzz/thin-film
per the vendored nodedef lines 12-73, transmission,
sheen) PRESERVE as named data on the imported material
record and REPORT — they must survive a re-export of the
document even though no Umber surface consumes them
today.

### 3.4. The custom-nodedef contract (both directions)

3.4.1. Outbound (ships today): painter customs emit declared
nodedefs first (§2.2.1); unknown foreign types parse with
warnings (§2.2.6). Inbound (kill B): a foreign `<nodedef>`
with a nodegraph implementation imports as a PRESERVED
custom (identity + interface + version + implementation
carried opaque); a foreign `<nodedef>` that is
declaration-only imports the same way minus the body.
Neither ever evaluates. This is the GSoC lesson (§1.2.7)
as a contract clause: identity-preserving opacity beats
lossy translation.

3.4.2. Umber's customs inside foreign documents are the
round-trip fidelity target: `flood_fill`, `edge_detect`,
`histogram_match`, `brick_pattern`, `direction_warp`,
`mesh_map_generator`, `anchor`, `material_output`,
`synth` (§2.3.1) must survive export→foreign-sojourn→import
with params byte-stable. `anchor`'s `{id}:{channel}`
spelling is Umber-private (`mtlx.rs:390-398`); a foreign
tool will carry it as an opaque `anchor`-typed value at
best and a string at worst — the importer accepts both
spellings back (typed value OR string fallback, cf.
§2.2.6) and REPORTS the normalization.

3.4.3. Nodedef versioning: the `version` /
`isdefaultversion` attributes (`open_pbr_surface.mtlx:6`)
are preserved and reported; node elements requesting "a
specific version of that node's definition … instead of
the default" [MTLX-SPEC §Node Elements] `(extract-read)`
import the request as data. No kill implements version
resolution semantics.

### 3.5. The never-crash doctrine (import half)

3.5.1. Every foreign construct lands in IMPORT / PRESERVE /
REPORT (§3.1.2); "skip silently" is removed as a
disposition — §2.6(a) (whole-document silent empty) and
§2.6(h) (XInclude dropped) are the named violations to
fix in kill A. The existing error enum (`MtlxError`,
`mtlx.rs:242-264`) grows REPORT-carrying capacity; the
existing warnings vec (`mtlx.rs:785-791`) is the seed of
the machine-readable import report, not its final form.

3.5.2. The import report is a FIRST-CLASS artifact, not a
log: per-element entries (element path, disposition,
reason, spec/interface cite), per-document summary
(imported/preserved/reported counts), and stability
guarantees (deterministic ordering — by document order,
like the writer's id/edge ordering §2.2.3; re-import of
an unchanged file yields a byte-identical report).
Falsifiability: every kill-A/B/C test asserts exact
report contents for its fixtures (the house rule —
"tests must be able to fail" — with exact values, per
the agent contract).

3.5.3. Resource bounds are part of the doctrine: a nesting
cap (§4.2), an input-count cap, and a document-size cap,
each a typed error naming the limit. The current reader
has none of the three (unbounded recursion §2.6(j),
unbounded allocations per input loop `mtlx.rs:852-897`).

---

## 4. The failure surface (each a real import hazard)

### 4.1. Versioned MaterialX formats

4.1.1. The root `version` attribute is REQUIRED and
"specified as a major and minor number separated by a
dot," with the library auto-upgrading "older-versioned
documents to the current MaterialX version at load time"
[MTLX-SPEC §MTLX File Format Definition] `(extract-read)`.
Umber writes 1.38 (§2.1.2); the vendored nodedef is 1.39;
Blender exports OpenPBR "supported in USD via MaterialX
1.39.3" (§1.2.2); the research bar cites "MaterialX
v1.39.5 (May 2026)" [04 §10]. The importer will meet
1.36-era files (the community plugin's vintage, §1.1.3)
through 1.39.x in one corpus.

4.1.2. Today's reader ignores the version entirely (§2.6(f)).
The hazard is semantic drift across versions, not syntax:
`open_pbr_surface` "ships in the 1.39 stdlib"
(`mtlx_doc.rs:48-50`); a 1.36 document cannot name it; a
1.39 document may use nodes Umber's table predates. Kill A
records the document version in the IR and the report;
kill B conditions the mapping table on it (unknown
version → whole-document REPORT + refuse-to-evaluate,
never guess). No kill implements auto-upgrade semantics —
that is the C++ library's job, and reimplementing it is
explicitly out (§5, "never").

4.1.3. Minor-version tolerance is parse-level only: unknown
attributes are ignored per-element (the `Elem::attr`
design, `mtlx.rs:419-426`), which is the correct forward
compat for additive metadata (`uimin/uimax/uiname/doc`
as seen live throughout `open_pbr_surface.mtlx:8-89`).
Destructive changes (renamed nodes, retyped inputs) are
REPORT + PRESERVE, never silent coercion.

### 4.2. Nodegraph nesting depth

4.2.1. The spec's graph model nests: nodegraphs group nodes
"into a single functional unit" and back custom nodedefs
[MTLX-SPEC §Node Graph] `(extract-read)`; the vendored
file nests implementation nodes inside
`NG_open_pbr_surface_surfaceshader`
(`open_pbr_surface.mtlx:96-100`); Blender inlines group
contents as bare nodegraphs for USD (§1.3.2) but CAN emit
`GroupNodeParser` structures for Hydra (§1.2.3). Depth in
the wild is therefore 0 (flat Blender lists), 1 (Umber's
own descent, `mtlx.rs:905`), or unbounded (hand-built
libraries, nested custom graphs).

4.2.2. `walk_node_list` recurses per `<nodegraph>` with no
counter (`mtlx.rs:799-910`, recursion at line 905).
A hostile or corrupt file with 10⁵ nested nodegraphs
overflows the stack — the same class as the truncation
test (`mtlx.rs:1050-1070`) but for depth. Kill A sets a
documented cap (proposed: 64, stated in module docs with
the rationale that no §1 practice exceeds 2 and the
vendored implementation nests 1) and returns a typed
`DepthExceeded` error naming the depth and the cap.
Recursion becomes an explicit stack or a counted
recursion; the choice is recorded in module docs.

4.2.3. Nesting also carries SCOPING semantics the flat
reader destroys: `[interface token]` filenames resolve
against "the containing nodegraph's interface"
[MTLX-SPEC §Filename Substitutions] `(extract-read)`,
`interfacename` bindings connect instance inputs to
nodedef interfaces, and hierarchical `colorspace` /
`namespace` attributes distribute to included children
[MTLX-SPEC §MTLX File Format Definition] `(extract-read)`.
Kill A preserves scope (each node records its ancestor
chain); kill B resolves interface tokens structurally
(substitution, never evaluation); unresolved tokens
REPORT with the scope path.

### 4.3. Geometric bindings and the file sidecar problem

4.3.1. Filename tokens must round-trip, not resolve.
`<UDIM>` / `<UVTILE>` / `[interface]` / `{frame}` /
`{0Nframe}` / `{hostattr}` ([MTLX-SPEC §Filename
Substitutions] `(extract-read)`) are resolution-time
constructs; the retain-on-export rule (§1.4.3) binds the
importer too. The IR carries file strings verbatim
(escaped form documented — cf. the `<UDIM>` escaping
test, `mtlx_doc.rs:590-606`); any normalization (e.g.
resolving `{frame}` to a number) is forbidden at import.
A file string containing NO token is still not a path to
open: the importer never touches the filesystem in kills
A/B (no existence checks, no reads — sidecar bytes are
kill-C-and-later explicit user action, §4.3.3).

4.3.2. Relative URIs and `fileprefix` are scope, not
strings. "As a shorthand convenience, MaterialX [uses] a
`fileprefix` [that] will be prepended to input values of
type 'filename'" [MTLX-SPEC §File Prefix — section name
approximate, `(extract-read)`]. An `image` node's `file`
is therefore uninterpretable without its ancestor chain
(§4.2.3). Kill A records the effective prefix per node
without joining it (joining is a filesystem claim);
joining happens only when the user explicitly binds
textures in kill C, and the join is reported.

4.3.3. The sidecar problem: a DCC document references
images; Umber's engine consumes content-hash assets
(`image_asset` "loads via the content-hash store" per
`docs/specs/node-graph-design.md:56`, and `Asset(String)`
is a "content-hash path," `lib.rs:88-89`). Importing an
`image` node therefore requires an explicit INGEST step
(file → hash → store) that kills A/B never perform
automatically: auto-reading attacker-controlled paths
from a document is a sandbox violation, and silently
dropping the binding is data loss. The doctrine: kills
A/B bind `image` nodes to UNRESOLVED file refs (reported);
a later ingest kill (NOT in §5 — named here so it is not
forgotten, §7 Q5) resolves them through the same
content-hash path with per-file report lines. Missing
files at ingest follow the stdlib `default` semantics
("the default color value will be 0.0 in all channels"
if `default` is undefined [MTLX-STDLIB §image]
`(extract-read)`) — but that 0.0 must be REPORTED as a
substitution, never mistaken for authored black.

4.3.4. Coordinate bindings are data, never names to fix.
`texcoord` / `geompropvalue*` / `geomcolor` /
`UsdPrimvarReader_*` nodes, `defaultgeomprop` attributes
(`open_pbr_surface.mtlx:82-88`), and the Blender `st`
renaming saga (§1.2.5) jointly forbid normalizing UV-set
names. The IR preserves the binding triple
(node-type, referenced-name, consuming-input) verbatim;
kill C binds Umber's mesh UVs to AT MOST one triple per
material and REPORTS which triple won and which were
dropped. Multi-UV-set materials (two meshes, two names,
one material — the §1.2.5 hard case) REPORT as
unsupported-with-data-preserved, never duplicate the
material silently.

### 4.4. Color management tags

4.4.1. Three tag sites, three hazards. (a) Document working
space: root `colorspace` ("strongly recommended" for
color-managed workflows [MTLX-SPEC §Color Spaces]
`(extract-read)`); Umber documents never declare one
(neither writer emits it — verified by reading both
emission paths, `mtlx.rs:304-307` and
`mtlx_doc.rs:278-280`). (b) Per-input spaces on
filenames/values (`srgb_texture`/`lin_rec709` in our
writer, `mtlx_doc.rs:110-129`; `srgb_rec709_scene` /
`lin_rec709_scene` in the spec's example [MTLX-SPEC
§Color Spaces] `(extract-read)` — NOTE the names
differ: scene-suffixed vs bare; CMS-relativity in one
glance). (c) CMS declarations: `cms` + `cmsconfig`
(OCIO + ACES config linkage [MTLX-SPEC-138 §Color
Spaces] `(extract-read)`).

4.4.2. The mapping rule kill C implements, and kills A/B
prepare: Umber's per-channel color-managed flags
("baseColor yes; roughness/metallic/normal = data" —
`mtlx_doc.rs:98-108`, the §9 rule) are the ONLY
authority for how an imported texture is treated.
`srgb_texture`-tagged color inputs → color-managed;
`lin_rec709`/`none`/absent on data inputs → data; any
tag outside the recognized set → REPORT
(unrecognized-colorspace, value preserved). `cms` /
`cmsconfig` values are preserved as document metadata
and REPORTED (Umber's OCIO adoption — the bundled OCIO
2.5 build per [REQ] §9 — may one day resolve them;
no import kill performs CMS-driven transforms).

4.4.3. The silent-corruption case the rule prevents:
treating an `srgb_texture` roughness (legal in a loose
foreign file) as data without a report, or linearizing
an already-linear emissive. Both are exact-value test
cases for kill C (tagged-roughness imports as data +
report line; tagged-basecolor imports as managed, no
line).

### 4.5. The remaining parse-level hazards (kill-A checklist, each with its disposition)

4.5.1. XInclude (`<xi:include>`): the spec SUPPORTS
includes, each a valid MTLX file whose children merge
into the root with hierarchical-attribute distribution
and global-setting agreement checks [MTLX-SPEC §MTLX
File Format Definition] `(extract-read)`. Today: dropped
silently (§2.6(h)). Kill A: REPORT the include
(href + agreement-check outcome) and do NOT follow it
(network/filesystem fetch is out — the ingest kill's
explicit action, §4.3.3). Following includes is a named
non-goal for kills A–C.

4.5.2. Namespaces (`namespace` attribute): same section,
same treatment — preserved on the IR node, reported
where a collision would occur on flattening, never
stripped silently.

4.5.3. Uniform-only violations: "String- and
filename-type inputs are required to be 'uniform'"
[MTLX-SPEC §Node Elements] `(extract-read)`; a foreign
file wiring a node output into a `filename` input is
either corrupt or version-skewed. Kill B: REPORT +
treat as unbound (with the stdlib `default`/0.0 rule
noted in §4.3.3), never evaluate the connection.

4.5.4. Multi-output disambiguation: the `output`
attribute on inputs "allowing the input to connect to a
specific, named output" ([MTLX-SPEC §Node Elements]
`(extract-read)`) collides head-on with Umber's
`output="N{id}"` edge spelling (§2.5.2). Kill A parses
BOTH spellings into the IR distinguished by form
(`N<digits>` legacy-edge vs free-name port-select);
a value that is neither (e.g. `output="2"`) is a typed
error. The writer keeps emitting the legacy form for
painter graphs; standard-shape emission (if ever) is a
separate kill, not a side effect.

4.5.5. Stringarray escapes: "a comma or semicolon
preceded by a backslash is interpreted as a regular
comma or semicolon rather than as a separator"
[MTLX-SPEC §MTLX File Format Definition]
`(extract-read)`. Umber's vector parser splits naively
on commas (`mtlx.rs:699-715`). Kill A implements the
unescape for `stringarray`-typed inputs (new IR type,
§2.5.5) and LEAVES the float-vector path byte-identical
(its exact-value tests forbid behavior change).

4.5.6. `node` version requests and `default`/`uifolder`/
`doc` metadata: preserved as node metadata in the IR,
reported in verbose mode, ignored in summary mode.
`member` selection on custom-type outputs: REPORT
(Umber's value space has no custom types, `lib.rs:79-94`).

---

## 5. The staged kills (what lands, in order, and what never does)

### 5.1. Kill A — parse-only + report (first, no eval, no registry)

5.1.1. Build a SECOND reader (new module, e.g.
`mtlx_import.rs` — `mtlx.rs`'s exact-value round-trip
tests forbid reshaping its behavior; §2.6 is the
evidence) that parses the standard element shape into a
lossless IR: named nodes (free-name keys, §2.5.3),
typed edges from declared socket types (§2.5.4),
nodedefs with identity+interface+version+body (§2.5.6),
nodegraph scope chains (§4.2.3), verbatim file strings
(§4.3.1), preserved metadata (§4.5.6), and the
machine-readable import report (§3.5.2). It REUSES the
tokenizer shape (`Cursor`/`Elem`, `mtlx.rs:428-689`)
or extracts it — the duplication-vs-refactor call is
the author's, recorded in module docs.

5.1.2. Kill A consumes: flat Blender-style node lists
(§1.2.1–1.2.2), Umber's own `to_mtlx_document` output
(§2.4 — the self-hosting proof: OUR standard shape must
be the first fixture), SP USD Creator-shaped wirings
(§1.1.4), nested nodegraphs to the depth cap (§4.2.2),
XInclude/namespace/version/colorspace/cms constructs
as reports (§4.1–4.5). It REFUSES: evaluation of any
kind, filesystem access of any kind, version-upgrade
semantics (§4.1.2).

5.1.3. Tests (exact-value, per the house rule): one
pinned fixture per §1 shape (hand-authored FROM the
documented behaviors cited — each fixture header names
its source claims, e.g. "Blender closed vocabulary per
§1.2.1; OpenPBR terminal per §1.2.2; flat per §1.2.3"),
byte-identical re-emit of the IR for lossless shapes,
exact report text for lossy ones, the depth-cap error,
the `output=`-both-spellings cases (§4.5.4), the
`<UDIM>` verbatim case (§4.3.1), and the
empty-graph-today case (§2.6(a)) inverted: the §2.4
document must yield a NON-empty IR with zero REPORT
lines above info level.

### 5.2. Kill B — graph import where mappable (second, registry-gated)

5.2.1. Map the IR to `Graph` where §3.2 says IMPORT
(grades A + B-with-report-line), PRESERVE opaque
 customs and C-grades through the existing
unknown-type mechanism (§2.2.6, extended to free names),
REPORT the rest (§3.5.2). Precondition: the
`KNOWN_STANDARD_NODES` split (§3.2.2) and the stdlib-doc
verification kill (§7 Q2) for every row kill B claims
above grade C.

5.2.2. Eval gating: imported graphs evaluate ONLY
through `full_registry` (`lib.rs:44-53`); any
`EvalError::UnknownNode` (`eval.rs:29-35`) is a
successful loud refusal, and a kill-B test pins it per
PRESERVE-grade node. The `anchor` dual-spelling rule
(§3.4.2) and the `dot`-collapse rule (§3.2 table) are
exact-value tests. No kill-B graph may depend on
unresolved file refs for evaluation — `image` nodes
bind UNRESOLVED (§4.3.3) and evaluation of such graphs
reports per node.

5.2.3. Explicit non-goals of kill B: material terminals
(kill C), USD extraction (§1.3.3), file ingest
(§4.3.3), version resolution (§3.4.3), any new eval
node smuggled in as a "mapping aid" (new nodes are
their own kills with their own math tests — §1.2.7
applies inside the house too).

### 5.3. Kill C — material-document import (third, structural)

5.3.1. Consume `surfacematerial` + `open_pbr_surface` /
`standard_surface` / `UsdPreviewSurface` terminals per
§3.3: texture refs (file + colorspace + token +
extract-index + normalmap-wrapping), fourteen-field
surface values with the textured-wins rule
(`mtlx_doc.rs:349-384` mirrored), AO/height absence,
unmapped-input preservation (§3.3.3), UV-triple binding
(§4.3.4), colorspace mapping (§4.4.2 with its two
exact-value cases). Output is a material record the
viewport's OpenPBR subset can consume — the record
type is kill C's design decision, recorded in module
docs (candidate: reuse `SurfaceParams` +
`MtlxTexture`, `mtlx_doc.rs:60-166,131-144`).

5.3.2. Prerequisite: the USD-embedded extraction path
(§1.3.3) lands HERE or as its own kill, not earlier —
kill C works on standalone documents first, USD layers
second, because the §1.3.2 flat-vs-instanced
distinction only matters once the standalone path is
pinned.

### 5.4. What never lands (the boundary, stated so no kill re-litigates it)

5.4.1. Full MaterialX evaluation is NOT the goal
(brief's words; requirements' "no automatic
full-fidelity interchange" [REQ] line 65): no BSDF
execution, no shadergen/codegen, no closure or light-
shader semantics, no OSL/MDL, no volumetric or
displacement evaluation (Blender's own surface-only
precedent, §1.2.4). The goal is round-trip fidelity
for the subset (§3.2 grades A/B + structural kill C).

5.4.2. No version auto-upgrade semantics (§4.1.2), no
XInclude following (§4.5.1), no CMS-driven color
transforms at import (§4.4.2), no automatic file ingest
(§4.3.3), no silent approximation anywhere (§3.1.2).
Each has a named reason above; re-proposing any of them
requires new evidence cited against the section that
closed it.

### 5.5. The staged-kill table

| Kill | Lands | Consumes (§1 shape) | Preconditions | Proves (exact-value) | Never touches |
|---|---|---|---|---|---|
| A — parse + report | lossless IR + report; new module beside `mtlx.rs` | flat node lists; own §2.4 docs; nested graphs to cap; include/ns/version/color constructs as reports | none (first) | byte-identical IR re-emit; exact report text per fixture; depth-cap error; `output=` both spellings; `<UDIM>` verbatim; §2.4 doc non-empty + clean | eval, registry, filesystem, version-upgrade |
| S — stdlib verification (research + pins) | per-node math table stdlib-vs-Umber; promotes §3.2 B rows to A or demotes to C | the stdlib definition document (§7 Q2) + Umber eval tests | kill A (needs IR fixtures to cite) | per-node equivalence or named difference (kernel, hash, colorimetry) | any import code |
| B — graph import | IR→Graph mapping; PRESERVE/REPORT paths; `KNOWN_STANDARD_NODES` split | grades A/B rows; foreign nodedefs opaque; `anchor` dual spelling | kills A + S | per-node import + report-line tests; `UnknownNode` refusal pins; unresolved-image eval report | terminals, USD, ingest, new eval nodes |
| C — material import | terminal→material-record structural import; colorspace + UV-triple rules | `open_pbr_surface` / `standard_surface` / preview terminals; §2.4 shape first | kills A + B (wiring + mapping exist) | tagged-roughness/tagged-basecolor cases (§4.4.3); triple-won/dropped lines (§4.3.4); 14-field + preserve-rest round-trip | BSDF eval, CMS transforms, auto-ingest |
| U — USD extraction (with C or after) | NodeGraph/Shader-prim lift into the kill-A IR | USD-embedded networks, flat + inlined-group (§1.3.2) | kill A; usda/usdc mesh import as-is | prim→IR identity preservation; flat-vs-instanced distinction in reports | mesh import changes (drive-by forbidden, §1.3.3) |
| NEVER | full eval; auto-upgrade; include-following; CMS transforms; silent approximation | — | — | — | §5.4 |

---

## 6. Sources (every claim resolves here)

### 6.1. In-tree sources (all read live 2026-10-14; line numbers are the tree as read)

- [UMBER-MTLX] `crates/umber-graph/src/mtlx.rs` (1–1220, entire
  file): module doctrine L1–68; `NodedefDecl` L72–80;
  `painter_nodedefs()` L83–156; `KNOWN_STANDARD_NODES`
  L163–240 (duplicates + customs noted §2.3.2);
  `MtlxError` L242–264; `esc`/`fmt_float`/`fmt_vec`
  L266–297; `to_mtlx` L304–406 (connections L331–349,
  params L350–401); tokenizer `Cursor`/`Elem`/`parse_doc`
  L412–689; `parse_node_id` L691–696; `parse_floats`
  L698–715; `value_from_typed` L717–783 (anchor L754–778,
  unknown→Asset L779–781); `from_mtlx` L792–934
  (nodedef L808–834, node L835–904, nodegraph descent
  L905, skip arm L906, dup/dangle checks L914–930);
  tests L936–1220 as cited.
- [UMBER-MTLXDOC] `crates/umber-graph/src/mtlx_doc.rs`
  (1–732, entire file): shape statement L8–15; input
  table L22–35; honest scope + 1.38/1.39 note L46–50;
  `UDIM_TOKEN` L54–58; `TextureMap` L60–108;
  `MtlxColorSpace` L110–129; `MtlxTexture` L131–144;
  `SurfaceParams` L146–234 (defaults L168–187, GPU
  unpack L202–233); `element_name` L239–248;
  `to_mtlx_document` L262–397 (images L286–302,
  extracts L303–318, normalmap L319–330, shader
  L337–384, material L386–394); tests L404–732 as cited.
- [UMBER-NODES] `crates/umber-graph/src/nodes/mod.rs`
  (1–129): family→node_def census L8–16; register fns
  L57–78. [UMBER-LIB] `crates/umber-graph/src/lib.rs`
  (1–171): `Node`/`ParamValue` L57–94;
  `full_registry()` L44–53; dispatch test L115–137.
  [UMBER-EVAL] `crates/umber-graph/src/eval.rs`
  L29–35 (`UnknownNode`), L206–210 (seeded registry:
  `uniform`, `image_asset`, `passthrough`), register
  sites for anchor (`anchor.rs:137–138`), material
  (`material.rs:77–78`), spatial (`nodes/spatial.rs:330`),
  synth (`nodes/synth.rs:776`), noise
  (`nodes/noise.rs:314`), filter/color
  (`nodes/filter.rs:397`, `nodes/color.rs:338`).
- [REQ] `docs/specs/requirements.md`: §4 engine row
  L65 (MaterialX serialization contract), §4 node-set
  row L66, §6 MaterialX-export row L102, §6 USD row
  L103, §7 USD-import rows L111–112. Wave/priority
  frame L3–7.
- [NODE-DESIGN] `docs/specs/node-graph-design.md`
  (1–118): eval engine L10–41, node set L43–63 (MTLX
  naming rule L58–61), serialization L65–77 (no-fidelity
  clause L74–77), `image_asset` content-hash line L56.
- [MTLX-EXPORT] `docs/specs/mtlx-export-design.md`
  (1–104): ground-truth + token engine + GPU material
  L6–18, document design L20–48, tests L50–55,
  as-built notes L64–96 (inputs L73–76, UDIM L77–83
  with the UNVERIFIED recall flag, colorspace L84–87,
  reader boundary L88–90).
- [OPENPBR-V] `docs/claw-artifacts/openpbr/open_pbr_surface.mtlx`:
  root version L2, nodedef open L6
  (`ND_open_pbr_surface_surfaceshader`,
  `version="1.1.1" isdefaultversion="true"`), input
  lines L8–89 as cited (base_color L10, base_metalness
  L14, specular_roughness L20, emission L74–77,
  geometry_opacity L78, geometry_normal L82–88 with
  `defaultgeomprop`), implementation nodegraph open
  L96, sample impl node L99–100.
- [01] `docs/research/01-painter-feature-inventory.md`:
  §7 Materials-import line L163 ("MaterialX: not in
  Painter (Designer only)"); channel vocabulary +
  color-managed flags L72–75; OpenPBR-default L135/L226.
- [04] `docs/research/04-nextgen-capabilities.md`: §10
  MaterialX-export row L40–41 (spec URL, v1.39.5,
  `materialx`-crate-placeholder, UDIM-token tractability);
  §11 UsdMtlx row L47–48; §14 graph-engine row L56–57
  (MaterialX-documents serialization, WGSL-gen PR
  #2996, 40-nodes + extension API).
- [WAVEPLAN] `docs/loop/wave-plan.md` (1–63): waves
  1–3 only — the absence of a wave-5 row is itself a
  cited fact (§scope box).
- [OPENUSD-V] `docs/claw-artifacts/openusd/README.md`:
  usda/usdc/usdz coverage L13–19.
- [DYNMAT] `docs/specs/dynamic-materials-design.md`:
  `N{id}` naming + string→Asset cites L103–108 (as
  found live via grep; full-doc read NOT done — the
  two cited lines only).

### 6.2. External sources (read 2026-10-14; `(extract-read)` = search-result extract, full text NOT verified)

- [MTLX-SPEC] `https://github.com/AcademySoftwareFoundation/MaterialX/blob/main/documents/Specification/MaterialX.Specification.md`
  `(extract-read)`: file-form/XInclude/version/colorspace/
  substitutions/inputs sections as quoted in §§1.4, 4.
- [MTLX-SPEC-138] `https://materialx.org/assets/MaterialX.v1.38.Spec.pdf`
  `(extract-read)`: color-spaces/CMS/ACES-config text as
  quoted in §§1.4.3, 4.4.
- [MTLX-STDLIB] `https://github.com/AcademySoftwareFoundation/MaterialX/blob/main/documents/Specification/MaterialX.StandardNodes.md`
  `(extract-read)`: image `file`/`default` semantics as
  quoted in §4.3.3.
- [OPENPBR-FAQ] `https://experienceleague.adobe.com/en/docs/substance-3d/general-knowledge/openpbr/openpbr-faq`
  `(extract-read)`: portability/validation/USD-path/library-
  workflows text as quoted in §§1.1.5, 1.3.1.
- [BL-111765] `https://projects.blender.org/blender/blender/pulls/111765`
  `(extract-read)`: `bl_use_materialx`, first node set,
  group-nodes + `USE_MATERIALX_NODEGRAPH`/UsdMtlxRead
  note as quoted in §§1.2.1, 1.2.3.
- [BL-112864] `https://projects.blender.org/blender/blender/issues/112864`
  `(extract-read)`: per-node support matrix as quoted in
  §§1.2.1, 1.2.4, 3.2.
- [BL-138164] `https://projects.blender.org/blender/blender/issues/138164`
  (+ fix PR #138165, same page) `(extract-read)`: Standard
  Surface→OpenPBR switch, MaterialX 1.39.3, as quoted in
  §1.2.2.
- [BL-122800] `https://projects.blender.org/blender/blender/issues/122800`
  `(extract-read)`: UV-primvar/`st`/UsdPrimvarReader
  discussion as quoted in §§1.2.5, 4.3.4.
- [BL-158138] `https://projects.blender.org/blender/blender/issues/158138`
  `(extract-read)`: in-memory-document import layering,
  missing socket types, Storm render tests, as quoted in
  §§1.2.6, 3.2.
- [BL-GSOC26] `https://devtalk.blender.org/t/gsoc-2026-materialx-shader-authoring-in-blender/45211`
  `(extract-read)`: `io_data_mtlx` flatten critique,
  convert-node symmetry note, USD nodedef-pair note,
  OSL prototype, as quoted in §§1.2.6–1.2.7, 1.3.2.
- [DES-101] `https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/release-notes/old-versions/version-2020-1-10-1`
  `(extract-read)`: MaterialX Plugin (Beta) + graph-
  category notes as quoted in §1.1.2.
- [MATXUTILS] `https://github.com/AllegorithmicSAS/materialx_utils`
  (README, `(extract-read)`): arnold + Metallic/Roughness
  scope as quoted in §1.1.3.
- [SPUSD-DEV] `https://ahmed-hindy.github.io/Substance-Painter-Usd-Creator/developer_guide.html`
  `(extract-read)`: payload shape, float end-to-end rule,
  OpenPBR-skip-standard rule as quoted in §1.1.4.
- [SPUSD-USER] `https://ahmed-hindy.github.io/Substance-Painter-Usd-Creator/user_guide.html`
  `(extract-read)`: four-network publish + layer layout
  as quoted in §§1.1.4, 1.3.

### 6.3. Deliberately NOT sources (the fabrication firewall)

The full MaterialX specification PDF, the full standard-nodes
/ pbr-nodes definition documents, any MaterialX C++ sources,
any Blender C++ exporter sources beyond the extracts above,
any `.mtlx` corpus file (none was provided — cf. the
spp-layout kill's §8 corpus-oracle pattern), and any Adobe
binary or SDK. Claims requiring those texts are §7
questions, not §1–5 claims.

---

## 7. Open questions (named as open, each with its resolving kill)

Q1. Real-corpus validation. Every §1 practice claim about
Blender/Designer/plugin output rests on DOCUMENTED behavior
(issues, PRs, READMEs), not on files. The first real `.mtlx`
corpus (exported from Blender ≥4.x with the MaterialX
network option, a Designer MatX export, an SP USD Creator
publish) decides: the true element census, the nesting
depths in practice, the filename-token distribution, the
colorspace-name distribution. Corpus rule (adopted from the
spp-layout kill): never committed, sha256-pinned, per-file
oracle notes. Resolves by: the implementer of kill A, before
freezing the IR.

Q2. Stdlib ground truth. This kill did NOT read the standard-
node definition document (only the two spec-text extracts in
§6.2). Every §3.2 grade-B row, every §2.3.2 spelling pair,
and the TRUE-stdlib half of the `KNOWN_STANDARD_NODES`
split wait on the S kill reading the stdlib definitions and
pinning per-node signatures (input names/types, defaults,
uniformity, version history). Without S, kill B maps nothing
above grade C.

Q3. Version-drift specifics. Which nodes/inputs changed
semantics between 1.36 and 1.39.x (the import's version-
conditioned table, §4.1.2) is unverified — the extracts
confirm auto-upgrade EXISTS, not what any upgrade DOES.
Resolves by: kill S against the versioned stdlib + the
1.38→1.39 delta notes upstream.

Q4. Houdini/Karma/UE shapes. §1 covers Painter-adjacent,
Blender, and USD practice; [04] §14 names Houdini/UE as
interchange targets but this kill gathered no documented
export behavior for either. Their terminals (Karma physical
sky? UE condensed graphs?) may add terminal kinds to §3.3.
Resolves by: a documentation pass before kill C freezes
the terminal list — NOT by guessing from the Blender set.

Q5. The ingest kill's shape. §4.3.3 defers file→hash→store
ingest without designing it (explicitly out of §5's scope):
its UI surface (per-file accept/deny?), its sandbox rule
(project-relative only?), its `default`-substitution UX.
Resolves by: a design note owned with the export-driver
team (`umber-export` resolves filenames today,
`mtlx_doc.rs:5`), before any kill reads a texture off disk
at import.

Q6. The IR's home for scope data. §4.2.3 requires ancestor
chains, effective `fileprefix`, and effective colorspace
per node; whether the kill-A IR stores resolved-effective
values, raw chains, or both (re-emit fidelity argues raw;
query convenience argues effective) is the kill-A author's
call — recorded in module docs per the house rule on
judgment calls.

Q7. `bl_use_materialx`'s CURRENT vocabulary. The first-node
list (§1.2.1) is a 2023 PR; the 2026 vocabulary is larger
("almost all nodes available in Cycles" per the PR thread,
`(extract-read)`). Kill B re-checks #112864's matrix at
build time rather than trusting this doc's snapshot —
this doc's table is the floor, not the ceiling.

