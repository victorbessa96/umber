# K8c — Vector Properties miter-limit row

- Date: 2026-11-21.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-21-umber-vector-panel-miter-limit.md` (`81088f54`).
- Implementation: `803d6a35a8c1987896cf4d006649377e7290c2e7`.
- Kill lock: `razul-vector-panel-miter-limit`.
- Locked surface: `crates/umber-app/src/vector_panel.rs`, both i18n catalogs, the brief, this report, `docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, and `docs/ROADMAP.md`.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8c brief.

## Built

- `vector_panel.rs`: adds a localized Miter-limit `ValueField` directly after the stroke-join combo. The production row is guarded by `shows_miter_limit(stroke.join)`, so it is present for Miter and hidden for Round/Bevel without discarding the stored value.
- The row binds through a temporary value and the production `set_miter_limit` mediator. A changed value writes only `stroke.miter_limit` and rides the existing `LayerCommand::set_vector` path; an unchanged value reports no change and does not mint redundant history.
- The field accepts `1.0..=1000.0`. The upper bound is a deliberate UI cap, not a claim about core: K8b accepts every finite value >= 1.
- `ROW_LABEL_KEYS` and the row-label expectation include `vector.stroke-miter-limit` in draw order.
- `en-US.ftl` / `pt-BR.ftl`: add `Miter limit` / `Limite de miter`.
- Tests pin Miter/Round/Bevel visibility, production guard ordering, no-op behavior, preservation of the complete `Stroke`, one undoable `SetVector`, and undo restoration.

## Verification

- `rustfmt --edition 2021 --check crates/umber-app/src/vector_panel.rs`: exit 0.
- `python3 scripts/check-layers.py`: GREEN, 16 crates, exit 0.
- `python3 scripts/scorecard.py --check`: README current, exit 0.
- `cargo clippy -p umber-app --all-targets -- -D warnings` under `/tmp/umber-cargo.lock`: exit 0.
- `cargo test -p umber-app` under the flock with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: exit 0; 321 lib tests, 891 bin tests, and integration/doc suites totaling 2, 1, 3, 17, 11, 10, 2, 1, and 0 tests all passed with zero failures.
- Independent re-review also ran `cargo test -p umber-core miter_limit_export_roundtrips_and_omits_default`: passed, including Round/Bevel preservation.
- `git diff --check`: clean.
- Ad-hoc freshness probe earlier in the hunt: 10/10 checks, focused vector-panel 19/19, self-deleted with zero scratch strays. This supplements, and does not replace, the canonical full-suite receipt above.
- Independent R1 (`deleg_afeb9db7`): REQUEST_CHANGES, one minor test-binding finding and one brief-wording nit. Fixed by routing the production row through the tested mediator, asserting full stroke preservation/no-op behavior plus the production guard, and describing 1000 as a UI cap.
- Independent R2 (`deleg_d39ed2ad`): APPROVED, zero findings; all requested gates exited 0.

## Judgment calls

- Miter-only visibility was chosen over showing a disabled field for Round/Bevel. The stored value remains intact and still participates in native/SVG roundtrips; hiding is only a panel concern.
- The temporary binding plus mediator makes the exact production edit operation testable and prevents no-op history. A pointer-level kittest was not required by the review once this production mediator and guard were pinned.
- The UI cap of 1000 is intentionally narrower than core storage. No core validation, geometry, serialization, or export behavior changed.
- Implementation and status documents land as separate path-limited commits so the report and queue can cite the verified implementation hash exactly.

## Unverified

- No interactive pointer-driven GUI smoke was run. The production mediator, source-order guard, document history, catalogs, and complete app suite are verified; physical pointer motion over the rendered field is not.
- No new SVG geometry or parser behavior is claimed. The targeted core roundtrip is regression evidence only.

## Deferred

- Secondary-contour editor affordances and further SVG attributes remain separately scoped work.
- Version bumps, tags, releases, public PRs, schema/project-version changes, and new dependencies remain outside this kill.

## Process note

R1 ran while the shared tree was on local `master`; its branch observation was accurate, not a code defect. Both pre-existing unpushed docs commits (`81088f54`, `bc8b8516`) are this session's ledgered K8c brief/claim. The kill lock remains held until the status-doc commit releases it.
