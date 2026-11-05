//! Persisted keymap bindings, format v2 — queue row 154, wave 4 prep
//! (`docs/loop/briefs/2026-11-02-commandspec-unification-design.md`,
//! migration wave 4: "the persisted format must migrate or be
//! namespaced v2"). Wired into `preferences.rs` (row 154 w4): saved as
//! the `keymap_v2=` line, read first on load with the v1 `keymap=` line
//! as fallback; this module is the format plus the v1 bridge.
//!
//! # v1 (what `preferences.rs` persists today)
//!
//! One `keymap=` line in the prefs file:
//! `keymap=Undo:ctrl+u;ToggleGrid:shift+g,f5;InvertMask:` — entries
//! `;`-separated, each `Action:spec,spec`, where the action is the
//! `ShellAction` Debug name ([`super::action_name`]: `Undo`,
//! `SelectTool(Erase)`) and the spec is [`super::binding_spec`]'s
//! platform-neutral spelling (`ctrl+shift+z`, `tab`). It is a plain
//! string line, not serde-on-the-enum; [`Overrides::parse`] reads it
//! leniently (unknown names/specs skipped, a typo never unbinds).
//!
//! # v2 (this module)
//!
//! ```json
//! { "version": 2,
//!   "bindings": {
//!     "edit.undo": { "id": "edit.undo",
//!                    "binding": [ { "key": "z", "chord": "cmd" } ] } } }
//! ```
//!
//! Records are keyed by [`ShellAction::id`]; each record repeats the id
//! and carries the action's full binding list as objects (`key` is egui's
//! `Key::name()` lowercased — the same spelling [`super::binding_spec`]
//! uses after the mods — and `chord` is `bare` | `shift` | `cmd` |
//! `cmdshift`; an empty list means unbound, at most
//! [`super::MAX_BINDINGS`] entries).
//!
//! # Decisions
//!
//! - Payload variants (`SelectTool(..)`) all share the id `tool.select`,
//!   so they cannot be keyed by id: v2 keeps their v1 Debug-name key
//!   (`"SelectTool(Erase)"`) while the record still carries
//!   `"id": "tool.select"`. Nothing is dropped.
//! - [`parse_overrides`] accepts v2 id-keyed records, v1 name-keyed
//!   records inside a v2 document, and a raw v1 `keymap=` string (a JSON
//!   string value). A key resolves as an id first, then as a v1 name.
//! - Strict where v1 was lenient: an unknown action key, an id/record
//!   mismatch, an unparseable binding or chord, a wrong `version`, or a
//!   non-object document are typed `Err`s — a record from a newer build
//!   must not vanish silently. The raw v1 string keeps its lenient parse.
//! - [`Overrides::set`]'s minimality is unchanged: a record equal to the
//!   defaults is dropped on parse, so the delta stays minimal and
//!   serialize/parse round-trips exactly.

// Wired into preferences.rs (row 154 w4); `name_to_id` is still only
// consumed by the tests, so the bin target would flag it dead.
#![cfg_attr(not(test), allow(dead_code))]

use serde_json::{json, Value};

use super::{action_from_name, action_name, parse_binding_spec, Binding, Chord, Overrides};
use crate::toolbar::ShellAction;

/// The persisted-bindings format version this module writes: v2 records
/// are keyed by the action's dotted command id ([`ShellAction::id`]).
pub const BINDINGS_FORMAT_VERSION: u32 = 2;

/// The dotted command id of the v1 persisted `name` — the name→id half
/// of the bridge (every [`super::ALL_ACTIONS`] name maps to its action's
/// id; payload variants map to the shared `tool.select`).
pub fn name_to_id(name: &str) -> Option<&'static str> {
    action_from_name(name).map(|action| action.id())
}

/// The chord's persisted name in a v2 binding object.
fn chord_name(chord: Chord) -> &'static str {
    match chord {
        Chord::Bare => "bare",
        Chord::Shift => "shift",
        Chord::Cmd => "cmd",
        Chord::CmdShift => "cmdshift",
    }
}

/// The inverse of [`chord_name`]; strict (`alt` and friends are an `Err`).
fn chord_from_name(name: &str) -> Option<Chord> {
    Some(match name {
        "bare" => Chord::Bare,
        "shift" => Chord::Shift,
        "cmd" => Chord::Cmd,
        "cmdshift" => Chord::CmdShift,
        _ => return None,
    })
}

/// One binding as a v2 JSON object: `{ "key": "z", "chord": "cmd" }`.
fn binding_json(binding: Binding) -> Value {
    json!({
        "key": binding.key.name().to_lowercase(),
        "chord": chord_name(binding.chord),
    })
}

