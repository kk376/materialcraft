//! Help ▸ Home screen, also shown at startup: start a graph from a template, open a package or a
//! recent one, and find the tutorials, documentation and community

use egui::{RichText, Ui, vec2};
use egui_phosphor::regular as icon;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::shell::modality::Modality;
use crate::template_card::{CARD_SIZE, TemplateCard};
use crate::theme::Tokens;

const WINDOW_SIZE: egui::Vec2 = vec2(1000.0, 620.0);
const RAIL_WIDTH: f32 = 220.0;
const CARD_GAP: f32 = 14.0;
const RAIL_PADDING: f32 = 12.0;
const STARTUP_BAR_HEIGHT: f32 = 32.0;
/// The links down the left side, under their headings: (heading, [(label, Help menu id)])
const LINKS: [(&str, &[(&str, &str)]); 2] = [
    ("Learn", &[("Tutorials", "ui.help.tutorials"), ("Documentation", "ui.help.docs"), ("What's new", "ui.help.news")]),
    ("Community", &[("Community (Discord)", "ui.help.discord"), ("Report a bug", "ui.help.bug"), ("All Craft apps", "ui.help.apps")]),
];

pub(crate) fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let size = crate::windows::dialog_size(ctx, WINDOW_SIZE);
    let mut open = true;
    crate::windows::modal(ctx, "home", tr("Home"), &mut open, size, |ui| {
        ui.set_min_size(size);
        let whole = ui.available_rect_before_wrap();
        let rail = egui::Rect::from_min_max(whole.min, egui::pos2(whole.left() + RAIL_WIDTH.min(size.x * 0.4), whole.bottom()));
        let content = egui::Rect::from_min_max(egui::pos2(rail.right() + 24.0, whole.top()), whole.max);

        let mut rail_ui = ui.new_child(egui::UiBuilder::new().max_rect(rail.with_max_y(rail.bottom() - STARTUP_BAR_HEIGHT)));
        egui::ScrollArea::vertical().id_salt("home-links").auto_shrink([false, false]).show(&mut rail_ui, |ui| left_rail(app, ui));
        let checkbox = rail.with_min_y(rail.bottom() - STARTUP_BAR_HEIGHT).with_min_x(rail.left() + RAIL_PADDING);
        ui.new_child(egui::UiBuilder::new().max_rect(checkbox)).checkbox(&mut app.ui.prefs.show_home, tr("Show at startup"));

        let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(content));
        egui::ScrollArea::vertical().id_salt("home-content").auto_shrink([false, false]).show(&mut content_ui, |ui| start(app, ui));

        ui.advance_cursor_after_rect(whole);
    });
    if !open {
        app.ui.windows.home = false;
    }
}

/// The app's name, the two ways to begin, the links and the startup switch
fn left_rail(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let modality = Modality::capture(app, ui.ctx());
    ui.painter().rect_filled(ui.max_rect(), 6.0, tokens.row);
    ui.add_space(RAIL_PADDING);

    ui.horizontal(|ui| {
        ui.add_space(RAIL_PADDING);
        ui.vertical(|ui| {
            ui.label(RichText::new("MaterialCraft").size(22.0).strong().color(tokens.accent));
            ui.label(RichText::new(format!("{} {}", tr("Version"), materialcraft_engine::VERSION)).small().color(tokens.text_dim));
            ui.add_space(16.0);

            let width = (ui.available_width() - RAIL_PADDING).max(0.0);
            let new_graph = crate::widgets::primary_button(ui, &format!("{}  {}", icon::FILE_PLUS, tr("New graph...")));
            if ui.add_enabled_ui(!modality.is_blocked(), |ui| ui.add_sized([width, 30.0], new_graph)).inner.clicked() {
                app.ui.windows.home = false;
                crate::new_graph_dialog::open(app, materialcraft_engine::core::GraphKind::Material);
            }
            let open_package = egui::Button::new(format!("{}  {}", icon::FOLDER_OPEN, tr("Open package...")));
            if ui.add_enabled_ui(modality.allows("file.open"), |ui| ui.add_sized([width, 30.0], open_package)).inner.clicked() {
                app.ui.windows.home = false;
                crate::menus::invoke(app, ui.ctx(), "file.open");
            }

            for (heading, links) in LINKS {
                ui.add_space(18.0);
                ui.label(RichText::new(tr(heading).to_uppercase()).small().strong().color(tokens.text_dim));
                for (label, id) in links {
                    if let Some(url) = crate::help_links::url(id) {
                        ui.hyperlink_to(tr(label), url);
                    }
                }
            }
        });
    });
}

/// The welcome, the template cards and the recent packages
fn start(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let free = !Modality::capture(app, ui.ctx()).is_blocked();
    ui.label(RichText::new(tr("Welcome to MaterialCraft")).size(20.0).strong().color(tokens.text_strong));
    ui.label(
        RichText::new(tr("Build tileable materials from noises, patterns and filters, see them live in 2D and 3D, and export the maps for any engine."))
            .color(tokens.text_dim),
    );
    ui.add_space(14.0);

    ui.label(RichText::new(tr("Start from a template")).size(15.0).strong());
    ui.add_space(6.0);
    let cards = crate::template_card::cards();
    let columns = ((ui.available_width() + CARD_GAP) / (CARD_SIZE.x + CARD_GAP)).floor().max(1.0) as usize;
    for row in cards.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            if !free {
                ui.disable();
            }
            for card in row {
                let preview = crate::windows::template_preview(app, ui.ctx(), card.id);
                if crate::template_card::show(ui, card, false, preview).clicked() && free {
                    begin_with(app, card);
                }
            }
        });
        ui.add_space(CARD_GAP);
    }

    ui.add_space(6.0);
    ui.label(RichText::new(tr("Recent packages")).size(15.0).strong());
    recent(app, ui);
}

/// A card opens the New graph dialog with its template chosen
fn begin_with(app: &mut MaterialCraftApp, card: &TemplateCard) {
    app.ui.windows.home = false;
    crate::new_graph_dialog::open(app, card.kind);
    app.ui.new_graph_template = card.id.to_string();
}

/// The recently opened packages, newest first: the file's name, then where it is
fn recent(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    if app.ui.recent.is_empty() {
        ui.label(RichText::new(tr("No recent packages")).color(tokens.text_dim));
        return;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let free = !Modality::capture(app, ui.ctx()).is_blocked();
        let mut selected = None;
        for path in &app.ui.recent {
            let file = std::path::Path::new(path).file_name().map_or_else(|| std::borrow::Cow::Borrowed(path.as_str()), |name| name.to_string_lossy());
            let text = egui::text::LayoutJob::simple_singleline(format!("{}  {file}", icon::PACKAGE), egui::FontId::proportional(13.0), tokens.text);
            let row = ui.add_enabled(free, egui::Button::new(text).frame_when_inactive(false)).on_hover_text(path.as_str());
            ui.label(RichText::new(path.as_str()).small().color(tokens.text_dim));
            if row.clicked() {
                selected = Some(path.clone());
            }
        }
        if let Some(path) = selected {
            crate::windows::open_path(app, &path);
            app.ui.windows.home = false;
        }
    }
}
