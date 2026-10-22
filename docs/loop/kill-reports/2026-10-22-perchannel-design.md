# Kill report — 2026-10-22 perchannel-design (research kill)

Authorship: dragon (Razul, session local-7de7d3ed). Kill-lock:
`perchannel-design` over `docs/research/perchannel-mode-restriction-2026-10-22.md`
+ this report. Design-doc sections implemented: none — a research kill;
the doc STAGES the K1 code kill (blocked on the cargo prohibition).

## Built

- `docs/research/perchannel-mode-restriction-2026-10-22.md` — the design
  fork for per-channel blend-mode restriction: a 11-row current-state
  table (every row read from the tree at its file:line this session),
  the gray-content algebra table proving Tint/Saturation/Color exact
  no-ops and Value ≈Normal on scalar channels, the four-option fork with
  Option 2 (shape-keyed gate) rejected-as-infeasible and documented as
  the trap (MaterialOutput has no type metadata; planes are rgb-native),
  Option 4 recommended (semantic scalar blocklist at the picker:
  the no-op trio off scalar-semantic channels, Value kept everywhere),
  the staged K1 plan with its one-file surface and test extensions,
  non-goals (picker-only, no saved-param rewriting), and three open
  questions.

## Judgment calls

- Option 4 over Option 1 (full Painter tables): the smallest provable
  step beats the large parity table — the blocklist is DERIVED from
  in-tree evidence (the achromatic guard + the gray-content algebra),
  not imported opinion. Rejected alternative: mirroring Painter's
  per-channel lists would need a per-name ruling for every future
  channel and over-restricts beyond what the tree proves.
- Unknown channel names default scalar-semantic (trio blocked):
  masks/weights are the common custom-channel case; a wrong default
  hides three modes and is recoverable by extending the pinned set —
  the reverse default would leave custom data channels offering the
  exact no-op trio this kill removes. Follow-up created: the predicate
  set is pinned by test and extends when a color-named custom channel
  appears.
- Value kept everywhere: ≈Normal on gray is redundant, not
  meaningless; blocking it would be opinionation beyond the evidence.
- Driver left permissive: backward compatibility for saved documents —
  the alternative (defense-in-depth driver block) would change
  saved-file behavior under a menu change; rejected for K1, nameable
  later if a real case appears.

## Unverified

- Painter's exact per-channel lists are NOT cited mode-by-mode (public
  UI behavior summarized in the doc). K1's blocklist does NOT claim
  Painter-list parity — it claims no-op-removal on in-tree evidence.
  If exact per-channel parity is wanted later, that is Option 1, a
  separate brief.
- The gray-content convention for scalar channels (r=g=b) is verified
  from tests + template usage, not enforced by code anywhere; the doc
  names the colored-roughness edge as garbage-in and records the
  export-warning idea as open question 2.

## Deferred

- K2 (the tooltip affordance): needs both .ftl catalogs; only if the
  panel wants it.
- The mtlx-type threading (Option 2's refactor): rejected and kept as
  the documented trap; revisit only if channel shape becomes
  load-bearing elsewhere.

## Process note

- Research lane under the 2026-10-22 envelope (full autonomy except
  cargo build/test): every claim re-derived from tree reads this
  session — the layers.rs enum census + `is_triplet`/`is_normal_map` +
  `rgb_to_hsv` guard docs; the layers_panel.rs picker + its `:1681`
  pin; the material_layer.rs driver gate + `MaterialOutput` + the
  triplet-routing test; document.rs `layer_blend_options`. No battery,
  no CI claims (the M2 standing rule); rides the recovery push.
