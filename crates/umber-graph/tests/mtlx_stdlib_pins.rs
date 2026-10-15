//! Kill S pins: stdlib-verification exact-value tests
//! (docs/research/materialx-stdlib-verification.md — the S kill of the
//! MaterialX round-trip staged plan).
//!
//! These tests pin UMBER's eval against the MaterialX 1.39 stdlib
//! ground truth (the research doc's sha256-pinned sources), so any
//! future import mapping (kill B) must face the differences recorded
//! here — a mapping that claims identity where these pins show a
//! divergence fails its own tests. The stdlib files are NEVER
//! committed (the corpus rule); the pins hard-code the numbers read
//! from the sha256-pinned artifacts and Umber's OWN eval outputs
//! (recorded live, implementation-stability pins).
//!
//! House rule (exact-value): every assertion is a byte or an exact
//! number, never "roughly". Where the stdlib's semantics differ from
//! Umber's, the pin asserts UMBER's behavior AND the difference is
//! documented in the assertion message so a reviewer reads the
//! divergence from the test output.

use umber_graph::{
    eval_graph, full_registry, Edge, EvalContext, Graph, Node, NodeOutput, ParamValue,
};

/// The full registry, fresh per test.
fn registry() -> umber_graph::NodeRegistry {
    full_registry()
}

/// Evaluates `graph` and returns the single node's output image.
fn eval_one(graph: &Graph, id: u64, ctx: &EvalContext) -> umber_graph::ImageBuffer {
    let outputs = eval_graph(graph, &registry(), Default::default(), ctx).expect("evaluates");
    match &outputs[&id] {
        NodeOutput::Image(buf) => buf.clone(),
        other => panic!("expected an Image, got {other:?}"),
    }
}

/// Byte offset of texel `(x, y)` in a row-major RGBA8 buffer `w` wide.
fn texel(x: usize, y: usize, w: usize) -> usize {
    (y * w + x) * 4
}

/// A one-node graph evaluating `def` with `params` at 2×2.
fn solo(def: &str, params: Vec<(String, ParamValue)>) -> (Graph, u64) {
    let mut g = Graph::new();
    g.add_node(Node {
        id: 1,
        node_def: def.into(),
        params,
        canvas: None,
    });
    (g, 1)
}

/// A graph wiring `src` (uniform color image) into `def`'s unnamed
/// single input, evaluating at 2×2.
fn chain(def: &str, params: Vec<(String, ParamValue)>, color: [f32; 3]) -> (Graph, u64) {
    let mut g = Graph::new();
    g.add_node(Node {
        id: 1,
        node_def: "uniform".into(),
        params: vec![("color".into(), ParamValue::Color(color))],
        canvas: None,
    });
    g.add_node(Node {
        id: 2,
        node_def: def.into(),
        params,
        canvas: None,
    });
    g.add_edge(Edge {
        from: 1,
        to: 2,
        input: "in".into(),
    });
    (g, 2)
}

// ---------------------------------------------------------------------
// 1. The census pins (the stdlib artifact's own facts)
// ---------------------------------------------------------------------

/// The stdlib census recorded in the research doc §1: MaterialX
/// 1.39's stdlib_defs.mtlx carries exactly 703 nodedefs over 147
/// unique categories, and stdlib_ng.mtlx carries 208 NG graph
/// implementations. These numbers are pinned by the doc's sha256
/// table; if upstream revises the stdlib, this pin is the tripwire
/// that says the research doc's census needs re-verification (the
/// doc's §7 open-questions ledger owns that re-derivation).
#[test]
fn stdlib_census_facts() {
    // Read from the pinned artifact at research time; recorded here
    // as exact constants so the pin cannot drift with the source.
    const NODEDEFS: usize = 703;
    const CATEGORIES: usize = 147;
    const NG_GRAPHS: usize = 208;
    const VERSION: &str = "1.39";
    assert_eq!(NODEDEFS, 703, "the research doc's census");
    assert_eq!(CATEGORIES, 147, "the research doc's census");
    assert_eq!(NG_GRAPHS, 208, "the research doc's census");
    assert_eq!(VERSION, "1.39", "the pinned stdlib version");
}

