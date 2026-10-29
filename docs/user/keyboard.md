# Keyboard reference

Generated from `crates/umber-app/src/actions.rs` (`BINDINGS`): do not edit by
hand. Regenerate with
`UMBER_REGEN_KEYBOARD_DOC=1 cargo test -p umber-app --bin umber keyboard_reference`.

Letter keys need no Ctrl/Cmd, so Ctrl+X and Ctrl+V are never these actions. While a
text field has focus it keeps its own keys: only Ctrl/Cmd chords and F-keys reach the
shell, and Ctrl+Z undoes the typing, not the document.

| Windows / Linux | macOS | Action | Fires |
|---|---|---|---|
| `Ctrl+Shift+Z` | `⇧⌘Z` | Redo | not while typing in a field |
| `Ctrl+Shift+P` | `⇧⌘P` | Command palette | everywhere, even while typing |
| `Ctrl+Shift+S` | `⇧⌘S` | Save project as (always asks) | everywhere, even while typing |
| `Ctrl+Z` | `⌘Z` | Undo | not while typing in a field |
| `Ctrl+S` | `⌘S` | Save project (asks for a folder the first time) | everywhere, even while typing |
| `Ctrl+O` | `⌘O` | Open project | everywhere, even while typing |
| `Ctrl+,` | `⌘,` | Preferences | everywhere, even while typing |
| `Ctrl+Space` | `⌘Space` | Maximize the hovered area | everywhere, even while typing |
| `Ctrl+PageDown` | `⌘PageDown` | Next workspace | everywhere, even while typing |
| `Ctrl+PageUp` | `⌘PageUp` | Previous workspace | everywhere, even while typing |
| `Shift+C` | `⇧C` | Previous channel view | not while typing in a field |
| `Shift+B` | `⇧B` | Previous mesh-map view | not while typing in a field |
| `W` | `W` | Toggle wireframe | not while typing in a field |
| `G` | `G` | Toggle grid | not while typing in a field |
| `V` | `V` | Cycle lit / unlit view | not while typing in a field |
| `X` | `X` | Toggle mask painting | not while typing in a field |
| `I` | `I` | Invert the mask | not while typing in a field |
| `E` | `E` | Eraser tool | not while typing in a field |
| `[` | `[` | Brush smaller | not while typing in a field |
| `]` | `]` | Brush larger | not while typing in a field |
| `F3` | `F3` | Command palette | everywhere, even while typing |
| `F1` | `F1` | Viewport preset: 3D + 2D both docked | everywhere, even while typing |
| `F2` | `F2` | Viewport preset: 3D viewport only | everywhere, even while typing |
| `F4` | `F4` | Viewport preset: 2D UV view only | everywhere, even while typing |
| `F5` | `F5` | Swap the 3D and 2D views | everywhere, even while typing |
| `Tab` | `Tab` | Hide / show the UI | nothing focused |
| `M` | `M` | Material view | not while typing in a field |
| `C` | `C` | Next channel view | not while typing in a field |
| `B` | `B` | Next mesh-map view | not while typing in a field |
