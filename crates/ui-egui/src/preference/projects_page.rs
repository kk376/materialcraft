//! Preferences ▸ Projects: the 3D view a new package and a reset start from

use egui::Ui;
use materialcraft_engine::view_cmds::ENVIRONMENTS;

use super::heading;
use crate::MaterialCraftApp;
use crate::i18n::tr;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = crate::theme::Tokens::get(ui.ctx());
    let prefs = &mut app.ui.prefs;
    let mut view = prefs.default_view3d.clone().unwrap_or_default();

    heading(ui, "Default 3D view");
    let saved = if prefs.default_view3d.is_some() { tr("A saved default is in use") } else { tr("The built-in default is in use") };
    ui.label(egui::RichText::new(saved).color(tokens.text_dim));
    ui.label(egui::RichText::new(tr("Save Current State As Default, in the 3D view's Scene menu, saves the whole view here")).small().color(tokens.text_dim));

    let current = ENVIRONMENTS.iter().find(|(id, _)| *id == view.environment).map_or(view.environment.clone(), |(_, label)| tr(label).to_string());
    let mut changed = false;
    crate::widgets::combo(ui, tr("Default environment"), current, |ui| {
        for (id, label) in ENVIRONMENTS {
            if ui.selectable_label(view.environment == *id, tr(label)).clicked() && view.environment != *id {
                view.environment = (*id).to_owned();
                changed = true;
            }
        }
    });

    heading(ui, "Default lighting state");
    changed |= ui.checkbox(&mut view.ambient.enabled, tr("Ambient Light")).changed();
    changed |= ui.checkbox(&mut view.lights[0].enabled, tr("Point Light 1")).changed();
    changed |= ui.checkbox(&mut view.lights[1].enabled, tr("Point Light 2")).changed();

    // Only an actual change makes a saved default out of the built-in one
    if changed {
        prefs.default_view3d = Some(view);
    }

    ui.add_space(8.0);
    if ui.add_enabled(prefs.default_view3d.is_some(), egui::Button::new(tr("Use the built-in default"))).clicked() {
        prefs.default_view3d = None;
    }
}
