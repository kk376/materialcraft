//! Preferences ▸ General: recent packages, history, language, appearance, views, autosave and previews

use egui::Ui;

use super::{heading, number_row};
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::state::Prefs;
use crate::theme::{Accent, Brightness};

/// The most undo steps the history slider reaches
const MAX_UNDO_STEPS: f32 = 10_000.0;
#[cfg(not(target_arch = "wasm32"))]
/// The longest autosave interval the slider reaches, in minutes
const MAX_AUTOSAVE_MINUTES: f32 = 60.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let prefs = &mut app.ui.prefs;

    heading(ui, "Recent documents");
    number_row(ui, "Recent packages list contains", &mut prefs.recent_limit, 1..=Prefs::MAX_RECENT, "packages");

    heading(ui, "History");
    let mut undo_limit = prefs.history.undo_limit as f32;
    if crate::widgets::slider(ui, tr("Undo history (steps)"), &mut undo_limit, 1.0..=MAX_UNDO_STEPS, 0).changed() {
        prefs.history.undo_limit = undo_limit.round() as u32;
    }

    heading(ui, "Language");
    crate::widgets::combo(ui, tr("Language"), crate::i18n::current().name, |ui| {
        for language in &crate::i18n::LANGUAGES {
            if ui.selectable_label(prefs.language == language.code, language.name).clicked() {
                prefs.language = language.code.to_owned();
                crate::i18n::set_language(&prefs.language);
            }
        }
    });

    heading(ui, "Appearance");
    crate::widgets::combo(ui, tr("Brightness"), tr(prefs.brightness.label()), |ui| {
        for brightness in Brightness::ALL {
            ui.selectable_value(&mut prefs.brightness, brightness, tr(brightness.label()));
        }
    });
    crate::widgets::combo(ui, tr("Accent color"), tr(prefs.accent.label()), |ui| {
        for accent in Accent::ALL {
            ui.selectable_value(&mut prefs.accent, accent, tr(accent.label()));
        }
    });
    crate::widgets::slider(ui, tr("Interface scale"), &mut prefs.ui_scale, 0.75..=2.0, 2);

    heading(ui, "Views");
    ui.checkbox(&mut prefs.invert_zoom, tr("Invert zoom in views"));

    #[cfg(not(target_arch = "wasm32"))]
    {
        heading(ui, "Auto backup");
        let mut minutes = prefs.autosave_minutes as f32;
        if crate::widgets::slider(ui, tr("Autosave every (minutes, 0 = off)"), &mut minutes, 0.0..=MAX_AUTOSAVE_MINUTES, 0).changed() {
            prefs.autosave_minutes = minutes.round() as u32;
        }

        let mut versions = prefs.autosave_versions as usize;
        if number_row(ui, "Keep up to", &mut versions, 1..=Prefs::MAX_AUTOSAVE_VERSIONS as usize, "versions") {
            prefs.autosave_versions = versions as u32;
        }

        ui.label(
            egui::RichText::new(format!("{} {}", tr("The copies are numbered files in"), crate::user_dir("autosave").display()))
                .small()
                .color(crate::theme::Tokens::get(ui.ctx()).text_dim),
        );
    }

    heading(ui, "Cooker");
    crate::widgets::combo(ui, tr("Preview size while editing"), crate::preview_resolution::label(prefs.preview_size), |ui| {
        crate::preview_resolution::choices(ui, &mut prefs.preview_size);
    });
}
