# Kill report — mtlx-kill-s (stdlib verification, research + pins)

Date: 2026-10-14. Author: dragon. Wave: t_a1d4c79a (Wave 7 card;
the kill is the MaterialX row's named next stage). Lock:
`mtlx-kill-s` over `crates/umber-graph/tests/mtlx_stdlib_pins.rs`,
`docs/research/materialx-stdlib-verification.md`, this report,
`docs/ROADMAP.md` (additive row tick at wave end — the hotspot
comment's rule).

Design-doc sections implemented: the staged-kill table's S row
(`docs/research/materialx-roundtrip.md` §5.5), resolving §7 Q2
(stdlib ground truth) and feeding §3.2.2 (the KNOWN_STANDARD_NODES
split). Parent research doc sections consumed: §3.2 (the subset
table), §2.3.2 (the duplicate-names claim), §7 Q2/Q3.

## Built

- `docs/research/materialx-stdlib-verification.md` — the stdlib
  ground-truth study against the REAL MaterialX 1.39 artifacts
  (stdlib_defs.mtlx sha256 865e73cc…, stdlib_ng.mtlx sha256
  8cc2a4b7…, both fetched live 2026-10-14, never committed — the
  corpus rule). Contains: the census (703 nodedefs / 147
  categories / 208 NG implementations), the KNOWN_STANDARD_NODES
  audit (20 true-stdlib names, 21 Umber customs, 22 dead or
  misspelled entries — `checker`, `separate2d/3d/4d`,
  `combine2d/3d/4d`, `turbulence2d/3d`, `triplanar`, `perlin`,
  `worley`, `value`, `linear`, `radial`, `angular`, `warp`, `brick`,
  `noise`, `mesh_map`, `histogram`, `levels_as_mask` triaged name
  by name), the canonical-spelling pair table (14 rows), and the
  per-node verdicts with every claim cited to the prose or the NG
  graph verbatim.
- `crates/umber-graph/tests/mtlx_stdlib_pins.rs` — ten exact-value
  pins locking the verdicts into the tree: census constants, mix
  formula identity (Fm+B(1-m) byte-exact at three factors),
  invert-as-amount-minus-in, hsv_adjust's hue-unit rewrite
  (×360, byte-exact: +120° ≡ amount.x=1/3), the checkerboard
  phase pin (Umber (0,0)=white vs stdlib color2=black — the
  identity-claim tripwire), color_correct's silent-ignore contract
  (the stdlib CDL params are no-ops at Umber's eval — the import
  REPORT is the only guard), constant→uniform and dot→passthrough
  rename maps, the noise stability pin (deterministic-field only;
  the values are Umber's own, NOT stdlib-matching), and the
  gradient screen-space convention pin.

## Judgment calls

1. **No KNOWN_STANDARD_NODES edit** — the charter bars import code
  and the list edit belongs to the mapping that consumes it (kill
  B). The split is fully recorded here. Alternative rejected:
  edit now, one commit earlier — violates the S-kill charter and
  touches a shared surface mid-wave for zero consumer.
2. **The noise family demoted B→C.** The stdlib's canonical noise
  hash is C++ (unread — fabrication firewall); unverifiable
  equivalence means preserve-or-report, never identity. §3.2's B
  grade was optimism; a B that cannot be promoted is a C.
3. **checkerboard demoted B→C** on the phase inversion + domain
  difference read from NG_checkerboard_color3. The B row assumed
  name identity implied eval identity; the NG says otherwise.
4. **colorcorrect demoted B→C** — the stdlib node is the CDL chain
  (lift/gain/contrast/exposure/gamma + luma-based saturate);
  Umber's is HSV + pivot-contrast + additive brightness. Different
  op families under colliding names: preserve, never identity-map.
5. **hsvadjust promoted to A conditionally** — the hue unit (0..1
  vs degrees) is a recorded rewrite, byte-exact after ×360. The
  pin locks the conversion so kill B cannot drift it.
6. **Duplicate count corrected: 8, not 9.** The parent doc's
  §2.3.2 claimed nine double entries; the live count is eight
  (`invert`, `levels`, `dots`, `sharpen`, `curves`, `uniform`,
  `image_asset`, `triplanar_blend`). Recorded here; the parent doc
  is NOT edited (its §5.5 amendment path owns that).
7. **The noise pin's placeholder-recording flow.** The corner-byte
  constants were NOT invented: the first pin asserts byte-range +
  determinism only; the exact values get pinned from the first
  live battery run (the process note below names the recording).
  A fabricated "recorded" constant would be a lie with a test
  suite attached; recording from the real run is the honest path.

## Unverified

- The stdlib's NATIVE-node semantics rest on prose + nodedef
  defaults, not executed reference math (the C++ is behind the
  fabrication firewall). If the reference implementation diverges
  from its own prose, this kill cannot see it; the research doc's
  §7 ledger carries the caveat.
- 1.36→1.39 drift for the graded nodes is unexamined (the doc pins
  1.39 only) — kill B's version-conditioned table owns that.
- The gradient ramp-family rewrite candidate (linear ↔
  ramplr/ramptb) is recorded as a CANDIDATE, not verified byte-wise
  (screen-space vs texcoord-space makes them non-equivalent; the
  report line is mandatory in any future mapping).

## Deferred

- The KNOWN_STANDARD_NODES edit (kill B, with its consumer).
- The import mappings themselves (kill B) and the material
  terminals (kill C) — this kill only grades the ground they walk.
- The USD extraction kill (U) and the §7 Q5 ingest design.

## Process note

- The wave runs beside the interactive session's umber-paint
  extraction (their commits 989ba7f..ca17305, all UNCOMPILED —
  cargo on hold per their commit messages). The battery scoped to
  umber-graph only; the workspace's Cargo.lock state compiles
  umber-graph against the committed tree. Their extraction touches
  umber-app/umber-paint surfaces — disjoint from this kill's
  files. The battery's `cargo fmt --all` may reformat their
  in-flight files (the documented lock-overlap; harmless,
  formatting only).
- The stdlib artifacts live in /tmp with sha256s recorded in the
  research doc; they are never committed (the corpus rule).
- Authorship split: the research doc, the pins file and this report
  were written by a dragon-loop leisure session (glm-5.3), which exited
  before running Gate 1. The interactive session finished the kill
  (2026-10-14 ~21:50) and ran its battery.

## Finish (interactive session)

Gate 1 r1 (`/tmp/kills-battery-r1.log`): 9/10 pins passed; one RED and
two warnings, all fixed:

- **invert pin was wrong about Umber, not about the stdlib.** It
  expected byte 128 for input 0.5 (float `1 - x`). Umber's invert is
  BYTE-domain: `255 - b` (`nodes/filter.rs` `InvertNode`), and
  `uniform` encodes 0.5 → 128 first, so the output is 127. Re-pinned
  to 127 with the encode-then-invert ordering explained in the test.
  A float-domain rewrite of invert is now a visible change.
- **The noise pin could not fail.** Recorded live at offset (0, 0), all
  four corners were 128: perlin is 0 at every integer lattice point,
  and at 4×4 / scale 8 the corners sit on the lattice. The fixture now
  uses offset (0.37, 0.61), which gave 141 / 123 / 168 / 141
  (corners (0,0), (x0,y3), (3,3), (x3,y0)), pinned exactly — judgment
  call 7's placeholder is retired with real recorded values.
- Clippy (r2): `identity_op` / `erasing_op` on hand-written texel
  offsets (`(0 * 4 + 2) * 4`) → a `texel(x, y, w)` helper; the unused
  `image()` helper and the useless `u8 <= 255` assert removed.

## Gate 1

Battery scoped `umber-graph`: r2 `/tmp/kills-battery-r2.log` — tests
141/141 GREEN, clippy RED (the 8 lint findings above); the clippy
re-run after the fixes is the commit gate.
