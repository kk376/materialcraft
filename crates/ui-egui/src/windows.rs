//! Dialogs (new graph, preferences, shortcuts, about, home), the console and history panels, file actions behind the File menu, and the function editor

use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use egui::{Color32, RichText, TextureHandle, Ui, vec2};
use egui_phosphor::regular as icon;
use materialcraft_engine::core::FunctionGraph;
use materialcraft_engine::core::NodeId;
use serde_json::{Value, json};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Template previews for the New graph dialog (rendered in the background on first open)
#[derive(Default)]
pub struct TemplatePreviews {
    tex: HashMap<String, TextureHandle>,
    rx: Option<Receiver<crate::thumbs::Previews>>,
    started: bool,
}

const PREVIEW: u32 = 96;

fn render_templates() -> crate::thumbs::Previews {
    let light = materialcraft_render::Lighting::new("studio");
    let scene = materialcraft_render::Scene::builtin("sphere");
    let settings = materialcraft_render::Settings { distance: 2.7, supersample: 2, ..Default::default() };
    materialcraft_engine::samples::TEMPLATES
        .iter()
        .filter_map(|(id, _)| {
            let mut s = materialcraft_engine::Session::new(materialcraft_engine::core::Package::new("t"));
            s.pkg.graphs.push(materialcraft_engine::samples::template(id, "t", 8).ok()?);
            s.material_preview = materialcraft_engine::MaterialPreview::for_graph(&s.pkg, 0);
            s.preview_size = Some(256);
            s.cook();
            let maps = materialcraft_render::Maps::from_usages(s.material_maps());
            Some((id.to_string(), materialcraft_render::render(&scene, &maps, &light, &settings, PREVIEW, PREVIEW)))
        })
        .collect()
}

impl TemplatePreviews {
    fn get(&mut self, ctx: &egui::Context, id: &str) -> Option<egui::TextureId> {
        if !self.started {
            self.started = true;
            let (tx, rx) = std::sync::mpsc::channel();
            #[cfg(not(target_arch = "wasm32"))]
            {
                let c = ctx.clone();
                let _ = std::thread::Builder::new().name("materialcraft-templates".into()).spawn(move || {
                    let _ = tx.send(render_templates());
                    c.request_repaint();
                });
            }
            #[cfg(target_arch = "wasm32")]
            let _ = tx.send(render_templates());
            self.rx = Some(rx);
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(previews) => {
                    for (template, pixels) in previews {
                        let image = egui::ColorImage::from_rgba_unmultiplied([PREVIEW as usize, PREVIEW as usize], &pixels);
                        let texture = ctx.load_texture(format!("tpl-{template}"), image, egui::TextureOptions::LINEAR);
                        self.tex.insert(template, texture);
                    }
                    self.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        self.tex.get(id).map(TextureHandle::id)
    }
}

/// A template's rendered preview, once the background render has finished
pub(crate) fn template_preview(app: &mut MaterialCraftApp, ctx: &egui::Context, id: &str) -> Option<egui::TextureId> {
    (id != crate::template_card::FUNCTION_TEMPLATE).then(|| app.template_previews.get(ctx, id)).flatten()
}

pub fn show(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    if let Some(error) = app.thumbs.tick(ctx, &app.session.library, app.cooker.busy() || app.session.dirty) {
        app.toast(error, true);
    }

    if app.ui.windows.new_graph {
        crate::new_graph_dialog::show(app, ctx);
    }
    if app.ui.windows.export {
        crate::export::show(app, ctx);
    }
    if app.ui.windows.preferences {
        crate::preferences::show(app, ctx);
    }
    if app.ui.windows.about {
        about(app, ctx);
    }
    if app.ui.windows.shortcuts {
        shortcuts(app, ctx);
    }
    if app.ui.windows.home {
        crate::home_screen::show(app, ctx);
    }
    if app.ui.windows.plugin_manager {
        crate::plugin_manager::show(app, ctx);
    }

    crate::unsaved::show(app, ctx);
    crate::explorer::dialogs(app, ctx);
    crate::properties::ExposeParameterDialog::show(app, ctx);
    #[cfg(target_arch = "wasm32")]
    crate::web_files::save_dialog(app, ctx);
    #[cfg(not(target_arch = "wasm32"))]
    crate::updates_ui::show(app, ctx);
}

pub(crate) fn modal(ctx: &egui::Context, id: &str, title: &str, open: &mut bool, size: egui::Vec2, body: impl FnOnce(&mut Ui)) {
    egui::Window::new(title)
        .id(egui::Id::unique(id))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_size(size)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, body);
}

pub(crate) fn dialog_size(ctx: &egui::Context, preferred: egui::Vec2) -> egui::Vec2 {
    // Leave room for the title bar and window margins inside the viewport
    const FRAME_SPACE: egui::Vec2 = vec2(32.0, 72.0);
    preferred.min((ctx.content_rect().size() - FRAME_SPACE).max(vec2(20.0, 40.0)))
}

fn about(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut open = true;
    modal(ctx, "about", tr("About MaterialCraft"), &mut open, vec2(400.0, 260.0), |ui| {
        let tk = Tokens::get(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("MaterialCraft").size(22.0).strong().color(tk.accent));
            ui.label(format!("{} {}", tr("Version"), materialcraft_engine::VERSION));
            ui.add_space(6.0);
            ui.label(tr("Node-based procedural materials, built in pure Rust."));
            ui.label(tr("An open-source Crafting App by the ArtCraft team and community."));
            ui.add_space(6.0);
            ui.hyperlink_to("getartcraft.com/apps/materialcraft", "https://getartcraft.com/apps/materialcraft");
            ui.hyperlink_to("discord.gg/artcraft", "https://discord.gg/artcraft");
            ui.add_space(6.0);
            ui.label(RichText::new("MIT OR Apache-2.0").size(11.0).color(tk.text_dim));
        });
    });
    if !open {
        app.ui.windows.about = false;
    }
}

