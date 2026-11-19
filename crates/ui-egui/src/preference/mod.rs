//! Edit ▸ Preferences: pages listed down the left side, and OK, Cancel and Apply. Changes show as
//! they are made; Cancel (or closing the window) goes back to the preferences as they were when
//! the window opened or Apply was last pressed

mod general_page;
mod graph_page;
mod library_page;
mod preference_action;
mod preferences_page;
mod preferences_state;
mod projects_page;
mod shortcuts_page;
mod view3d_page;

use egui::{RichText, Ui, vec2};
use preference_action::PreferenceAction;

pub use preferences_page::PreferencesPage;
pub use preferences_state::PreferencesState;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const WINDOW_SIZE: egui::Vec2 = vec2(900.0, 620.0);
const PAGE_LIST_WIDTH: f32 = 150.0;
const PAGE_ROW_HEIGHT: f32 = 36.0;
const BUTTON_WIDTH: f32 = 110.0;
const BUTTON_BAR_HEIGHT: f32 = 40.0;

/// A section heading of a page, in capitals like the reference layout
pub(super) fn heading(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(tr(title).to_uppercase()).size(13.0).strong().color(Tokens::get(ui.ctx()).text_strong));
    ui.add_space(2.0);
}

/// A labelled whole-number field with a unit after it. Returns true when it changed
pub(super) fn number_row<T: egui::emath::Numeric>(ui: &mut Ui, label: &str, value: &mut T, range: std::ops::RangeInclusive<T>, unit: &str) -> bool {
    ui.horizontal(|ui| {
        let caption = ui.label(tr(label));
        let changed = ui.add(egui::DragValue::new(value).range(range).speed(0.2)).labelled_by(caption.id).changed();
        ui.label(tr(unit));
        changed
    })
    .inner
}

pub fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    if app.ui.preferences.applied.is_none() {
        app.ui.preferences.applied = Some(app.ui.prefs.clone());
    }

    let size = crate::windows::dialog_size(ctx, WINDOW_SIZE);
    let mut open = true;
    let mut choice = None;
    let mut rebuild_previews = false;
    crate::windows::modal(ctx, "prefs", tr("Preferences"), &mut open, size, |ui| {
        ui.set_min_size(size);
        let whole = ui.available_rect_before_wrap();
        let bar_top = whole.bottom() - BUTTON_BAR_HEIGHT;
        let list = egui::Rect::from_min_max(whole.min, egui::pos2(whole.left() + PAGE_LIST_WIDTH.min(size.x * 0.35), bar_top));
        let page = egui::Rect::from_min_max(egui::pos2(list.right() + 16.0, whole.top()), egui::pos2(whole.right(), bar_top));

        page_list(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(list)));

        let mut page_ui = ui.new_child(egui::UiBuilder::new().max_rect(page));
        egui::ScrollArea::both().id_salt("preferences-page").auto_shrink([false, false]).show(&mut page_ui, |ui| {
            ui.set_min_width((page.width() - 12.0).max(0.0));
            match app.ui.preferences.page {
                PreferencesPage::General => general_page::show(app, ui),
                PreferencesPage::Projects => projects_page::show(app, ui),
                PreferencesPage::Graph => graph_page::show(app, ui),
                PreferencesPage::View3d => view3d_page::show(app, ui),
                PreferencesPage::Library => rebuild_previews = library_page::show(app, ui),
                PreferencesPage::Shortcuts => shortcuts_page::show(app, ui),
            }
        });

        let bar = egui::Rect::from_min_max(egui::pos2(whole.left(), bar_top), whole.max);
        let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::right_to_left(egui::Align::Center)));
        for (label, pressed) in [("Apply", PreferenceAction::Apply), ("Cancel", PreferenceAction::Cancel), ("OK", PreferenceAction::Ok)] {
            let button = if pressed == PreferenceAction::Ok { crate::widgets::primary_button(&bar_ui, tr(label)) } else { egui::Button::new(tr(label)) };
            if bar_ui.add_sized([BUTTON_WIDTH.min(bar.width() / 3.0 - 4.0), 28.0], button).clicked() {
                choice = Some(pressed);
            }
        }

        ui.advance_cursor_after_rect(whole);
    });

    if rebuild_previews {
        app.run("library.cache.clear", serde_json::json!({}));
    }

    // Closing the window is Cancel
    let choice = if open { choice } else { Some(PreferenceAction::Cancel) };
    match choice {
        Some(PreferenceAction::Apply) => app.ui.preferences.applied = Some(app.ui.prefs.clone()),
        Some(PreferenceAction::Cancel) => {
            if let Some(applied) = app.ui.preferences.applied.take() {
                app.ui.prefs = applied;
            }
            close(app);
        }
        Some(PreferenceAction::Ok) => close(app),
        None => {}
    }
    app.apply_prefs();
}

fn close(app: &mut MaterialCraftApp) {
    app.ui.windows.preferences = false;
    app.ui.preferences.applied = None;
    app.ui.preferences.capturing = None;
}

/// The pages down the left side; the chosen one is highlighted
fn page_list(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    ui.painter().rect_filled(ui.max_rect(), 0.0, tokens.row);

    for page in PreferencesPage::ALL {
        let chosen = app.ui.preferences.page == page;
        let label = RichText::new(tr(page.label()).to_uppercase()).size(14.0).color(if chosen { tokens.text_strong } else { tokens.text });
        let button = egui::Button::selectable(chosen, label).frame_when_inactive(false);
        let row = ui
            .allocate_ui_with_layout(
                vec2(ui.available_width(), PAGE_ROW_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true).with_main_align(egui::Align::LEFT),
                |ui| ui.add(button),
            )
            .inner;
        crate::widgets::name(&row, egui::Role::Tab, page.label());
        if row.clicked() {
            app.ui.preferences.page = page;
            app.ui.preferences.capturing = None;
        }
    }
}