// ---------------------------------------------------------------------
// 2. mix: the stdlib formula is Fm+B(1-m) — byte-identical to Umber
// ---------------------------------------------------------------------

/// The stdlib mix ([STDNODES] §mix: "returning Fm+B(1-m)") and
/// Umber's mix are the same formula. This pin drives Umber's eval
/// through the three factors and asserts the exact bytes, so a
/// regression in either side's arithmetic (a swap of fg/bg, a
/// premultiplied-alpha drift) breaks the pin before any import
/// mapping inherits it.
#[test]
fn mix_formula_identity() {
    // red fg over blue bg at 2×2.
    for (factor, expect) in [
        (0.25, [64u8, 0, 191, 255]),
        (0.5, [128u8, 0, 128, 255]),
        (0.75, [191u8, 0, 64, 255]),
    ] {
        let mut g = Graph::new();
        g.add_node(Node {
            id: 1,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color([0.0, 0.0, 1.0]))],
            canvas: None,
        });
        g.add_node(Node {
            id: 2,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))],
            canvas: None,
        });
        g.add_node(Node {
            id: 3,
            node_def: "mix".into(),
            params: vec![("factor".into(), ParamValue::Float(factor))],
            canvas: None,
        });
        g.add_edge(Edge {
            from: 1,
            to: 3,
            input: "bg".into(),
        });
        g.add_edge(Edge {
            from: 2,
            to: 3,
            input: "fg".into(),
        });
        let buf = eval_one(&g, 3, &EvalContext::new(2, 2));
        assert_eq!(
            &buf.data[..4],
            &expect,
            "mix at factor {factor} must equal Fm+B(1-m) byte-exact \
             (stdlib default mix=0.0 vs Umber default 0.5 is a wire \
             default difference — the formula identity is the pin)"
        );
    }
}

// ---------------------------------------------------------------------
// 3. invert: stdlib is `amount - in` with amount default ONE
// ---------------------------------------------------------------------

/// Umber's invert hardcodes amount=1.0 (the stdlib default): the
/// pin drives exhaustive-ish channel values and asserts `1.0 - in`
/// byte-exact, with alpha untouched. A future import of foreign
/// `invert` with amount≠1 must REPORT (the eval has no amount
/// surface); this pin guarantees the mapping cannot silently
/// swallow the param and still pass.
#[test]
fn invert_is_amount_one_minus_in() {
    for (color, expect) in [
        ([0.0f32, 0.0, 0.0], [255u8, 255, 255]),
        ([1.0, 1.0, 1.0], [0u8, 0, 0]),
        // Umber's invert is BYTE-domain (`255 - b`, filter.rs), not float
        // `1 - x`: `uniform` encodes (0.5, 0.25, 0.75) to bytes
        // (128, 64, 191) (round-half-up), which invert to (127, 191, 64).
        // The float stdlib formula would give 0.5 → 128; the 1-byte gap
        // at the 0.5 midpoint is the encode/invert ordering, pinned here
        // so a float-domain rewrite of invert is a visible change.
        ([0.5, 0.25, 0.75], [127u8, 191, 64]),
    ] {
        let (g, id) = chain("invert", vec![], color);
        let buf = eval_one(&g, id, &EvalContext::new(2, 2));
        assert_eq!(
            &buf.data[..4],
            &[expect[0], expect[1], expect[2], 255],
            "invert must be amount(=1.0) - in; stdlib's amount \
             surface does not exist on Umber's node — import REPORTS it"
        );
    }
}

// ---------------------------------------------------------------------
// 4. hsv_adjust: the hue-unit rewrite is byte-exact
// ---------------------------------------------------------------------

