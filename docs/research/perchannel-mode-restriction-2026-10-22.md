# Per-channel blend-mode restriction — the design fork (2026-10-22)

Status: research/design kill (docs-only, battery-exempt). The code kill it
stages is BLOCKED behind the fleet closes + a sanctioned battery (the
2026-10-22 envelope forbids cargo build/test). Seed: the 10-21
deep-context design seed (task-progress `umber`) + the STATE block's
open-lanes row.

## The question

`channel_blend_options` offers the HSV quartet (Tint, Saturation, Color,
Value) on every non-normal channel, including the scalar-semantic
channels (roughness, metallic, height, masks). On those channels the
quartet is selectable-and-active but semantically meaningless — near-no-ops
on the channel's conventional gray content, or a redundant ≈Normal
replace. Should the picker restrict the mode list per channel semantics
(Painter's parity answer), and what is the smallest honest gate?

## Current state, verified from the tree (2026-10-22, every row read at its cite)

| surface | fact | cite |
|---|---|---|
| mode census | 32-mode enum, each variant census-documented against Painter's list (Dissolve deliberately excluded as non-separable-safe; Reflect/Glow absent from Painter's list) | `crates/umber-core/src/layers.rs:183`+ |
| the picker | `channel_blend_options(channel)`: 31 modes on `normal`, 28 on every other channel — only Passthrough (never) and the normal-map trio (normal-only) are gated; declaration order pinned by test | `crates/umber-app/src/layers_panel.rs:91`, test `:1681` |
| picker call site | the channel row iterates the options directly | `crates/umber-app/src/layers_panel.rs:397` |
| layer-row picker | `layer_blend_options(kind)` = `channel_blend_options("")` + Passthrough for folders | `crates/umber-app/src/document.rs:1450` |
| the driver gate | plan.blend ∈ {Passthrough} ∪ (normal-map ∧ channel≠normal) → `SkippedBlend` badge, never a Normal fallback | `crates/umber-paint/src/material_layer.rs:997-1011` |
| per-channel control | `MaterialChannel {name, blend, opacity, enabled}` → `ChannelPlan`; channels the definition no longer declares are ignored; undeclared channels default Normal/1.0/enabled | `crates/umber-paint/src/material_layer.rs:975-992` |
| channel identity | `MaterialOutput = {node_id, channel: String}` — NO type/shape metadata; channel names are free strings from the mtlx definition | `crates/umber-paint/src/material_layer.rs:292` |
| triplet routing | the eval routes triplet modes through `blend_rgb` on the plane's rgb bytes (a scalar-primitive routing would fail loud) | test `crates/umber-paint/src/material_layer.rs:1775` |
| triplet set | 7 modes: Tint, Saturation, Color, Value + the normal trio; `is_triplet` membership pinned by census test | `crates/umber-core/src/layers.rs:494` |
| the achromatic guard | Tint/Color: an exactly-achromatic top leaves the bottom unchanged (saturation checked first; the hue-0 fallback is never read); Saturation: black top desaturates at the bottom's value — pinned behavior, not a guard; Value: no guard | enum docs `crates/umber-core/src/layers.rs:271-305`, `rgb_to_hsv` `:524`+ |
| channel planes are rgb-native | a channel's plane is rgb regardless of semantic (the Saturation routing test's red src bytes prove the plane carries full rgb) | `material_layer.rs:1775-1800` |

## The honesty gap

On a scalar-semantic channel (roughness/metallic/height/a mask), the
conventional content is gray (r=g=b). Run the quartet's math on gray:

| mode | math on gray top/bottom | effect |
|---|---|---|
| Tint | hue of top — top is achromatic → the guard fires | exact no-op |
| Color | hue+sat of top — the guard fires | exact no-op |
| Saturation | sat of top (0) + hue/value of bottom → gray at the bottom's value | exact no-op on gray |
| Value | value of top over bottom's hue/sat (0) → the top's scalar | ≈ Normal (redundant, not meaningless) |

Three of the four are provable no-ops on the channel's expected content;
the fourth duplicates Normal. The modes are still ACTIVE math: on
non-gray content in a scalar channel (nothing forbids a colored
roughness plane) the quartet runs real hue algebra on data the channel's
consumers read as data, not color. Selectable-but-marginal is the
honesty gap: the menu sells modes the channel cannot use meaningfully.

## Painter parity

