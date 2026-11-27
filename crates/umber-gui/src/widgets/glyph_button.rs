//! Glyph-only small buttons with a mandatory tooltip (round-3 audit F8).
//!
//! Every glyph button in the app goes through [`glyph_button`]: the
//! tooltip is a required argument and doubles as the button's AccessKit
//! label, so a tooltip-less glyph button cannot drift in again.

use egui::{Response, Ui, WidgetInfo, WidgetType};

/// A small glyph button whose tooltip is mandatory and is also the
/// button's accessible name (`WidgetInfo::labeled`, same pattern as
/// `tool_row`'s row widget).
pub fn glyph_button(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    let r = ui.small_button(glyph).on_hover_text(tooltip);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tooltip));
    r
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("app source directory must be readable") {
            let path = entry.expect("source directory entry").path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                out.push(path);
            }
        }
    }

    /// A non-ASCII symbol (not an accented letter) or an UPPER_SNAKE glyph
    /// constant anywhere in a button-construction block. Looking at the
    /// whole block covers `selectable_label(condition, label)` and multiline
    /// calls instead of mistaking the condition for the label.
    fn looks_like_glyph_call(call: &str) -> bool {
        call.contains("_GLYPH") || call.chars().any(|c| !c.is_ascii() && !c.is_alphabetic())
    }

    /// The current constructor call only, through its balanced closing
    /// parenthesis. The `widget_info` lookahead is deliberately separate:
    /// a later button's glyph must not contaminate this call.
    fn call_block(lines: &[&str], index: usize) -> String {
        let mut block = String::new();
        let mut depth = 0i32;
        for line in lines[index..(index + 12).min(lines.len())].iter() {
            for c in line.chars() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            block.push_str(line);
            block.push(' ');
            if depth <= 0 {
                break;
            }
        }
        block
    }

    #[test]
    fn detector_fires_on_every_supported_shape_and_rejects_text() {
        for call in [
            r#"ui.small_button("✎")"#,
            r#"Button::new("▲")"#,
            r#"selectable_label(!hidden, if hidden { "·" } else { "👁" })"#,
            r#"ui.button("✕")"#,
            "ui.small_button(DUPLICATE_GLYPH)",
            "ui.button(\n    \"✕\"\n)",
        ] {
            assert!(looks_like_glyph_call(call), "not detected: {call}");
        }
        for text in [
            r#"ui.button("Save")"#,
            r#"ui.button(tr("layers.duplicate"))"#,
            r#"ui.button("Ação")"#,
        ] {
            assert!(!looks_like_glyph_call(text), "false positive: {text}");
        }
    }

    #[test]
    fn app_has_no_unlabelled_glyph_buttons() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../umber-app/src");
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        assert!(!files.is_empty(), "the census must cover real app sources");
        let constructors = [
            ".small_button(",
            "Button::new(",
            ".selectable_label(",
            ".button(",
        ];
        let mut offenders = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).expect("app source must be readable");
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if !constructors.iter().any(|call| line.contains(call)) {
                    continue;
                }
                let end = (index + 12).min(lines.len());
                let block = call_block(&lines, index);
                if looks_like_glyph_call(&block)
                    && !lines[index..end]
                        .iter()
                        .any(|line| line.contains("widget_info"))
                {
                    offenders.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "unlabelled glyph buttons: {offenders:?}"
        );
    }
}