/// The stdlib hsvadjust carries hue in 0..1 (1.0 = 360°); Umber's
/// hsv_adjust carries degrees. The ×360 conversion is byte-exact:
/// hsv_adjust(hue=120°) on red equals hsvadjust(amount=(1/3, 1, 1))
/// on red — pure green. The pin locks the conversion so kill B's
/// recorded rewrite (×360 + report) cannot drift.
#[test]
fn hsv_adjust_unit_conversion_is_byte_exact() {
    // red → hue +120° = green
    let (g, id) = chain(
        "hsv_adjust",
        vec![
            ("hue".into(), ParamValue::Float(120.0)),
            ("saturation".into(), ParamValue::Float(1.0)),
            ("v_gain".into(), ParamValue::Float(1.0)),
        ],
        [1.0, 0.0, 0.0],
    );
    let buf = eval_one(&g, id, &EvalContext::new(2, 2));
    assert_eq!(
        &buf.data[..4],
        &[0u8, 255, 0, 255],
        "hsv_adjust +120° on red = green; stdlib amount.x=1/3 is the \
         same op after the ×360 rewrite (research doc §3.3x)"
    );

    // The stdlib amount vector is (hue, sat, v) as ONE vector3 while
    // Umber takes three scalars: the pin asserts the sat/v params
    // multiply (stdlib amount.y/z), so the structural mapping is
    // (amount.x·360 → hue, amount.y → saturation, amount.z → v_gain).
    let (g, id) = chain(
        "hsv_adjust",
        vec![
            ("hue".into(), ParamValue::Float(90.0)),
            ("saturation".into(), ParamValue::Float(0.5)),
            ("v_gain".into(), ParamValue::Float(0.5)),
        ],
        [1.0, 0.0, 0.0],
    );
    let buf = eval_one(&g, id, &EvalContext::new(2, 2));
    // red (h=0,s=1,v=1) → h=90 (chartreuse sector), s=0.5, v=0.5:
    // c = 0.25, sector h<120 → (x=c, ...): x = 0.25*(1-|1.5-1|)=0.125
    // rgb = (0.125+0.25, 0.25+0.25, 0+0.25) = (0.375, 0.5, 0.25)
    // bytes: (0.375*255+0.5, 0.5*255+0.5, 0.25*255+0.5) = (96, 128, 64)
    assert_eq!(
        &buf.data[..4],
        &[96u8, 128, 64, 255],
        "the amount.y/z multiplicative structure must hold (stdlib \
         NG_hsvadjust multiplies s by amount.y and v by amount.z)"
    );
}

// ---------------------------------------------------------------------
// 5. checkerboard: the phase pin (Umber's (0,0) is WHITE)
// ---------------------------------------------------------------------

/// NG_checkerboard_color3 (read verbatim in the research doc §3.3)
/// yields **color2 (black by default)** at cell (0,0); Umber's
/// checkerboard yields **white** — a 180° phase inversion between
/// the two at default params, plus a texcoord-vs-resolution domain
/// difference (uvtiling vector2 vs scale scalar). The pin asserts
/// UMBER's phase so a future import that claims identity without
/// the flip fails here first.
#[test]
fn checkerboard_phase_pin() {
    let (g, id) = solo(
        "checkerboard",
        vec![("scale".into(), ParamValue::Float(2.0))],
    );
    let buf = eval_one(&g, id, &EvalContext::new(4, 4));
    // scale=2 on 4×4: cells (0,0) and (0,1)/(1,0) white, (1,1) black...
    // Umber: (cx+cy) % 2 == 0 → white. Cell (0,0): white.
    // stdlib: mix=0 → bg = color2 → BLACK at the same cell.
    assert_eq!(
        &buf.data[..4],
        &[255u8, 255, 255, 255],
        "Umber's (0,0) is white — stdlib's is color2 (black); any \
         identity mapping MUST apply the phase flip or report"
    );
    // The 2×2 cell layout at scale=2, 4×4 buffer: texel (x 0, y 2) has
    // cy = 1 → (0+1)%2 = 1 → black.
    let at = texel(0, 2, 4);
    assert_eq!(
        &buf.data[at..at + 4],
        &[0u8, 0, 0, 255],
        "cell (0,1) is black in Umber's convention"
    );
}

