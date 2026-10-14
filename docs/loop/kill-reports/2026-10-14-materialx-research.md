# Kill report — MATERIALX-ROUNDTRIP RESEARCH (2026-10-14)

## 1. Built

- `docs/research/materialx-roundtrip.md` (NEW, 1356 lines, 7
  sections): the import research doc the brief demands. Numbered
  sections answering each of the 6 research questions (§1 DCC
  practice shapes: Painter/Designer/plugins, Blender Hydra path,
  UsdMtlx; §2 Umber's own contract from `mtlx.rs` +
  `mtlx_doc.rs` + the export design doc, all read live entire;
  §3 honest-subset table with A/B/C evidence grades + never-crash
  doctrine; §4 failure surface: versions, nesting depth,
  geometric bindings/sidecars, color tags, parse-level checklist;
  §5 staged kills A/S/B/C/U + NEVER table; §6 every-claim
  sources; §7 seven named-open questions). No code touched, no
  behavior changed.
- Tests added: 0 (research kill — the test plans live in §5.1.3,
  §5.2.2, §5.3.1, §4.4.3 of the doc: exact-value pins + report
  assertions per fixture).

## 2. Judgment calls

- Graded the §3.2 subset B ("evaluates but UNVERIFIED") instead of
  claiming equivalence: no stdlib definition document was read
  this kill, and promoting any row to A without it would be the
  fabrication this repo fears most. The S kill exists to promote
  or demote each row. Recorded in §3.2.1 + Q2.
- Split `KNOWN_STANDARD_NODES` cleanup into kill-B prerequisite
  work (§3.2.2) rather than proposing the corrected table here:
  the canonical spellings need the stdlib doc (Q2), and guessing
  them now repeats the accretion error §2.3.2 documents (nine
  live-read duplicate entries).
- Specified kill A as a SECOND reader beside `mtlx.rs`, not a
  refactor of `from_mtlx`: the existing byte-identical round-trip
  tests forbid reshaping its behavior, and §2.6 inventories every
  foreign-shape behavior line by line so the author knows exactly
  what stays and what moves. Recorded in §5.1.1.
- Marked all web-derived claims `(extract-read)` with quoted
  wording instead of full-document section cites: only search
  extracts were fetched, and the doc says so per claim (§scope
  box, §6.2, §6.3 firewall). The wave-5 row label is the brief's,
  not the tree's — `wave-plan.md` as read live plans waves 1–3
  only, and the doc states that plainly rather than inventing a
  roadmap row.
- Did NOT mark any requirements row: the standing amendment locks
  this kill to the 2 files, and the §4 engine row already carries
  its `✓ built` note. Flagging, not skipping.

## 3. Unverified

- Every `[MTLX-SPEC]`, `[MTLX-SPEC-138]`, `[MTLX-STDLIB]`,
  `[BL-*]`, `[DES-101]`, `[MATXUTILS]`, `[SPUSD-*]`,
  `[OPENPBR-FAQ]` claim is extract-read (search-result text,
  fetched 2026-10-14), NOT verified against the full documents.
  Section names from extracts are flagged approximate where the
  extract did not confirm them (§4.3.2 fileprefix). The
  implementing kills must confirm each against the full text —
  Q1–Q4, Q7 name exactly which confirmations unblock which kill.
- No `.mtlx` corpus file was available by any permitted means;
  all §1 fixtures are specified as hand-authored FROM documented
  behavior with per-fixture source headers (§5.1.3), pending the
  Q1 corpus oracle.
- `docs/specs/dynamic-materials-design.md` was NOT read whole —
  only the two `N{id}`/string→Asset lines found live via grep;
  the cite is scoped to those lines (§6.1 [DYNMAT]).
- Behavior needing runtime runs (none run — NO cargo): nothing in
  this kill is behavioral; the staged kills' measurement/test
  obligations are recorded in §§5.1.3/5.2.2/5.3.1.

## 4. Deferred

- Requirements-row marking — for the dragon at landing (surface
  lock; see §2 last bullet).
- The file-ingest design (Q5), Houdini/UE documentation pass
  (Q4), IR scope-storage decision (Q6) — named OPEN in §7 for
  the implementing kills, not designed here.
- Pre-existing related follow-ups deliberately not absorbed: the
  `mtlx-export-design.md` UNVERIFIED recall flag on
  `<UDIM>`/`srgb_texture` names (now partially corroborated by
  the spec extracts in §§1.4.3–1.4.4 — the design doc itself was
  not edited; surface lock), the nine `KNOWN_STANDARD_NODES`
  duplicates (§2.3.2, kill-B work), the unbounded-recursion
  vector (§2.6(j)/§4.2.2, kill-A work).

## 5. Gates run

- `cargo fmt` / `clippy` / test suites: NOT RUN — forbidden by
  the kill protocol (NO cargo; research kill, markdown-only
  surface).
- `bash scripts/verify-kill.sh` on the 2 locked files: PASS, verbatim
  tail: "VERIFY-KILL: docs/research/materialx-roundtrip.md
  docs/loop/kill-reports/2026-10-14-materialx-research.md —
  balanced, fmt-clean, trap-scanned, 1 unlocked-but-noted file(s)"
  (the noted file is `crates/umber-app/src/retouch.rs` — another
  kill's dragon-owned surface, not this kill's).
- In-tree citation rule: every `path:line` citation was read live
  this kill (`mtlx.rs` full 1220 lines; `mtlx_doc.rs` full 732;
  `mtlx-export-design.md` full 104; `node-graph-design.md` full
  118; `nodes/mod.rs` full 129; `lib.rs` full 171;
  `eval.rs` L1–120 + grep-verified register sites;
  `open_pbr_surface.mtlx` L1–100; `requirements.md` full 179;
  `04-nextgen-capabilities.md` full 99; `01` inventory §§1–13
  excerpts + L163; `wave-plan.md` full 63; openusd README full
  39; research README full 13; spp exemplar full 246).
- No commits (per protocol). Surface respected: only the 2 locked
  files written.