fn shortcuts(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut open = true;
    modal(ctx, "shortcuts", tr("Shortcuts"), &mut open, vec2(420.0, 420.0), |ui| {
        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
            egui::Grid::new("sc").striped(true).show(ui, |ui| {
                for i in crate::menus::CATALOG.iter().filter(|i| i.shortcut.is_some()) {
                    ui.label(tr(i.label));
                    ui.label(RichText::new(crate::menus::shortcut_label(ctx, i.shortcut.unwrap_or_default())).monospace());
                    ui.end_row();
                }
                for (k, what) in [
                    ("Space / Tab", "Node search (over the graph)"),
                    ("F", "Frame all / the selection"),
                    ("Wheel", "Zoom the graph and the views"),
                    ("Middle drag / Alt+drag", "Pan the graph"),
                    ("Double-click node", "Show in the 2D view"),
                    ("Shift+D", "Disable selection"),
                    ("Middle drag / Alt+drag (3D view)", "Pan the camera"),
                    ("Shift+drag (3D view)", "Turn the environment"),
                ] {
                    ui.label(tr(what));
                    ui.label(RichText::new(k).monospace());
                    ui.end_row();
                }
            });
        });
    });
    if !open {
        app.ui.windows.shortcuts = false;
    }
}

pub fn console(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    let console_line = |text: String, color: Color32| RichText::new(text).monospace().size(11.0).color(color);

    egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
        for line in &app.session.log {
            let warning = line.starts_with(materialcraft_engine::WARNING_PREFIX);
            ui.label(console_line(line.clone(), if warning { tk.warning } else { tk.text }));
        }

        // Placeholders for missing SBS nodes are already listed in the warnings, so they get one line
        let (failed, placeholders) = app.session.cook_failures();
        for (id, error) in failed {
            ui.label(console_line(format!("node {}: {error}", id.0), tk.danger));
        }
        if placeholders > 0 {
            let text = format!("{placeholders} {}", tr("placeholder node(s) stand in for nodes MaterialCraft doesn't have yet (see the warnings above)"));
            ui.label(console_line(text, tk.warning));
        }
    });
}

pub fn history(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, h) in app.session.document_history_labels(crate::history::document(app)).iter().enumerate() {
            ui.label(RichText::new(format!("{:>3}  {h}", i + 1)).size(11.5).color(tk.text));
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(crate::menus::enabled(app, "edit.undo"), egui::Button::new(format!("{} {}", icon::ARROW_U_UP_LEFT, tr("Undo")))).clicked() {
                app.run("edit.undo", json!({}));
            }
            if ui.add_enabled(crate::menus::enabled(app, "edit.redo"), egui::Button::new(format!("{} {}", icon::ARROW_U_UP_RIGHT, tr("Redo")))).clicked() {
                app.run("edit.redo", json!({}));
            }
        });
    });
}

