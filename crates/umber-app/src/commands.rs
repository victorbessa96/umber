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
            // A whole-layer filter apply (the Assets panel's Filters
            // category) is one undo step on the shell's own plane-op
            // stack; when its history row is the newest, it steps first.
            let newest = app.state.doc.session_history.entries().last().cloned();
            if app.state.filter_undos.newest_is_ours(newest.as_ref()) {
                if let Some(paint) = app.state.paint.as_mut() {
                    app.state.filter_undos.undo(paint);
                }
                app.state.doc.session_history.pop_last();
            } else {
                let _ = history_panel::unified_undo(
                    &mut app.state.doc,
                    app.state.paint.as_mut(),
                    Some(&mut app.state.graph as &mut dyn history_panel::GraphJournal),
                );
            }
        }
        ShellAction::Redo => {
            app.drain_graph_commits();
            // The filter stack's redo twin: when the undone filter row's
            // id is exactly the next one the log would assign, the filter
            // steps first; otherwise the journals' unified redo runs.
            let next = app.state.doc.session_history.next_op_id();
            if app.state.filter_undos.redo_is_next(next) {
                app.state
                    .doc
                    .session_history
                    .record_context(history_panel::HistoryKind::FilterApply);
                let id = app
                    .state
                    .doc
                    .session_history
                    .entries()
                    .last()
                    .map_or(next, |e| e.op_id);
                if let Some(paint) = app.state.paint.as_mut() {
                    app.state.filter_undos.redo(paint, id);
                }
            } else {
                let _ = history_panel::unified_redo(
                    &mut app.state.doc,
                    app.state.paint.as_mut(),
                    Some(&mut app.state.graph as &mut dyn history_panel::GraphJournal),
                );
            }
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
        // The brush popup opens at the pointer (or the window centre
        // when the pointer is outside), toggling like Blender's.
        ShellAction::BrushPopup => app.toggle_brush_popup(),
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
        // U6 A2-4: the Layers panel's creation row, by id. Same command
        // and the same add-then-select rule as the panel's "+" row.
        ShellAction::AddPaintLayer | ShellAction::AddFillLayer | ShellAction::AddFolderLayer => {
            let kind = match action {
                ShellAction::AddPaintLayer => crate::document::NewLayer::Paint,
                ShellAction::AddFillLayer => crate::document::NewLayer::Fill,
                _ => crate::document::NewLayer::Folder,
            };
            let doc = &mut app.state.doc;
            let n = doc.layers().len() + 1;
            doc.run(umber_core::layers::LayerCommand::add(
                kind.default_name(n),
                kind.kind(),
            ));
            if let Some(top) = doc.layers().last() {
                let target = umber_paint::paint_state::PlaneOwner::Layer(top.id);
                if let Some(paint) = app.state.paint.as_mut() {
                    paint.set_active_target(target);
                }
                crate::sync_gesture_selection(&mut app.state.layer_gestures, target);
            }
        }
        ShellAction::ShowLayers => {
            app.focus.restore(&mut app.dock);
            app.rails.unfold_holding(&mut app.dock, &Panel::LayerStack);
            shell_menu::show_panel(&mut app.dock, &Panel::LayerStack);
            app.save_layouts();
        }
        ShellAction::ShowTextureSets => {
            app.focus.restore(&mut app.dock);
            app.rails.unfold_holding(&mut app.dock, &Panel::TextureSets);
            shell_menu::show_panel(&mut app.dock, &Panel::TextureSets);
            app.save_layouts();
        }
        ShellAction::ShowAssets => {
            app.focus.restore(&mut app.dock);
            app.rails.unfold_holding(&mut app.dock, &Panel::Assets);
            shell_menu::show_panel(&mut app.dock, &Panel::Assets);
            app.save_layouts();
        }
        ShellAction::ShowBakes => {
            app.focus.restore(&mut app.dock);
            app.rails.unfold_holding(&mut app.dock, &Panel::Bakes);
            shell_menu::show_panel(&mut app.dock, &Panel::Bakes);
            app.save_layouts();
        }
        ShellAction::ShowExport => {
            app.focus.restore(&mut app.dock);
            app.rails.unfold_holding(&mut app.dock, &Panel::Export);
            shell_menu::show_panel(&mut app.dock, &Panel::Export);
            app.save_layouts();
        }
        ShellAction::ShowHistory => {
            app.focus.restore(&mut app.dock);
            app.rails.unfold_holding(&mut app.dock, &Panel::History);
            shell_menu::show_panel(&mut app.dock, &Panel::History);
            app.save_layouts();
        }
        ShellAction::RunBake => {
            if let Err(why) = run_bake(app) {
                app.notices.post(umber_gui::NoticeLevel::Info, tr(why));
            }
        }
        ShellAction::RunExport => {
            if let Err(why) = run_export(app) {
                app.notices.post(umber_gui::NoticeLevel::Info, tr(why));
            }
        }
        ShellAction::FoldLeft => app.toggle_fold(crate::dock_rail::Side::Left),
        ShellAction::FoldRight => app.toggle_fold(crate::dock_rail::Side::Right),
        // Applied by `ViewToggles::apply` above.
        ShellAction::ToggleWireframe | ShellAction::ToggleGrid | ShellAction::CycleViewMode => {}
    }
}

