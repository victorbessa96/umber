# Umber Strategy Review — the adversarial pass (claude-opus, 2026-10-12)

The full review dispatched by Bessa ("review the project plans, the roadmap and all the requisites, expand on all of it and improve"). Preserved verbatim as the load-bearing planning document; the dragon's disposition follows at the bottom.

---

**Bottom line:** the engine crates are real. The product loop on top of them is broken: what you paint can't be seen in 3D, can't be undone, isn't saved, and takes no pen pressure. The P0 floor is not met.

## 1. GAPS: rows marked ✓ that have holes

- **[critical] The 3D viewport never shows paint** — mesh pass has no UV attribute, BASE_COLOR is a constant, OpenPBR binds no textures (shaders.rs:120/145/482). Undermines §5:77, §5:79, §5:87, §1:29.
- **[critical] Live paint target is 512², not 4K** (paint_state.rs:180); the "4K default" row (§2:43) is a test hook only.
- **[critical] Brush strokes have no undo** (paint_state.rs:445 "v1 — paint strokes have no undo"); the §12 baseline measures metadata bytes.
- **[critical] Painted pixels are not saved** — save_project writes metadata + graph only; reopen loses all paint. The round-trip tests never touch pixels.
- **[critical] Pen pressure never reaches the brush** — begin_stroke hardcodes 1.0; the stylus crate has zero uses in umber-app.
- **[critical] Only base color can be painted** — one RGBA8 PaintThread per tile, no layer/channel mapping; the §2 channel stack is a model with no pixels.
- **[major] Rows whose own text admits gaps** (camera "ortho pending", perf "the floor itself = the nightly run", masks "metadata model", history "record_context = the follow-up") — un-tick or split.
- **[major] P0 rows unmarked while STATE says "all 57 built"**: OCIO (stub-only), stylus, distribution (no MSI/Flatpak/AppImage).
- **[major] Golden coverage misses SPEC-required surfaces** (seam strokes, layer composites, bakes).
- **[minor] STATE.md contradicts itself** (status line "wave 3", handoff "everything done").

**Solid:** the bake engine, the export driver, the .umber JSON layer, the HDR row's honest boundary.

## 2. RISKS (top 5)

1. Ticks recorded faster than features exercised — no recorded interactive verification anywhere; the roadmap's own success test never ran.
2. The core loop is a scaffold — the six criticals all sit on the one user path.
3. The per-layer/per-channel tile pool + GPU compositor don't exist and are the hard 20% (~64MB per layer-channel-tile at 4K — prototype the memory model first).
4. OCIO's P0 depends on a C++ build that never ran; the display panel already shipped (the re-scope trigger fired).
5. Bus factor; SPEC v1.0 never ratified.

**Release checklist audit:** every gate checks process, none checks function → add Gate 0 (the journey test: FBX → 4K pressure paint → see lit → undo/redo → save/reopen → ORM export decode-verified). Also stale text, a P0 deferred under "known-deferred", installer promises the dist config can't produce, and Gate 5 compares against a missing perf/baseline.json.

## 3. EXPANSIONS (the credible-v0.1 table additions)

P0: color picker/eyedropper; ID-map mask selection; layer effects stack; stencil/decal; multi-texture-set docs; material painting as its own row; height→normal live in viewport.
P1: layer thumbnails (verify kill 1's claim); asset shelf; shortcut remapping + brush-size hotkey; UV overlap warnings at import; light rotation/preview sphere.

## 4. IMPROVEMENTS to the plan

1. **Core-loop phase BEFORE all GUI/P2 work** — freeze chrome kills until the Gate-0 journey passes.
2. **Split ✓ into "✓ code" vs "✓ reachable"** — only reachable counts toward the floor; est. 6-10 rows drop.
3. **Hold P2 while P0 has holes.**
4. **SPEC ratification + dogfood gate as release gates 6-7.**
5. **Re-baseline perf after the real data path lands**; commit perf/baseline.json.

## 5. VERDICT

**The P0/P1 completion claim is not honest as stated.** The engine work is substantial and good; the user-path joins don't exist. Until the core loop passes Gate 0, "v0.1.0 is a strong library set with a demo shell. Tagging it now would be the most damaging mistake available."

---

## The dragon's disposition (2026-10-12)

ACCEPTED IN FULL. The review's six criticals are the real shape of the gap — I ran 40 kills against the requirements TABLE while the product's spine (see-paint / undo-paint / save-paint / pressure / channels) was unbuilt. The code-vs-reachable split is adopted: the un-tick pass over the inflated rows comes first, then the core-loop phase becomes the only hunt.

**The queue order, effective now:**
1. The truth pass: un-tick/split the rows the review named (camera, perf floor, masks, history, OCIO, stylus, distribution); STATE.md rewritten from current truth.
2. The core-loop phase: viewport texture binding → per-layer/per-channel 4K tile pool + compositor → stroke undo → pixel save/load → Windows pressure. Each a kill; Gate 0 the phase's gate.
3. GUI + P2 frozen until Gate 0 passes.
4. The Gate-0 journey test added to the release checklist as the FIRST gate.
5. SPEC ratification + dogfood cohort on the release path.
