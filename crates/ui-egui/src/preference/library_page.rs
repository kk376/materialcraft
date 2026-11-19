//! Preferences ▸ Library: the library's previews, their cache and its layout

use egui::Ui;

use super::heading;
use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Width of the Rebuild thumbnails button
const REBUILD_WIDTH: f32 = 360.0;

/// Returns true when the stored previews should be thrown away, which happens after the window drew
pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) -> bool {
    let prefs = &mut app.ui.prefs;
    let rebuild = ui.add_sized([ui.available_width().min(REBUILD_WIDTH), 24.0], egui::Button::new(tr("Rebuild thumbnails"))).clicked();
    ui.label(tr("Previews rebuild as they come into view."));

    heading(ui, "Library preview cache");
    #[cfg(not(target_arch = "wasm32"))]
    {
        use materialcraft_engine::LibraryCachePrefs;

        ui.horizontal(|ui| {
            let label = ui.label(tr("Disk limit (MiB, 0 = off)"));
            ui.add(egui::DragValue::new(&mut prefs.library_cache.disk_mib).range(0..=LibraryCachePrefs::MAX_DISK_MIB)).labelled_by(label.id);
        });
        ui.horizontal(|ui| {
            let label = ui.label(tr("Maximum stored previews"));
            ui.add(egui::DragValue::new(&mut prefs.library_cache.max_previews).range(1..=LibraryCachePrefs::MAX_PREVIEWS)).labelled_by(label.id);
        });
    }

    heading(ui, "Layout");
    ui.checkbox(&mut prefs.library_list, tr("Show the library as a list"));
    rebuild
}