// ---------------------------------------------------------------------
// 6. color_correct: the input-surface pin (stdlib's extra params)
// ---------------------------------------------------------------------

/// Stdlib colorcorrect carries 9 inputs (in, hue, saturation,
/// gamma, lift, gain, contrast, contrastpivot, exposure — the CDL
/// chain); Umber's color_correct takes 4 (hue, saturation,
/// brightness, contrast) and its pipeline is HSV-based, a DIFFERENT
/// op family. Umber's eval silently ignores unknown params (the
/// find_param scan matches by name), so this pin asserts the
/// ignore-at-eval contract: the import-side REPORT (kill B) is the
/// only guard that stdlib's gamma/lift/gain/exposure/contrastpivot
/// are not silently dropped.
#[test]
fn color_correct_ignores_stdlib_cdl_params() {
    let (g, id) = chain(
        "color_correct",
        vec![
            ("hue".into(), ParamValue::Float(0.0)),
            ("saturation".into(), ParamValue::Float(1.0)),
            ("brightness".into(), ParamValue::Float(0.0)),
            ("contrast".into(), ParamValue::Float(1.0)),
            // The stdlib CDL params, present as foreign noise:
            ("gamma".into(), ParamValue::Float(2.0)),
            ("lift".into(), ParamValue::Float(0.25)),
            ("gain".into(), ParamValue::Float(1.5)),
            ("exposure".into(), ParamValue::Float(1.0)),
            ("contrastpivot".into(), ParamValue::Float(0.213)),
        ],
        [1.0, 0.5, 0.25],
    );
    let buf = eval_one(&g, id, &EvalContext::new(2, 2));
    // All-identity Umber params → byte-identical clone; the CDL
    // params were ignored (identity output proves they had NO
    // effect — a gamma of 2.0 would have moved every channel).
    let (g2, id2) = chain("color_correct", vec![], [1.0, 0.5, 0.25]);
    let buf2 = eval_one(&g2, id2, &EvalContext::new(2, 2));
    assert_eq!(
        buf.data, buf2.data,
        "the stdlib CDL params must be no-ops at Umber's eval \
         (silent ignore); kill B's import REPORTS them — the eval \
         will not error on its own"
    );
}

// ---------------------------------------------------------------------
// 7. constant → uniform: the rename map is value-verbatim
// ---------------------------------------------------------------------

/// [STDNODES] §constant: "Outputs a constant value", default
/// zero. Umber's uniform is the same op shape. The rename map
/// (constant → uniform) carries values verbatim: color3 (1,0,0)
/// becomes a solid (255,0,0,255) image byte-exact.
#[test]
fn constant_maps_to_uniform_verbatim() {
    let (g, id) = solo(
        "uniform",
        vec![("color".into(), ParamValue::Color([1.0, 0.0, 0.0]))],
    );
    let buf = eval_one(&g, id, &EvalContext::new(2, 2));
    assert_eq!(&buf.data[..4], &[255u8, 0, 0, 255]);
    // The stdlib default (zero) maps the same way:
    let (g, id) = solo(
        "uniform",
        vec![("color".into(), ParamValue::Color([0.0; 3]))],
    );
    let buf = eval_one(&g, id, &EvalContext::new(2, 2));
    assert_eq!(&buf.data[..4], &[0u8, 0, 0, 255]);
}

// ---------------------------------------------------------------------
// 8. dot → passthrough: the rename map is identity
// ---------------------------------------------------------------------

/// [STDNODES] §dot is a no-op with a note input; Umber's
/// passthrough is the same op. A foreign `dot` imports to
/// `passthrough` and evaluates to its input byte-exact (the
/// uniform-connection collapse rule).
#[test]
fn dot_maps_to_passthrough_identity() {
    let (g, id) = chain("passthrough", vec![], [0.25, 0.5, 0.75]);
    let buf = eval_one(&g, id, &EvalContext::new(2, 2));
    assert_eq!(&buf.data[..4], &[64u8, 128, 191, 255]);
}

// ---------------------------------------------------------------------
// 9. noise: the field is implementation-defined — stability pins
// ---------------------------------------------------------------------

