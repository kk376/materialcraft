//! The outputs tree: a checkbox per graph and per output, and each output's exported color space

use egui::{RichText, Ui};
use materialcraft_engine::ColorSpace;
use materialcraft_engine::core::Graph;
use serde_json::{Value, json};

use super::{ExportSettings, output_key};
use crate::i18n::tr;
use crate::theme::Tokens;

/// Width of the outputs column, so the color space header sits over its dropdowns
const OUTPUT_COLUMN_WIDTH: f32 = 220.0;

/// One output node as the window lists it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedOutput {
    pub identifier: String,
    pub usage: &'static str,
}

/// `graph`'s output nodes in node order
pub fn listed(graph: &Graph) -> impl Iterator<Item = ListedOutput> {
    graph
        .nodes
        .iter()
        .filter(|node| node.def == "output")
        .map(|node| ListedOutput { identifier: materialcraft_engine::eval::identifier(node), usage: materialcraft_engine::eval::output_usage(node) })
}

impl ExportSettings {
    pub fn includes(&self, graph: &str, identifier: &str) -> bool {
        !self.skipped_outputs.contains(&output_key(graph, identifier))
    }

    /// The chosen color space or the graph usage's default before format-specific encoding.
    pub fn color_space(&self, graph: &str, output: &ListedOutput) -> ColorSpace {
        self.color_spaces.get(&output_key(graph, &output.identifier)).copied().unwrap_or_else(|| ColorSpace::for_usage(output.usage))
    }

    /// Check or uncheck every output of `graph`
    pub fn include_all(&mut self, graph: &Graph, include: bool) {
        let graph_path = graph.sbs_path();
        for output in listed(graph) {
            let key = output_key(&graph_path, &output.identifier);
            if include {
                self.skipped_outputs.remove(&key);
            } else {
                self.skipped_outputs.insert(key);
            }
        }
    }

    /// The `file.export` `outputs` and `colorSpaces` for `graph`; `None` when none of its outputs is checked
    pub fn selection(&self, graph: &Graph) -> Option<(Value, Value)> {
        let graph_path = graph.sbs_path();
        let outputs: Vec<_> = listed(graph).collect();
        let included: Vec<&ListedOutput> = outputs.iter().filter(|output| self.includes(&graph_path, &output.identifier)).collect();
        if included.is_empty() {
            return None;
        }

        let identifiers =
            if included.len() == outputs.len() { Value::Null } else { json!(included.iter().map(|output| &output.identifier).collect::<Vec<_>>()) };
        let color_spaces: serde_json::Map<String, Value> = included
            .iter()
            .filter_map(|output| {
                let space = self.color_spaces.get(&output_key(&graph_path, &output.identifier))?;
                Some((output.identifier.clone(), json!(space.id())))
            })
            .collect();
        Some((identifiers, Value::Object(color_spaces)))
    }
}

/// Draw `graphs` with their outputs: the graph's checkbox checks all its outputs
pub fn tree(ui: &mut Ui, settings: &mut ExportSettings, graphs: &[&Graph]) {
    let tokens = Tokens::get(ui.ctx());
    if graphs.is_empty() {
        ui.label(RichText::new(tr("No graph with outputs")).color(tokens.text_dim));
        return;
    }

    egui::ScrollArea::vertical().id_salt("export-outputs").max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
        for graph in graphs {
            let graph_path = graph.sbs_path();
            let outputs: Vec<_> = listed(graph).collect();
            let included = outputs.iter().filter(|output| settings.includes(&graph_path, &output.identifier)).count();
            let id = ui.make_persistent_id(("export-graph", &graph_path));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    let mut all = !outputs.is_empty() && included == outputs.len();
                    let checkbox = ui.add(egui::Checkbox::new(&mut all, &graph.name).indeterminate(included > 0 && included < outputs.len()));
                    crate::widgets::name(&checkbox, egui::Role::CheckBox, &graph.name);
                    if checkbox.changed() {
                        settings.include_all(graph, all);
                    }
                    let indent = ui.spacing().indent;
                    ui.add_space((OUTPUT_COLUMN_WIDTH + indent + 16.0 - ui.min_rect().width()).max(8.0));
                    ui.label(RichText::new(tr("Exported color space")).color(tokens.text_dim));
                })
                .body(|ui| outputs_grid(ui, settings, &graph_path, &outputs));
        }
    });
}

fn outputs_grid(ui: &mut Ui, settings: &mut ExportSettings, graph_path: &str, outputs: &[ListedOutput]) {
    if outputs.is_empty() {
        ui.label(RichText::new(tr("This graph has no outputs")).color(Tokens::get(ui.ctx()).text_dim));
        return;
    }

    egui::Grid::new(("export-outputs-grid", graph_path)).num_columns(2).min_col_width(OUTPUT_COLUMN_WIDTH).spacing([8.0, 4.0]).show(ui, |ui| {
        for output in outputs {
            let key = output_key(graph_path, &output.identifier);
            let mut included = !settings.skipped_outputs.contains(&key);
            let checkbox = ui.checkbox(&mut included, &output.identifier);
            crate::widgets::name(&checkbox, egui::Role::CheckBox, &output.identifier);
            if checkbox.changed() {
                if included {
                    settings.skipped_outputs.remove(&key);
                } else {
                    settings.skipped_outputs.insert(key.clone());
                }
            }

            let original_space = settings.color_spaces.get(&key).copied();
            let mut space = original_space;
            let label = space.map_or_else(|| tr("Automatic"), ColorSpace::label);
            let combo = egui::ComboBox::from_id_salt(("export-color-space", &key)).width(110.0).selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut space, None, tr("Automatic"));
                for choice in ColorSpace::ALL {
                    ui.selectable_value(&mut space, Some(choice), choice.label());
                }
            });
            crate::widgets::name(&combo.response, egui::Role::ComboBox, &format!("{} {}", output.identifier, tr("color space")));
            if space != original_space {
                if let Some(space) = space {
                    settings.color_spaces.insert(key, space);
                } else {
                    settings.color_spaces.remove(&key);
                }
            }
            ui.end_row();
        }
    });
}
