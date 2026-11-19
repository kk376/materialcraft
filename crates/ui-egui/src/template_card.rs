//! The template cards of the New graph dialog and the home screen: a rendered preview, a name,
//! the kind of graph, and an information button that says what the template holds

use egui::{Color32, Sense, Ui, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::core::GraphKind;

use crate::i18n::tr;
use crate::theme::Tokens;

pub(crate) const CARD_SIZE: egui::Vec2 = vec2(140.0, 196.0);
const PREVIEW_SIDE: f32 = 96.0;

/// A template a new graph can start from
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TemplateCard {
    /// The `file.new` template id; function graphs have one template
    pub id: &'static str,
    pub label: &'static str,
    pub kind: GraphKind,
    pub description: &'static str,
}

/// The id the function graph card goes by
pub(crate) const FUNCTION_TEMPLATE: &str = "function";

/// Every card: the material templates the engine has, then the function graph
pub(crate) fn cards() -> Vec<TemplateCard> {
    let material =
        materialcraft_engine::samples::TEMPLATES.iter().map(|(id, label)| TemplateCard { id, label, kind: GraphKind::Material, description: description(id) });
    let function = TemplateCard { id: FUNCTION_TEMPLATE, label: "Function graph", kind: GraphKind::Function, description: description(FUNCTION_TEMPLATE) };
    material.chain(std::iter::once(function)).collect()
}

/// What a template holds, in a sentence
fn description(id: &str) -> &'static str {
    match id {
        "empty" => "An empty material graph",
        "pbr" => "Base color, normal, roughness, metallic, height and ambient occlusion outputs with inputs already connected",
        "atomics" => "A small graph of atomic nodes working together",
        FUNCTION_TEMPLATE => "An empty function graph: a value computed from function nodes",
        _ => "",
    }
}

impl TemplateCard {
    /// The name the kind of graph goes by under the card's title
    pub fn kind_label(&self) -> &'static str {
        if self.kind == GraphKind::Function { "Function" } else { "Material" }
    }
}

/// Draw a card; `preview` is its rendered picture once ready. Returns the card's response
pub(crate) fn show(ui: &mut Ui, card: &TemplateCard, chosen: bool, preview: Option<egui::TextureId>) -> egui::Response {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(CARD_SIZE, Sense::click());
    crate::widgets::name(&response, egui::Role::ListItem, card.label);

    let fill = if chosen || response.hovered() { tokens.row_hover } else { tokens.row };
    ui.painter().rect_filled(rect, 6.0, fill);
    if chosen || response.has_focus() {
        ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.5, tokens.accent), egui::StrokeKind::Inside);
    }

    let picture = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 18.0 + PREVIEW_SIDE * 0.5), vec2(PREVIEW_SIDE, PREVIEW_SIDE));
    match (preview, card.kind) {
        (Some(texture), _) => {
            ui.painter().image(texture, picture, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        (None, GraphKind::Function) => {
            ui.painter().text(picture.center(), egui::Align2::CENTER_CENTER, icon::FUNCTION, egui::FontId::proportional(48.0), tokens.text_dim);
        }
        (None, _) => {
            ui.painter().circle_filled(picture.center(), PREVIEW_SIDE * 0.45, tokens.separator);
        }
    }

    let title = ui.painter().layout(tr(card.label).to_string(), egui::FontId::proportional(13.0), tokens.text_strong, CARD_SIZE.x - 12.0);
    ui.painter().galley(egui::pos2(rect.center().x - title.size().x * 0.5, picture.bottom() + 14.0), title, tokens.text_strong);
    let kind = tr(card.kind_label());
    ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 18.0), egui::Align2::CENTER_CENTER, kind, egui::FontId::proportional(11.0), tokens.text_dim);

    let info = egui::Rect::from_center_size(rect.right_top() + vec2(-16.0, 16.0), vec2(18.0, 18.0));
    let info_response = ui.interact(info, response.id.with("info"), Sense::click());
    crate::widgets::name(&info_response, egui::Role::Button, tr(card.description));
    ui.painter().text(info.center(), egui::Align2::CENTER_CENTER, icon::INFO, egui::FontId::proportional(16.0), tokens.text_dim);
    if info_response.has_focus() {
        ui.painter().rect_stroke(info, 3.0, egui::Stroke::new(1.0, tokens.accent), egui::StrokeKind::Inside);
        info_response.show_tooltip_text(tr(card.description));
    }
    info_response.on_hover_text(tr(card.description));

    response
}