/// The inverse of [`binding_json`]. Unknown fields in the object are
/// ignored (forward compatibility); a missing/unknown `key` or `chord`
/// is an `Err`.
fn binding_from_json(value: &Value) -> Result<Binding, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("binding must be an object {{key, chord}}, got {value}"))?;
    let key_name = obj
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("binding needs a string \"key\": {value}"))?;
    let key = egui::Key::ALL
        .iter()
        .copied()
        .find(|k| k.name().eq_ignore_ascii_case(key_name))
        .ok_or_else(|| format!("unknown key name {key_name:?}"))?;
    let chord_name = obj
        .get("chord")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("binding needs a string \"chord\": {value}"))?;
    let chord = chord_from_name(chord_name)
        .ok_or_else(|| format!("unknown chord {chord_name:?} (bare|shift|cmd|cmdshift)"))?;
    Ok(Binding { key, chord })
}

/// A record's binding list. Accepts the v2 form (an array of binding
/// objects, or one object), the v1 form (an array of spec strings such
/// as `"ctrl+z"`, or one spec string), or a mix.
fn bindings_field(value: &Value) -> Result<Vec<Binding>, String> {
    let one = |item: &Value| -> Result<Binding, String> {
        match item {
            Value::String(spec) => {
                parse_binding_spec(spec).ok_or_else(|| format!("unknown binding spec {spec:?}"))
            }
            other => binding_from_json(other),
        }
    };
    match value {
        Value::Array(items) => items.iter().map(one).collect(),
        other => one(other).map(|b| vec![b]),
    }
}

/// The v2 document for `overrides`:
/// `{ "version": 2, "bindings": { <key>: { "id", "binding": [...] } } }`.
///
/// `<key>` is [`ShellAction::id`] for payloadless actions; payload
/// variants (`SelectTool(..)`, whose id `tool.select` is shared) keep
/// their v1 Debug-name key so no binding is lost (see the module docs).
pub fn serialize_overrides_v2(overrides: &Overrides) -> Value {
    let mut bindings = serde_json::Map::new();
    for (action, list) in overrides.iter() {
        let key = match ShellAction::from_id(action.id()) {
            Some(fixed) if fixed == action => action.id().to_string(),
            _ => action_name(action),
        };
        let binding: Vec<Value> = list.iter().map(|b| binding_json(*b)).collect();
        bindings.insert(key, json!({ "id": action.id(), "binding": binding }));
    }
    json!({
        "version": BINDINGS_FORMAT_VERSION,
        "bindings": Value::Object(bindings),
    })
}