/// The function parameter of a pixel/value processor: a button opening the function graph editor, and the function as JSON for copy and paste
pub fn function_summary(app: &mut MaterialCraftApp, ui: &mut Ui, node: NodeId, param: &str, f: &FunctionGraph) {
    let tk = Tokens::get(ui.ctx());
    if ui.add(crate::widgets::primary_button(ui, &format!("{}  {}", icon::FLOW_ARROW, tr("Edit function graph...")))).clicked() {
        crate::graph::subgraph_navigation::open_function(app, node, crate::function::FunctionTarget::Parameter(param.to_string()));
    }
    ui.label(RichText::new(format!("{} {}", f.nodes.len(), tr("function nodes"))).size(10.5).color(tk.text_dim));
    let id = egui::Id::unique(("fn-edit", node.0, param));
    egui::CollapsingHeader::new(tr("As JSON")).id_salt(("fn-json", node.0, param)).default_open(false).show(ui, |ui| {
        let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| serde_json::to_string_pretty(f).unwrap_or_default());
        if ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(8).desired_width(f32::INFINITY)).changed() {
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
        ui.horizontal(|ui| {
            if ui.button(tr("Apply")).clicked() {
                match serde_json::from_str::<FunctionGraph>(&text) {
                    Ok(g) => {
                        let v = serde_json::to_value(materialcraft_engine::core::Value::Function(Box::new(g))).unwrap_or(Value::Null);
                        app.run("graph.node.param", json!({"id": node.0, "param": param, "value": v}));
                        ui.data_mut(|d| d.remove::<String>(id));
                    }
                    Err(e) => app.toast(format!("{}: {e}", tr("Bad function")), true),
                }
            }
            if ui.button(tr("Revert")).clicked() {
                ui.data_mut(|d| d.remove::<String>(id));
            }
        });
    });
}

/// Authoring and published material formats offered by File ▸ Open.
pub const OPEN_EXTS: &[&str] = &["sbs", "mcraft", "mcmat"];

