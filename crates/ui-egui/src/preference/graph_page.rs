//! Preferences ▸ Graph: how the graph canvases respond and what opening a graph shows

use egui::Ui;

use super::heading;
use crate::MaterialCraftApp;
use crate::i18n::tr;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let prefs = &mut app.ui.prefs;

    heading(ui, "Common");
    ui.checkbox(&mut prefs.tab_opens_node_search, tr("Tab key shows the node search"));
    ui.checkbox(&mut prefs.wire_drop_opens_node_search, tr("Create nodes by dropping a wire on empty canvas"));
    ui.checkbox(&mut prefs.view_outputs_on_open, tr("View outputs in 3D view when opening a graph"));

    heading(ui, "Material graph");
    ui.checkbox(&mut prefs.thumbnails, tr("Show node thumbnails"));
    ui.checkbox(&mut prefs.snap_to_grid, tr("Snap to grid"));
}
