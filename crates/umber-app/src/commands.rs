//! The shell's one dispatch site, extracted from `UmberApp::dispatch`
//! (main.rs) in queue row 154 wave 1 (table extraction): the match moved
//! here verbatim as [`run_command`] — arms unchanged except `self.` ->
//! `app.` — and `dispatch` is now a one-line delegate. Behavior is
//! byte-identical; wave 2 keys a command registry off
//! [`toolbar::ShellAction::id`].

use crate::i18n::tr;
use crate::{
    actions, brush_cursor, display_keys, history_panel, shell_menu, toolbar, workspaces, Panel,
    UmberApp,
};

/// Runs one shell action: the availability gate, then the view toggles,
/// then the action's arm. Moved verbatim from `UmberApp::dispatch`.
pub fn run_command(app: &mut UmberApp, action: toolbar::ShellAction) {
    use toolbar::ShellAction;
    // The registry's one enabled-check (GUI plan G0): a shortcut on an
    // action that cannot run says why instead of doing nothing
    // (photocraft's disabled-reason rule). Menu rows and toolbar
    // buttons are already greyed, so only keys reach this arm.
    if let Err(reason) = actions::availability(action, app.shell_state()) {
        app.notices.post(umber_gui::NoticeLevel::Info, tr(reason));
        return;
    }
    if app.view.apply(action) {
        return;
    }
    match action {
        ShellAction::NewProject => app.new_project(),
        ShellAction::OpenMesh => app.open_mesh(),
        ShellAction::OpenProject => app.open_project(),
        ShellAction::SaveProject => app.save_project(),
        ShellAction::SaveProjectAs => app.save_project_as(),
        // One timeline (C2-7; three arms since L6): Ctrl+Z steps
        // whichever journal holds the newest applied edit — a layer op,
        // a stroke, or a graph command.
        ShellAction::Undo => {
            // The palette dispatches after the dock, where a graph edit
            // may have queued this frame: drain before choosing.
            app.drain_graph_commits();
            let _ = history_panel::unified_undo(
                &mut app.state.doc,
                app.state.paint.as_mut(),
                Some(&mut app.state.graph as &mut dyn history_panel::GraphJournal),
            );
        }
        ShellAction::Redo => {
            app.drain_graph_commits();
            let _ = history_panel::unified_redo(
                &mut app.state.doc,
                app.state.paint.as_mut(),
                Some(&mut app.state.graph as &mut dyn history_panel::GraphJournal),
            );
        }
        // Quick mask (§2's P1 row): needs a paint session; I inverts
        // only while the mask is being edited (outside it, nothing).
        ShellAction::ToggleMaskMode => {
            if let Some(paint) = app.state.paint.as_mut() {
                paint.toggle_mask_mode();
            }
        }
        ShellAction::InvertMask => {
            if let Some(paint) = app.state.paint.as_mut() {
                if paint.quick_mask().is_editing() {
                    paint.invert_mask();
                }
            }
        }
        ShellAction::SelectTool(kind) => app.brush_panel.select_tool(kind),
        // `[` / `]` (Painter's size steps): one notch of the shared
        // multiplicative step, so ten presses span ~2.6x.
        ShellAction::CommandPalette => app.palette.open(),
        ShellAction::Preferences => app.prefs_open = true,
        ShellAction::ToggleHideUi => {
            app.focus.toggle(
                shell_menu::FocusMode::HideUi,
                &mut app.dock,
                Panel::Viewport,
            );
        }
        ShellAction::ViewMaterial
        | ShellAction::NextChannel
        | ShellAction::PrevChannel
        | ShellAction::NextMeshMap
        | ShellAction::PrevMeshMap => {
            let mode = app.view.view_mode;
            app.view.view_mode = match action {
                ShellAction::NextChannel => display_keys::cycle_channel(mode, true),
                ShellAction::PrevChannel => display_keys::cycle_channel(mode, false),
                ShellAction::NextMeshMap => display_keys::cycle_mesh_map(mode, true),
                ShellAction::PrevMeshMap => display_keys::cycle_mesh_map(mode, false),
                _ => umber_gpu::ViewMode::Lit,
            };
            // The soloed name, so the user sees what the viewport shows.
            app.notices.post(
                umber_gui::NoticeLevel::Info,
                display_keys::mode_name(app.view.view_mode, tr),
            );
        }
        ShellAction::NextWorkspace | ShellAction::PrevWorkspace => {
            let to = app
                .workspaces
                .active()
                .cycle(action == ShellAction::NextWorkspace);
            app.switch_workspace(to);
        }
        ShellAction::ViewportPresetBoth
        | ShellAction::ViewportPreset3d
        | ShellAction::ViewportPreset2d
        | ShellAction::ViewportPresetSwap => {
            // A preset is a layout edit on the active workspace: leave
            // any focus mode first so the real layout is what changes
            // (the switch_workspace rule), then apply and persist.
            let preset = match action {
                ShellAction::ViewportPresetBoth => workspaces::ViewportPreset::Both,
                ShellAction::ViewportPreset3d => workspaces::ViewportPreset::View3d,
                ShellAction::ViewportPreset2d => workspaces::ViewportPreset::View2d,
                _ => workspaces::ViewportPreset::Swap,
            };
            app.focus.restore(&mut app.dock);
            workspaces::apply_viewport_preset(&mut app.dock, preset);
            app.save_layouts();
        }
        ShellAction::ToggleMaximize => {
            let keep = shell_menu::maximize_target(&mut app.dock);
            app.focus
                .toggle(shell_menu::FocusMode::Maximized, &mut app.dock, keep);
        }
        ShellAction::BrushSmaller | ShellAction::BrushLarger => {
            if let Some(paint) = app.state.paint.as_mut() {
                let grow = action == ShellAction::BrushLarger;
                let next = brush_cursor::step_size(paint.brush_size(), grow);
                paint.set_brush_size(next);
            }
        }
        // Applied by `ViewToggles::apply` above.
        ShellAction::ToggleWireframe | ShellAction::ToggleGrid | ShellAction::CycleViewMode => {}
    }
}