pub(crate) fn package_filters(open: bool, archive: bool) -> Vec<(&'static str, &'static [&'static str])> {
    let mut filters: Vec<(&str, &[&str])> = vec![("SBS XML Package (*.sbs)", &["sbs"]), ("MaterialCraft Project (*.mcraft)", &["mcraft"])];
    if archive {
        filters.reverse();
    }
    if open {
        filters.push(("MaterialCraft Material (*.mcmat)", &["mcmat"]));
        filters.insert(0, ("All formats (*.sbs *.mcraft *.mcmat)", OPEN_EXTS));
    }
    filters
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn package_dialog(open: bool, archive: bool) -> craft_dialogs::FileDialog {
    let mut dialog = if open { craft_dialogs::FileDialog::open() } else { craft_dialogs::FileDialog::save() };
    for (name, extensions) in package_filters(open, archive) {
        dialog = dialog.filter(name, extensions);
    }

    dialog
}

#[cfg(not(target_arch = "wasm32"))]
fn resource_dialog(vector: bool) -> craft_dialogs::FileDialog {
    let mut dialog = craft_dialogs::FileDialog::open();
    for (label, extensions) in resource_filters(vector) {
        dialog = dialog.filter(label, extensions);
    }

    dialog
}

pub(crate) fn resource_filters(vector: bool) -> Vec<(String, &'static [&'static str])> {
    use materialcraft_engine::formats::{BITMAP_FORMATS, VECTOR_FORMATS};
    let formats = if vector { VECTOR_FORMATS } else { BITMAP_FORMATS };
    formats
        .iter()
        .map(|(label, extensions)| {
            let patterns = extensions.iter().map(|extension| format!("*.{extension}")).collect::<Vec<_>>().join(" ");
            (format!("{label} ({patterns})"), *extensions)
        })
        .collect()
}

/// Import an asset into the active package without creating a graph node.
pub fn import_resource(app: &mut MaterialCraftApp, ctx: &egui::Context, vector: bool) {
    import_resource_into(app, ctx, vector, "Resources");
}

pub(crate) fn import_resource_into(app: &mut MaterialCraftApp, ctx: &egui::Context, vector: bool, folder: &str) {
    if !app.session.has_package() {
        return;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let then = crate::native_dialogs::Then::ImportResource { document: app.session.document_id, vector, folder: folder.to_string() };
        crate::native_dialogs::ask(app, resource_dialog(vector), then);
    }
    #[cfg(target_arch = "wasm32")]
    crate::web_files::import_resource(ctx, app.session.document_id, vector, folder);
}

/// After a package opened with warnings (an SBS file using nodes MaterialCraft lacks, say), bring the Console forward
pub fn after_open(app: &mut MaterialCraftApp) {
    let warning_count = app.session.load_warnings.len();
    if warning_count > 0 {
        app.ui.dock.focus(crate::docking::Panel::Console);
        app.toast(format!("{} {warning_count} {}", tr("Opened with"), tr("warnings: see the Console")), false);
    }
}

/// File menu actions that need a file dialog. Returns true when handled
pub fn file_action(app: &mut MaterialCraftApp, ctx: &egui::Context, id: &str) -> bool {
    if matches!(id, "file.save" | "file.saveAs" | "file.saveCopyAs" | "file.importBitmap" | "file.importVector") && !app.session.has_package() {
        return true;
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = ctx;
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::native_dialogs::{self, Then};
        match id {
            "file.open" => {
                native_dialogs::ask(app, package_dialog(true, false), Then::OpenPackage);
                true
            }
            "file.save" if has_local_path(app) => {
                if let Some(r) = app.run("file.save", json!({})) {
                    let path = r["path"].as_str().unwrap_or_default().to_string();
                    app.ui.add_recent(&path);
                    app.toast(format!("{} {path}", tr("Saved")), false);
                }
                true
            }
            "file.save" | "file.saveAs" | "file.saveCopyAs" => {
                native_dialogs::save_package(app, id == "file.saveCopyAs");
                true
            }
            "file.exportMaterial" => {
                crate::export::open(app, crate::export::ExportTab::Mcmat);
                true
            }
            "file.saveAll" => {
                save_all(app);
                true
            }
            "file.sendToTextureCraft" => {
                if app.run("file.sendToTextureCraft", json!({})).is_some() {
                    app.toast(tr("Sent to TextureCraft: find it on the shelf under My smart materials"), false);
                }
                true
            }
            "file.importBitmap" => {
                let then = Then::ImportBitmap(native_dialogs::GraphRef::active(app));
                native_dialogs::ask(app, resource_dialog(false), then);
                true
            }
            "file.importVector" => {
                import_resource(app, ctx, true);
                true
            }
            _ => false,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        match id {
            "file.open" => crate::web_files::open(ctx),
            "file.importBitmap" => crate::web_files::import_bitmap(app, ctx),
            "file.importVector" => import_resource(app, ctx, true),
            "file.save" => crate::web_files::save(app),
            "file.saveAs" => crate::web_files::save_as(app, false),
            "file.saveCopyAs" => crate::web_files::save_copy(app),
            "file.exportMaterial" => crate::export::open(app, crate::export::ExportTab::Mcmat),
            _ => return false,
        }
        true
    }
}

/// File ▸ Save all: save every modified package with a file and say which still need Save as
#[cfg(not(target_arch = "wasm32"))]
fn save_all(app: &mut MaterialCraftApp) {
    let Some(reply) = app.run("file.saveAll", json!({})) else { return };

    let saved = reply["saved"].as_array().into_iter().flatten();
    let saved_count = saved.clone().count();
    for path in saved.filter_map(Value::as_str) {
        app.ui.add_recent(path);
    }

    let without_file: Vec<&str> = reply["withoutFile"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let failures: Vec<String> = reply["failed"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|failure| format!("{}: {}", failure["package"].as_str().unwrap_or_default(), failure["error"].as_str().unwrap_or_default()))
        .collect();
    let mut message = format!("{saved_count} {}", tr("packages saved"));
    if !without_file.is_empty() {
        message.push_str(&format!("\n{} {}", tr("Use Save as for:"), without_file.join(", ")));
    }
    if !failures.is_empty() {
        message.push_str(&format!("\n{} {}", tr("Not saved:"), failures.join("; ")));
    }

    app.session.log(format!("Save all: {saved_count} saved, {} need Save as, {} failed", without_file.len(), failures.len()));
    app.toast_only(message, !failures.is_empty());
}

/// Whether File ▸ Save writes straight to the package's path (a package opened from a URL has none to write to)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn has_local_path(app: &MaterialCraftApp) -> bool {
    app.session.path.as_ref().is_some_and(|path| !path.contains("://"))
}

/// Open a package from a path (recent list, command line)
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path(app: &mut MaterialCraftApp, path: &str) {
    open_path_now(app, path);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_path_now(app: &mut MaterialCraftApp, path: &str) {
    if app.run("file.open", json!({"path": path})).is_some() {
        app.ui.add_recent(path);
        app.ui.frame_all = true;
        app.toast(format!("{} {path}", tr("Opened")), false);
        after_open(app);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "windows.test.rs"]
mod tests;