/// `bake.run` (A2-4 tail): the Bakes panel's Bake button with its current
/// settings, through the panel's own `bake_current` -> `bake_now` (the
/// one bake path; no second implementation). Starts a background job;
/// `Err` is the i18n key of the reason the button would be greyed.
fn run_bake(app: &mut UmberApp) -> Result<(), &'static str> {
    let Some(mesh) = app.state.mesh.as_ref() else {
        return Err("action.unavailable-no-mesh");
    };
    let mesh_path = app.state.mesh_path.as_deref();
    app.state
        .bakes
        .bake_current(&app.gpu.device, &app.gpu.queue, mesh, mesh_path)
}

/// `export.run` (A2-4 tail): the Export panel's Export button with its
/// current settings, through the dialog's own `export_current` ->
/// `export_now`, fed the same [`ExportContext`] the drawn panel builds.
fn run_export(app: &mut UmberApp) -> Result<(), &'static str> {
    if let Some(paint) = app.state.paint.as_mut() {
        paint.flush_for_export();
    }
    let ctx = crate::export_dialog::ExportContext {
        gpu: Some(&app.gpu),
        mesh: app.state.mesh.as_ref(),
        mesh_path: app.state.mesh_path.as_deref(),
        paint: app.state.paint.as_ref(),
        doc: Some(&app.state.doc),
        graph: Some(&app.state.graph),
        baked_normal: app.state.bakes.baked_normal(),
    };
    app.state.export.export_current(&ctx)
}