/// Parses persisted overrides, accepting both formats (see the module
/// docs): a v2 document (id-keyed records, v1 name-keyed records mixed
/// in), or a raw v1 `keymap=…` line as a JSON string (the `keymap=`
/// prefix is optional; that form keeps [`Overrides::parse`]'s lenient
/// skip-unknowns contract). Anything else — wrong shape, wrong version,
/// unknown action key, id mismatch, unparseable binding — is a typed
/// `Err`; this function never panics.
pub fn parse_overrides(json: &Value) -> Result<Overrides, String> {
    match json {
        Value::String(line) => {
            let line = line.strip_prefix("keymap=").unwrap_or(line);
            Ok(Overrides::parse(line))
        }
        Value::Object(obj) => {
            if let Some(version) = obj.get("version") {
                let version = version
                    .as_u64()
                    .ok_or_else(|| format!("\"version\" must be a number, got {version}"))?;
                if version != u64::from(BINDINGS_FORMAT_VERSION) {
                    return Err(format!(
                        "unsupported bindings format version {version} (expected {BINDINGS_FORMAT_VERSION})"
                    ));
                }
            }
            let bindings = obj
                .get("bindings")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    "expected a v2 document: {\"version\": 2, \"bindings\": {…}}".to_string()
                })?;
            let mut out = Overrides::default();
            for (key, record) in bindings {
                // v2 keys are dotted ids; v1 keys are Debug names. An
                // unknown key is a typed error, never silently dropped.
                let action = ShellAction::from_id(key)
                    .or_else(|| action_from_name(key))
                    .ok_or_else(|| format!("unknown action key {key:?}"))?;
                let list = match record {
                    Value::Object(rec) => {
                        if let Some(id) = rec.get("id") {
                            let id = id.as_str().ok_or_else(|| {
                                format!("record {key:?}: \"id\" must be a string")
                            })?;
                            if id != action.id() {
                                return Err(format!(
                                    "record {key:?} claims id {id:?}, which is {}",
                                    action.id()
                                ));
                            }
                        }
                        match rec.get("binding") {
                            Some(binding) => bindings_field(binding)?,
                            None if rec.contains_key("key") => vec![binding_from_json(record)?],
                            None => {
                                return Err(format!("record {key:?} has no \"binding\""));
                            }
                        }
                    }
                    other => bindings_field(other)?,
                };
                out.set(action, list);
            }
            Ok(out)
        }
        other => Err(format!(
            "expected a v2 bindings object or a v1 keymap= string, got {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{default_bindings, ALL_ACTIONS};
    use crate::toolbar::ToolKind;
    use egui::Key;

    fn b(key: Key, chord: Chord) -> Binding {
        Binding { key, chord }
    }

    fn sample_overrides() -> Overrides {
        let mut o = Overrides::default();
        o.set(ShellAction::Undo, vec![b(Key::U, Chord::Cmd)]);
        o.set(
            ShellAction::ToggleGrid,
            vec![b(Key::G, Chord::Shift), b(Key::F5, Chord::Bare)],
        );
        o.set(ShellAction::InvertMask, Vec::new()); // unbound
        o.set(
            ShellAction::SelectTool(ToolKind::Erase),
            vec![b(Key::E, Chord::Cmd)],
        );
        o
    }

    #[test]
    fn v2_serialize_parse_round_trips_identically() {
        let o = sample_overrides();
        let doc = serialize_overrides_v2(&o);
        assert_eq!(doc["version"], json!(BINDINGS_FORMAT_VERSION));
        let back = parse_overrides(&doc).expect("v2 document parses");
        assert_eq!(back, o);
        assert_eq!(serialize_overrides_v2(&back), doc, "serialize is stable");
    }

    #[test]
    fn v2_records_are_keyed_by_id_with_structured_bindings() {
        let mut o = Overrides::default();
        o.set(ShellAction::Undo, vec![b(Key::U, Chord::Cmd)]);
        let doc = serialize_overrides_v2(&o);
        assert_eq!(
            doc["bindings"]["edit.undo"],
            json!({ "id": "edit.undo", "binding": [ { "key": "u", "chord": "cmd" } ] })
        );
        // An unbound action is an empty binding list, not a missing record.
        o.set(ShellAction::ToggleGrid, Vec::new());
        let doc = serialize_overrides_v2(&o);
        assert_eq!(
            doc["bindings"]["view.toggle_grid"],
            json!({ "id": "view.toggle_grid", "binding": [] })
        );
    }

    #[test]
    fn payload_variants_keep_their_v1_name_key_but_carry_the_id() {
        let o = sample_overrides();
        let doc = serialize_overrides_v2(&o);
        // All six SelectTool kinds share "tool.select", so the map key
        // stays the v1 Debug name; the record's "id" is the dotted id.
        assert_eq!(
            doc["bindings"]["SelectTool(Erase)"],
            json!({ "id": "tool.select", "binding": [ { "key": "e", "chord": "cmd" } ] })
        );
        assert!(doc["bindings"].get("tool.select").is_none());
        assert_eq!(parse_overrides(&doc).expect("parses"), o);
    }

    #[test]
    fn v1_line_migrates_to_id_keyed_v2_and_back() {
        let v1 = "keymap=Undo:ctrl+u;ToggleGrid:shift+g,f5;InvertMask:;SelectTool(Erase):ctrl+e";
        let o = parse_overrides(&json!(v1)).expect("v1 line parses");
        assert_eq!(o.get(ShellAction::Undo), Some(&[b(Key::U, Chord::Cmd)][..]));
        assert!(
            matches!(o.get(ShellAction::InvertMask), Some(list) if list.is_empty()),
            "unbound stays unbound"
        );
        assert_eq!(
            o.get(ShellAction::SelectTool(ToolKind::Erase)),
            Some(&[b(Key::E, Chord::Cmd)][..])
        );
        // Serialized as v2 the records are id-keyed (the payload variant
        // keeps its name key), and v2 parses back to the same delta.
        let v2 = serialize_overrides_v2(&o);
        assert!(v2["bindings"].get("edit.undo").is_some());
        assert!(v2["bindings"].get("view.toggle_grid").is_some());
        assert!(v2["bindings"].get("paint.invert_mask").is_some());
        assert!(v2["bindings"].get("SelectTool(Erase)").is_some());
        assert!(v2["bindings"].get("Undo").is_none());
        assert_eq!(parse_overrides(&v2).expect("v2 parses"), o);
        // The prefix-less line parses too.
        assert_eq!(
            parse_overrides(&json!("Undo:ctrl+u")).expect("bare line"),
            parse_overrides(&json!("keymap=Undo:ctrl+u")).expect("prefixed line")
        );
    }

    #[test]
    fn v1_name_keyed_records_inside_a_v2_document_parse() {
        let doc = json!({ "bindings": {
            "Undo": "ctrl+u",
            "ToggleGrid": ["shift+g", "f5"],
            "SelectTool(Smudge)": { "id": "tool.select",
                                    "binding": [ { "key": "s", "chord": "shift" } ] },
            "edit.redo": [ { "key": "y", "chord": "cmdshift" } ],
        }});
        let o = parse_overrides(&doc).expect("mixed v1/v2 records parse");
        assert_eq!(o.get(ShellAction::Undo), Some(&[b(Key::U, Chord::Cmd)][..]));
        assert_eq!(
            o.get(ShellAction::ToggleGrid),
            Some(&[b(Key::G, Chord::Shift), b(Key::F5, Chord::Bare)][..])
        );
        assert_eq!(
            o.get(ShellAction::SelectTool(ToolKind::Smudge)),
            Some(&[b(Key::S, Chord::Shift)][..])
        );
        assert_eq!(
            o.get(ShellAction::Redo),
            Some(&[b(Key::Y, Chord::CmdShift)][..])
        );
        // …and re-serializing keys them by id (payload variant by name).
        let v2 = serialize_overrides_v2(&o);
        assert!(v2["bindings"].get("edit.undo").is_some());
        assert!(v2["bindings"].get("SelectTool(Smudge)").is_some());
        assert_eq!(parse_overrides(&v2).expect("v2 parses"), o);
    }

    #[test]
    fn corrupt_input_is_a_typed_error_never_a_panic() {
        let bad = [
            json!(42),
            json!(null),
            json!(["edit.undo"]),
            json!({}),                               // no "bindings"
            json!({ "version": 3, "bindings": {} }), // future version
            json!({ "version": "two", "bindings": {} }),
            json!({ "bindings": 5 }),
            json!({ "bindings": { "no.such.command": [] } }), // unknown id
            json!({ "bindings": { "Bogus": [] } }),           // unknown v1 name
            json!({ "bindings": { "tool.select": [] } }),     // payload id, no fixed action
            json!({ "bindings": { "edit.undo": { "id": "edit.redo", "binding": [] } } }),
            json!({ "bindings": { "edit.undo": { "id": "edit.undo" } } }), // no binding
            json!({ "bindings": { "edit.undo": { "binding": [ { "key": "z", "chord": "alt" } ] } } }),
            json!({ "bindings": { "edit.undo": { "binding": [ { "key": "nosuchkey", "chord": "cmd" } ] } } }),
            json!({ "bindings": { "edit.undo": { "binding": [ { "chord": "cmd" } ] } } }),
            json!({ "bindings": { "edit.undo": { "binding": "alt+z" } } }),
            json!({ "bindings": { "edit.undo": { "binding": [7] } } }),
        ];
        for input in bad {
            let err = parse_overrides(&input).expect_err(&format!("must fail: {input}"));
            assert!(!err.is_empty(), "error names the problem: {input}");
        }
        let err = parse_overrides(&json!({ "bindings": { "no.such.command": [] } })).unwrap_err();
        assert!(err.contains("no.such.command"), "{err}");
        let err = parse_overrides(&json!({ "version": 3, "bindings": {} })).unwrap_err();
        assert!(err.contains("version 3"), "{err}");
    }

    #[test]
    fn empty_overrides_round_trip_through_v2() {
        let o = Overrides::default();
        let doc = serialize_overrides_v2(&o);
        assert_eq!(doc, json!({ "version": 2, "bindings": {} }));
        assert_eq!(parse_overrides(&doc).expect("parses"), o);
    }

    #[test]
    fn default_equal_records_stay_minimal_in_v2() {
        let mut o = Overrides::default();
        // Equal to the defaults → the delta drops the entry, as in v1.
        o.set(ShellAction::Undo, default_bindings(ShellAction::Undo));
        let doc = serialize_overrides_v2(&o);
        assert_eq!(doc["bindings"].as_object().expect("object").len(), 0);
        assert_eq!(parse_overrides(&doc).expect("parses"), Overrides::default());
    }

    #[test]
    fn every_action_name_maps_to_its_id_and_every_id_round_trips() {
        for action in ALL_ACTIONS {
            let name = action_name(*action);
            assert_eq!(name_to_id(&name), Some(action.id()), "{name}");
            if let Some(fixed) = ShellAction::from_id(action.id()) {
                assert_eq!(fixed, *action, "{}", action.id());
            }
        }
        // The payload id has no fixed action, by design.
        assert_eq!(ShellAction::from_id("tool.select"), None);
        assert_eq!(name_to_id("SelectTool(Paint)"), Some("tool.select"));
    }
}
