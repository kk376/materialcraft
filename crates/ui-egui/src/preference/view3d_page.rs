//! Preferences ▸ 3D View: what a new 3D view shows and how its camera moves

use egui::Ui;
use materialcraft_engine::geom::view::Background;

use super::heading;
use crate::MaterialCraftApp;
use crate::i18n::tr;

/// The renderer's rays per pixel side, with the names the sample count list shows
const SAMPLE_COUNTS: [(u32, &str); 4] = [(1, "Low (1 spp)"), (2, "Medium (4 spp)"), (3, "9 spp"), (4, "High (16 spp)")];
/// The furthest the 3D camera's target may move from the mesh, in mesh radii
const MAX_PAN_LIMIT: f32 = 20.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let prefs = &mut app.ui.prefs;
    let mut view = prefs.default_view3d.clone().unwrap_or_default();

    heading(ui, "Misc");
    let mut environment_shown = view.background == Background::Environment;
    let background_changed = ui.checkbox(&mut environment_shown, tr("Environment shown behind the mesh by default")).changed();
    if background_changed {
        view.background = if environment_shown { Background::Environment } else { Background::default() };
    }
    let current = SAMPLE_COUNTS.iter().find(|(samples, _)| *samples == view.samples).map_or("", |(_, label)| label);
    let original_samples = view.samples;
    crate::widgets::combo(ui, tr("Sample count"), tr(current), |ui| {
        for (samples, label) in SAMPLE_COUNTS {
            ui.selectable_value(&mut view.samples, samples, tr(label));
        }
    });
    if background_changed || view.samples != original_samples {
        prefs.default_view3d = Some(view);
    }

    heading(ui, "Camera");
    crate::widgets::slider(ui, tr("3D pan limit (mesh radii)"), &mut prefs.camera.pan_limit, 0.0..=MAX_PAN_LIMIT, 1);
    crate::widgets::slider(ui, tr("3D pan speed"), &mut prefs.camera.pan_speed, 0.1..=4.0, 2);
}