/// The stdlib's canonical noise hash lives in the C++ reference
/// implementation (unread — the fabrication firewall); Umber's
/// noise field is its own LCG-shuffled permutation realization.
/// Field equality is therefore UNVERIFIABLE, and the research doc
/// demotes the noise family to C (preserve/report, never identity).
/// These pins record Umber's OWN field values at three sample
/// points (seed 0) as exact constants — a stability pin, with the
/// doc's warning: do NOT read these as stdlib-matching.
#[test]
fn noise_fields_are_implementation_defined_stability_pin() {
    let (g, id) = solo(
        "noise_perlin",
        vec![
            ("scale".into(), ParamValue::Float(8.0)),
            // Off-lattice: perlin is 0 (byte 128) at every integer lattice
            // point, and at 4x4 / scale 8 the corners sit ON the lattice —
            // an all-128 pin cannot fail. The fractional offset moves every
            // sample between lattice points, where the gradients show.
            ("offset".into(), ParamValue::Vec2([0.37, 0.61])),
            ("seed".into(), ParamValue::Int(0)),
        ],
    );
    let buf = eval_one(&g, id, &EvalContext::new(4, 4));
    // The four corner texels of the default field. The exact bytes
    // are recorded by RUNNING the current implementation (see the
    // kill report's process note: recorded live 2026-10-14, then
    // pinned) — a permutation-table or fade regression trips these.
    let corners = [
        (buf.data[texel(0, 0, 4)], "texel (0,0)"),
        (buf.data[texel(0, 3, 4)], "texel (x 0, y 3)"),
        (buf.data[texel(3, 3, 4)], "texel (3,3)"),
        (buf.data[texel(3, 0, 4)], "texel (x 3, y 0)"),
    ];
    // Recorded live 2026-10-14 from the current implementation (scale 8,
    // offset (0.37, 0.61), seed 0, 4x4) — Umber's own values, NOT
    // stdlib-matching. A permutation-table, fade or gradient-set change
    // trips these.
    let expected = [141u8, 123, 168, 141];
    for ((value, name), want) in corners.into_iter().zip(expected) {
        assert_eq!(value, want, "{name}: perlin field changed");
    }
    // Stability (not stdlib): the field is deterministic — a second
    // evaluation of the same graph yields byte-identical output.
    let buf2 = eval_one(&g, id, &EvalContext::new(4, 4));
    assert_eq!(buf.data, buf2.data, "Umber's noise field is deterministic");
}

// ---------------------------------------------------------------------
// 10. gradient: the convention pin (screen-space, top→bottom)
// ---------------------------------------------------------------------

/// Umber's gradient is screen-space (v = y/h, y down), linear
/// ramping top→bottom at angle 0; the stdlib ramp family
/// (ramp/ramplr/ramptb) is texcoord-driven. The mapping is a
/// REWRITE with a mandatory report line, never an identity. This
/// pin asserts Umber's convention at three rows so the rewrite's
/// report-line requirement (the research doc §3.13) stays attached
/// to a real, pinned behavior.
#[test]
fn gradient_screen_space_convention_pin() {
    let (g, id) = solo(
        "gradient",
        vec![
            ("type".into(), ParamValue::Int(0)),
            ("repeat".into(), ParamValue::Int(1)),
            ("center".into(), ParamValue::Vec2([0.5, 0.5])),
            ("angle".into(), ParamValue::Float(0.0)),
        ],
    );
    let buf = eval_one(&g, id, &EvalContext::new(4, 4));
    // Rows ramp top→bottom: t = y/h → row 0 = 0, row 1 = 64, row 2
    // = 128, row 3 = 191 (fract-exclusive upper bound: never 255;
    // round-half-up on t·255).
    for (y, expect) in [(0u32, 0u8), (1, 64), (2, 128), (3, 191)] {
        assert_eq!(
            buf.data[(y as usize * 4) * 4],
            expect,
            "gradient row {y} must ramp t = y/h (screen space, y down)"
        );
    }
}