Painter restricts the blend list per channel (public UI behavior;
summarized, not cited mode-by-mode here). Umber's enum census already
mirrors Painter's mode-LIST decisions variant by variant (the layers.rs
census docs); the per-channel restriction is the next parity step. The
driver-side gate already encodes the routing contract for Passthrough and
the normal trio ("the Normal map blending modes are only useful for the
Normal channel" — `is_normal_map`'s doc); the HSV quartet has no
per-channel story at either layer.

## The fork

### Option 1 — full per-channel mode tables (Painter's shape)
A name → mode-set map per channel mirroring Painter's per-channel lists.
+ Maximal parity.
− A large table to maintain and pin; every new channel name needs a
  ruling; duplicates the driver gate as a second enforcement site;
  over-restricts beyond the proven no-op set (opinionation without
  in-tree evidence).

### Option 2 — shape-keyed gate (REJECTED, kept as the documented trap)
Gate the picker on the channel's data shape (scalar vs color triplet).
REJECTED AS INFEASIBLE TODAY: `MaterialOutput` carries no type metadata
(`:292`) and channel planes are rgb-native — the shape does not exist at
the picker's altitude. Building it means threading mtlx output types
through discovery → MaterialOutput → picker: a real refactor, not a
gate. And shape would be the WRONG key even then: the load-bearing
distinction is semantic (what the channel's consumers do with the
plane), not the plane's byte shape.

### Option 3 — status quo + documentation
Keep everything selectable; document the gray-content behavior in the
mode row's tooltip.
+ Zero code, zero new tables.
− The honesty gap stays; parity divergence stays; tooltips do not fix a
  menu that offers provable no-ops.

### Option 4 — semantic scalar blocklist at the picker (RECOMMENDED)
Block exactly the proven no-op trio (Tint, Saturation, Color) on
scalar-semantic channels; keep Value everywhere (≈Normal on gray —
redundant, but meaningful and Painter-listed); keep everything else
as-is.
+ The smallest honest step: removes only the provable no-ops; the
  blocklist is DERIVED from the gray-content algebra + the guard
  semantics above, not imported opinion.
+ No new user-facing strings → no .ftl catalog work (hiding a mode adds
  no key — the v1 i18n boundary rule).
+ Testable by extending the existing `:1681` pin: roughness 28→25, the
  trio absent on scalar channels and present on baseColor, Value present
  everywhere, `normal` unchanged at 31.
− A small semantic predicate to maintain: `channel_is_color_semantic`
  with the known color set (baseColor/diffuse/albedo/emissive…) and
  `normal` special-cased; unknown names default SCALAR (masks/weights
  are the common custom-channel case — the honest default; a wrong
  default hides three modes and is recoverable, the reverse default
  leaves custom data channels carrying the no-op trio the kill removes).
− A second enforcement site beside the driver gate — accepted: the
  picker is the UX contract; the driver stays permissive (see non-goals).

## The staged code kill (BLOCKED on the battery — the plan)

**K1 (one kill, one file surface)** — `layers_panel.rs`:
`channel_is_color_semantic(name) -> bool` beside the picker; the filter
gains one clause blocking the no-op trio when the channel is
scalar-semantic; extend the `:1681` test (roughness 28→25 + the
presence/absence matrix); requirements.md row + CHANGELOG entry + this
doc's closure tick in the same commit. Surface: `layers_panel.rs` only.
**K2 (optional, separate kill)** — a tooltip explaining the hidden trio
on scalar channels; needs BOTH .ftl catalogs; only if the panel wants
the affordance.
**Gates**: kill-lock; `rustfmt --check --edition 2021` per touched file;
full battery (minimum scope umber-app + umber-core) when cargo is
re-sanctioned; cross-review by the other claw; rides a WAVE push —
never a per-kill push.

## Non-goals (named so they are not rediscovered as surprises)

- NO driver-side change: the gate at `material_layer.rs:997-1011` keeps
  evaluating whatever a saved document carries — a saved
  roughness+Tint from an older build still evals to its no-op rather
  than changing behavior under a moved menu. Picker-only is deliberate
  backward compatibility.
- NO rewriting of saved params: files carrying a now-unpickable combo
  stay valid; the picker only refuses NEW selections.
- NO Value blocklist entry: Value is the quartet's one meaningful member
  on scalar channels.

## Open questions (for the code-kill brief, not blockers)

1. The color-semantic set: no current template declares `emissive` as a
   material channel (grep, 2026-10-22); the predicate set is
   forward-looking and pinned by test.
2. Should the export path warn when a scalar channel's plane is
   non-gray (garbage-in detection)? Out of scope for K1; recorded as an
   audit-class idea.
3. i18n: K1 hides modes without adding strings; if a future kill
   explains the hiding, BOTH catalogs move in that kill.
