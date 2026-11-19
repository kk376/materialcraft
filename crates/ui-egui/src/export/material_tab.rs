//! Publish a procedural archive or bake its outputs for MaterialX.
use super::{ExportSettings, ExportTab, TextureDepth, outputs};
use crate::{MaterialCraftApp, i18n::tr};
use egui::Ui;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, settings: &mut ExportSettings, tab: ExportTab) {
    let material = &mut settings.material;
    super::form(ui, "export-material-options", |ui, width| {
        super::form_label(ui, tr("Root graph"));
        let selected = material.root.clone();
        super::dropdown(ui, "Root graph", width, &selected, |ui| {
            for graph in app.session.pkg.graphs.iter().filter(|g| g.function.is_none()) {
                let path = graph.sbs_path();
                ui.selectable_value(&mut material.root, path.clone(), path);
            }
        });
        ui.end_row();
        #[cfg(not(target_arch = "wasm32"))]
        {
            super::form_label(ui, tr("Destination"));
            let path = if tab == ExportTab::Mcmat { &mut material.archive } else { &mut material.folder };
            if super::path_field(ui, path, width) {
                ask_destination(app, tab, &material.root);
            }
            ui.end_row();
        }
        if tab == ExportTab::MaterialX {
            super::form_label(ui, tr("Resolution"));
            super::size_menu(ui, "Resolution", width, &mut material.size);
            ui.end_row();
            super::form_label(ui, tr("Textures"));
            super::dropdown(ui, "Textures", width, tr(material.texture_depth.label()), |ui| {
                for depth in TextureDepth::ALL {
                    ui.selectable_value(&mut material.texture_depth, depth, tr(depth.label()));
                }
            });
            ui.end_row();
            super::form_label(ui, tr("Source normals"));
            let conventions = [(None, "Automatic"), (Some(false), "OpenGL (Y+)"), (Some(true), "DirectX (Y-)")];
            let label = conventions.iter().find(|(value, _)| *value == material.normal_y_down).map_or("Automatic", |(_, label)| *label);
            super::dropdown(ui, "Source normals", width, tr(label), |ui| {
                for (value, label) in conventions {
                    ui.selectable_value(&mut material.normal_y_down, value, tr(label));
                }
            });
            ui.end_row();
            super::form_label(ui, tr("Displacement scale"));
            ui.add(egui::DragValue::new(&mut material.displacement_scale).speed(0.01).range(0.0..=100.0));
            ui.end_row();
        }
    });
    if tab == ExportTab::MaterialX {
        let graph = app.session.pkg.graph_by_name(&material.root).map(|(_, g)| g);
        outputs::tree(ui, settings, graph.as_slice());
    }
}

/// A folder for MaterialX, an archive path for MCMAT
#[cfg(not(target_arch = "wasm32"))]
fn ask_destination(app: &mut MaterialCraftApp, tab: ExportTab, root: &str) {
    use materialcraft_engine::formats::material::MATERIAL_EXT;

    let dialog = if tab == ExportTab::Mcmat {
        let graph = root.rsplit('/').next().unwrap_or(root);
        let file = format!("{}.{MATERIAL_EXT}", materialcraft_engine::formats::safe_file_name(graph));
        craft_dialogs::FileDialog::save().file_name(file).filter("MaterialCraft material", &[MATERIAL_EXT])
    } else {
        craft_dialogs::FileDialog::folder()
    };
    crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::ExportDestination(tab));
}