/// Typed failure for dispatch-by-id (queue row 154, wave 2a): the agent/MCP
/// surface reports these, never panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// The id is not in the registry.
    UnknownCommand(String),
    /// The command exists but carries a runtime payload (`tool.select`
    /// needs a `ToolKind`); id-only dispatch cannot express it yet.
    NeedsParams(String),
    /// The availability guard refused; the `&'static str` is the i18n key
    /// `actions::availability` returned.
    Disabled(&'static str),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::UnknownCommand(id) => write!(f, "unknown command: {id}"),
            CommandError::NeedsParams(id) => write!(f, "command needs params: {id}"),
            CommandError::Disabled(why) => write!(f, "command disabled: {why}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Every registered command id, dotted and stable (wave 2 registry surface;
/// `command.list` will serialize this with labels/menus/shortcuts).
#[cfg_attr(not(test), allow(dead_code))]
pub const COMMAND_IDS: &[&str] = &[
    "file.new_project",
    "file.open_mesh",
    "file.open_project",
    "file.save_project",
    "file.save_project_as",
    "edit.undo",
    "edit.redo",
    "view.toggle_wireframe",
    "view.toggle_grid",
    "view.cycle_view_mode",
    "view.view_material",
    "view.next_channel",
    "view.prev_channel",
    "view.next_mesh_map",
    "view.prev_mesh_map",
    "view.viewport_preset_both",
    "view.viewport_preset_3d",
    "view.viewport_preset_2d",
    "view.viewport_preset_swap",
    "paint.toggle_mask_mode",
    "paint.invert_mask",
    "tool.brush_smaller",
    "tool.brush_larger",
    "tool.brush_popup",
    "tool.select",
    "window.toggle_hide_ui",
    "window.toggle_maximize",
    "window.command_palette",
    "window.preferences",
    "window.next_workspace",
    "window.prev_workspace",
    "layer.add_paint",
    "layer.add_fill",
    "layer.add_folder",
    "window.show_layers",
    "window.show_texture_sets",
    "window.show_assets",
    "window.show_bakes",
    "window.show_export",
    "window.show_history",
    "bake.run",
    "export.run",
    "window.fold_left",
    "window.fold_right",
];

/// Dispatch by dotted id — the path menus, shortcuts, the agent protocol
/// and the MCP tools all converge on (wave 2+). Payload-less commands run
/// through the exact same [`run_command`] as a menu click.
pub fn run_command_id(app: &mut UmberApp, id: &str) -> Result<(), CommandError> {
    let Some(action) = toolbar::ShellAction::from_id(id) else {
        if id == "tool.select" {
            return Err(CommandError::NeedsParams(id.to_string()));
        }
        return Err(CommandError::UnknownCommand(id.to_string()));
    };
    if let Err(why) = actions::availability(action, app.shell_state()) {
        return Err(CommandError::Disabled(why));
    }
    // The bake/export commands carry panel-level gates (no map checked,
    // no tile selected) the shell registry cannot see: a remote caller
    // gets them as a typed refusal instead of a toast it never reads.
    match action {
        toolbar::ShellAction::RunBake => return run_bake(app).map_err(CommandError::Disabled),
        toolbar::ShellAction::RunExport => return run_export(app).map_err(CommandError::Disabled),
        _ => {}
    }
    run_command(app, action);
    Ok(())
}

/// The `job.status` wire payload (A2-4 tail): the bake and export
/// results a control client waits on after `bake.run` / `export.run`.
/// `completed` counts landed jobs, so a client reads it first and waits
/// for it to grow. Read-only.
pub fn job_status_json(app: &UmberApp) -> serde_json::Value {
    let bakes = &app.state.bakes;
    let export = &app.state.export;
    // Absolute: the panels write under the app's working directory, which
    // a remote client cannot know.
    let abs = |p: &std::path::Path| {
        std::path::absolute(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .display()
            .to_string()
    };
    let outputs: Vec<String> = bakes.last_records().iter().map(|r| abs(&r.path)).collect();
    let written: Vec<String> = export.last_written().iter().map(|p| abs(p)).collect();
    serde_json::json!({
        "bake": {
            "running": bakes.is_baking(),
            "completed": bakes.completed_jobs(),
            "status": bakes.status(),
            "outputs": outputs,
        },
        "export": {
            "completed": export.completed_exports(),
            "status": export.status(),
            "written": written,
        },
    })
}

/// The `command.list` wire payload (control channel, row 154 wave 2c): one
/// row per registered id with the live availability verdict. Labels/shortcuts
/// join in wave 2b when the i18n `cmd.*` namespace lands.
fn action_label_key(id: &str) -> &'static str {
    toolbar::ShellAction::from_id(id)
        .map(|a| a.label_key())
        .unwrap_or("cmd-tool-select")
}

pub fn command_list_json(app: &UmberApp) -> serde_json::Value {
    let state = app.shell_state();
    let rows: Vec<serde_json::Value> = COMMAND_IDS
        .iter()
        .map(|id| {
            let (enabled, reason) = if *id == "tool.select" {
                // Payload command: enabled iff a mesh/paint surface exists.
                (state.paint_available, None)
            } else {
                match toolbar::ShellAction::from_id(id) {
                    Some(action) => match actions::availability(action, state) {
                        Ok(()) => (true, None),
                        Err(why) => (false, Some(why)),
                    },
                    None => (false, None),
                }
            };
            serde_json::json!({
                "id": id,
                "label_key": action_label_key(id),
                "label": tr(action_label_key(id)),
                "enabled": enabled,
                "disabled_reason": reason,
            })
        })
        .collect();
    serde_json::json!({ "commands": rows, "count": rows.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolbar::ShellAction;

    #[test]
    fn every_payloadless_id_round_trips() {
        for id in COMMAND_IDS {
            if *id == "tool.select" {
                assert!(ShellAction::from_id(id).is_none());
                assert_eq!(action_label_key(id), "cmd-tool-select");
                continue;
            }
            let action = ShellAction::from_id(id).expect("registered id must resolve");
            assert_eq!(action.id(), *id, "id() must be the inverse of from_id()");
            // label_key is the cmd.* form of the id; the catalogs carry every
            // cmd-* key in both locales (i18n parity test enforces it).
            let want = format!("cmd-{}", id.replace(['.', '_'], "-"));
            assert_eq!(action.label_key(), want);
            // The label key must resolve in the catalogs, not fall back to
            // the key itself (regression pin for the dash/underscore mismatch).
            crate::i18n::init();
            assert_ne!(
                crate::i18n::tr_en(action.label_key()),
                action.label_key(),
                "cmd key missing from en-US catalog"
            );
        }
    }

    /// U6 A2-4: the layer and panel commands are in the registry, the
    /// palette and both catalogs, so the command palette and the control
    /// channel reach them.
    #[test]
    fn layer_and_panel_commands_are_registered() {
        for id in [
            "layer.add_paint",
            "layer.add_fill",
            "layer.add_folder",
            "window.show_layers",
            "window.show_texture_sets",
            "window.show_assets",
            "window.show_bakes",
            "window.show_export",
            "window.show_history",
        ] {
            assert!(COMMAND_IDS.contains(&id), "{id} registered");
            let action = ShellAction::from_id(id).expect(id);
            assert!(
                crate::palette::CATALOG.iter().any(|e| e.action == action),
                "{id} in the palette"
            );
        }
    }

    #[test]
    fn show_panel_docks_a_closed_panel_and_activates_it() {
        let mut dock = crate::workspaces::preset(crate::workspaces::Workspace::Paint);
        crate::shell_menu::toggle_panel(&mut dock, &Panel::Bakes);
        assert!(!crate::shell_menu::is_open(&dock, &Panel::Bakes));
        crate::shell_menu::show_panel(&mut dock, &Panel::Bakes);
        let at = dock.find_tab(&Panel::Bakes).expect("docked");
        let leaf = dock.leaf(at.node_path()).unwrap();
        assert_eq!(leaf.active, at.tab, "brought forward");
        // An open but hidden tab is brought forward, not duplicated.
        let hist = dock.find_tab(&Panel::History).unwrap();
        crate::shell_menu::show_panel(&mut dock, &Panel::History);
        assert_eq!(dock.leaf(hist.node_path()).unwrap().active, hist.tab);
        let count = dock
            .iter_all_tabs()
            .filter(|(_, p)| **p == Panel::History)
            .count();
        assert_eq!(count, 1);
    }

    /// A2-4 tail: bake and export run by id, through the registry, the
    /// palette and both catalogs.
    #[test]
    fn bake_and_export_run_commands_are_registered() {
        for (id, action) in [
            ("bake.run", ShellAction::RunBake),
            ("export.run", ShellAction::RunExport),
        ] {
            assert!(COMMAND_IDS.contains(&id), "{id} registered");
            assert_eq!(ShellAction::from_id(id), Some(action));
            assert_eq!(action.id(), id);
            assert!(
                crate::palette::CATALOG.iter().any(|e| e.action == action),
                "{id} in the palette"
            );
            assert!(crate::actions::ALL_ACTIONS.contains(&action));
        }
    }

    #[test]
    fn unknown_ids_are_typed_errors() {
        assert_eq!(ShellAction::from_id("nope.not_a_command"), None);
    }
}
