//! Tools ▸ Plugin Manager: the window only, for now. MaterialCraft loads no plugins yet, so the list
//! is empty and its buttons wait for the plugin system

use egui::{RichText, vec2};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const WINDOW_SIZE: egui::Vec2 = vec2(700.0, 460.0);
const LOADED_WIDTH: f32 = 110.0;
const BUTTON_WIDTH: f32 = 130.0;

pub(crate) fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut open = true;
    crate::windows::modal(ctx, "plugin-manager", tr("Plugin Manager"), &mut open, WINDOW_SIZE, |ui| {
        let tokens = Tokens::get(ui.ctx());
        ui.set_min_size(WINDOW_SIZE);
        let filter = ui.add(egui::TextEdit::singleline(&mut app.ui.plugin_filter).hint_text(tr("Filter")).desired_width(f32::INFINITY));
        crate::widgets::name(&filter, egui::Role::SearchInput, "Filter");
        ui.add_space(6.0);

        // The table's header row, then its rows
        let header = ui.available_rect_before_wrap().with_max_y(ui.cursor().top() + 24.0);
        ui.painter().rect_filled(header, 0.0, tokens.row_hover);
        ui.horizontal(|ui| {
            ui.set_height(24.0);
            ui.add_space(6.0);
            let left = egui::Layout::left_to_right(egui::Align::Center);
            let plugins_width = ui.available_width() - LOADED_WIDTH;
            ui.allocate_ui_with_layout(vec2(plugins_width, 24.0), left, |ui| {
                ui.set_min_width(plugins_width);
                ui.label(RichText::new(tr("Plugins")).strong());
            });
            ui.label(RichText::new(tr("Loaded")).strong());
        });

        let list = ui.available_rect_before_wrap().with_max_y(ui.max_rect().bottom() - 44.0);
        ui.painter().rect_filled(list, 0.0, tokens.row);
        ui.new_child(egui::UiBuilder::new().max_rect(list.shrink(10.0)))
            .label(RichText::new(tr("No plugins are installed. MaterialCraft does not load plugins yet.")).color(tokens.text_dim));
        ui.advance_cursor_after_rect(list);
        ui.add_space(8.0);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let waiting = tr("Plugins are not supported yet");
            for label in ["Refresh", "Install...", "Browse..."] {
                ui.add_enabled(false, egui::Button::new(tr(label)).min_size(vec2(BUTTON_WIDTH, 28.0))).on_disabled_hover_text(waiting);
            }
        });
    });
    if !open {
        app.ui.windows.plugin_manager = false;
    }
}
