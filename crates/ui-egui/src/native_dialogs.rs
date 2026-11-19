//! The desktop's file dialogs: queued on [`craft_dialogs::Dialogs`], shown at the end of a frame and answered on a later one

use craft_dialogs::{Answer, FileDialog};
use materialcraft_engine::image::Image;
use serde_json::{Value, json};

use crate::MaterialCraftApp;
use crate::i18n::tr;

pub(crate) type Dialogs = craft_dialogs::Dialogs<Then>;

/// What the app does with an answered dialog
pub(crate) enum Then {
    OpenPackage,
    /// `document` is the package the request was for; `before_action` finishes a Save in the unsaved-changes prompt
    SavePackage {
        document: u64,
        copy: bool,
        before_action: bool,
    },
    ImportBitmap(GraphRef),
    ImportResource {
        document: u64,
        vector: bool,
        folder: String,
    },
    /// The Export window's destination for this tab
    ExportDestination(crate::export::ExportTab),
    LoadMesh {
        document: u64,
    },
    SaveImage(Box<Image>),
    /// The 3D view's frame to write as a PNG file: width, height and RGBA pixels
    SaveRender(Box<(u32, u32, std::sync::Arc<Vec<u8>>)>),
    /// The mesh the 3D view shows, to write as an OBJ file
    ExportGeometry(std::sync::Arc<materialcraft_engine::geom::Mesh>),
    SaveViewState,
    /// Apply a saved 3D view state, also switching to its mesh when `with_mesh` is set
    LoadViewState {
        with_mesh: bool,
    },
    /// The image file the 2D view draws behind transparent pixels
    BackgroundImage,
    /// Help ▸ Export log: diagnostic events without message details
    ExportLog(String),
    /// The parameters `graph.input.bind` runs with, before the path
    BindInput(Value, GraphRef),
}

/// The active graph when a dialog was requested. Its index can pass to another graph when one is deleted, so its folder-qualified path is part of the identity
#[derive(Clone, PartialEq)]
pub(crate) struct GraphRef {
    document: u64,
    graph: usize,
    path: String,
}

impl GraphRef {
    pub(crate) fn active(app: &MaterialCraftApp) -> GraphRef {
        GraphRef { document: app.session.document_id, graph: app.session.graph, path: app.session.graph().map(|g| g.sbs_path()).unwrap_or_default() }
    }
}

/// A queue for an app that has no window to wake yet (offscreen snapshots and tests)
pub(crate) fn unattached() -> Dialogs {
    craft_dialogs::Dialogs::new(std::sync::Arc::new(|| {}))
}

/// A queue whose answers wake the window
pub(crate) fn attached(ctx: &egui::Context) -> Dialogs {
    let ctx = ctx.clone();
    craft_dialogs::Dialogs::new(std::sync::Arc::new(move || ctx.request_repaint()))
}

/// Queue a dialog unless one is already open or waiting, in which case the open one comes forward
pub(crate) fn ask(app: &mut MaterialCraftApp, dialog: FileDialog, then: Then) {
    if app.dialogs.is_open() || app.dialogs.queued() > 0 {
        app.dialogs.raise();
    } else {
        app.dialogs.ask(dialog, then);
    }
}

fn save_dialog(app: &MaterialCraftApp) -> FileDialog {
    let archive = app.session.path.as_deref().is_some_and(materialcraft_engine::file_cmds::is_mcraft);
    let extension = if archive { "mcraft" } else { "sbs" };
    let name = format!("{}.{extension}", materialcraft_engine::formats::safe_file_name(&app.session.pkg.name));

    crate::windows::package_dialog(false, archive).file_name(name)
}

/// File ▸ Save as and Save a copy as: choose where to write the active package
pub(crate) fn save_package(app: &mut MaterialCraftApp, copy: bool) {
    let then = Then::SavePackage { document: app.session.document_id, copy, before_action: false };
    let dialog = save_dialog(app);
    ask(app, dialog, then);
}

/// Help ▸ Export log: ask where to write a diagnostic event log
pub(crate) fn export_log(app: &mut MaterialCraftApp) {
    let text = log_for_export(&app.session);
    let dialog = craft_dialogs::FileDialog::save().file_name("materialcraft-log.txt").filter(tr("Text"), &["txt"]);
    ask(app, dialog, Then::ExportLog(text));
}

fn log_for_export(session: &materialcraft_engine::Session) -> String {
    use std::fmt::Write as _;

    let mut text = format!("MaterialCraft {}\nMessage details omitted to protect document names and paths.\n", materialcraft_engine::VERSION);

    // Raw Console messages can contain names and paths in platform-specific error formats
    for message in &session.log {
        let event = match message.as_str() {
            message if message.starts_with(materialcraft_engine::WARNING_PREFIX) => "Package warning",
            message if message.starts_with("Opened ") => "Opened package",
            message if message.starts_with("Saved ") => "Saved file",
            message if message.starts_with("Save all: ") => "Saved multiple packages",
            message if message.starts_with("Exported ") => "Exported file",
            _ => "Session message",
        };
        let _ = writeln!(text, "{event}");
    }

    let (failures, unsupported) = session.cook_failures();
    let _ = writeln!(text, "Node errors in the active graph: {}\nUnsupported nodes: {unsupported}", failures.len());
    text
}

/// Save in the unsaved-changes prompt, for a package without a path: waits behind any open dialog
pub(crate) fn save_before_action(app: &mut MaterialCraftApp) {
    let then = Then::SavePackage { document: app.session.document_id, copy: false, before_action: true };
    let dialog = save_dialog(app);
    app.dialogs.ask(dialog, then);
}

/// Apply the answers that arrived since the last frame. Call once per frame
pub(crate) fn poll(app: &mut MaterialCraftApp) {
    for (then, answer) in app.dialogs.take_answers() {
        match answer {
            Answer::Paths(paths) => match paths.into_iter().next() {
                Some(path) => chosen(app, then, &path),
                None => cancelled(app, then),
            },
            Answer::Cancelled | Answer::Message(_) => cancelled(app, then),
            Answer::Unavailable(why) => {
                app.toast(format!("{}: {why}", tr("The file dialog could not be shown")), true);
                cancelled(app, then);
            }
        }
    }
}

fn cancelled(app: &mut MaterialCraftApp, then: Then) {
    if matches!(then, Then::SavePackage { before_action: true, .. }) {
        crate::unsaved::finish_save_dialog(app, false);
    }
}

fn chosen(app: &mut MaterialCraftApp, then: Then, path: &std::path::Path) {
    let picked = path.display().to_string();
    match then {
        Then::OpenPackage => crate::windows::open_path(app, &picked),
        Then::SavePackage { document, copy, before_action } => {
            let saved = save_to(app, document, copy, &picked);
            if before_action {
                crate::unsaved::finish_save_dialog(app, saved);
            }
        }
        Then::ImportBitmap(target) => {
            if GraphRef::active(app) == target {
                crate::graph::import_bitmap_file(app, &picked);
            } else {
                not_applied(app);
            }
        }
        Then::ImportResource { document, vector, folder } => {
            // A resource picker belongs to the package that opened it, even after another package becomes active
            if app.run("package.activate", json!({"document": document})).is_none() {
                return;
            }

            let command = if vector { "file.importVector" } else { "file.importBitmap" };
            app.run(command, json!({"path": picked, "folder": folder}));
        }
        Then::ExportDestination(tab) => {
            let settings = app.ui.export_draft.get_or_insert_with(|| app.ui.export.clone());
            match tab {
                crate::export::ExportTab::MaterialX => settings.material.folder = picked,
                crate::export::ExportTab::Mcmat => settings.material.archive = picked,
                crate::export::ExportTab::FromGraph | crate::export::ExportTab::Batch => settings.destination = picked,
            }
        }
        Then::LoadMesh { document } => {
            if app.session.document_id == document {
                app.run("view.3d.loadMesh", json!({"path": picked}));
            } else {
                not_applied(app);
            }
        }
        Then::SaveImage(image) => save_image(app, &image, path),
        Then::SaveRender(frame) => {
            let (width, height, pixels) = *frame;
            let snapshot = materialcraft_engine::DisplaySnapshot::render_to(width, height, pixels, path.to_path_buf());
            if app.run_with_display_snapshot("view.3d.saveRender", json!({}), snapshot).0.is_some() {
                app.toast(format!("{} {picked}", tr("Saved")), false);
            }
        }
        Then::ExportGeometry(mesh) => {
            let snapshot = materialcraft_engine::DisplaySnapshot::geometry_to(mesh, path.to_path_buf());
            if app.run_with_display_snapshot("view.3d.exportGeometry", json!({}), snapshot).0.is_some() {
                app.toast(format!("{} {picked}", tr("Saved")), false);
            }
        }
        Then::ExportLog(text) => match materialcraft_engine::formats::write_atomic(path, text.as_bytes()) {
            Ok(()) => app.toast_only(format!("{} {picked}", tr("Saved")), false),
            Err(error) => app.toast_only(format!("{}: {error}", tr("The log could not be saved")), true),
        },
        Then::SaveViewState => {
            if app.run("view.3d.saveState", json!({"path": picked})).is_some() {
                app.toast(format!("{} {picked}", tr("Saved")), false);
            }
        }
        Then::LoadViewState { with_mesh } => {
            app.run("view.3d.loadState", json!({"path": picked, "mesh": with_mesh}));
        }
        Then::BackgroundImage => {
            app.run("view.2d", json!({"background": picked}));
        }
        Then::BindInput(mut parameters, target) => {
            if GraphRef::active(app) != target {
                not_applied(app);
                return;
            }

            parameters["path"] = json!(picked);
            app.run("graph.input.bind", parameters);
        }
    }
}

/// The commands behind these dialogs act on the active graph, so an answer for a graph that is no longer active, or was renamed or moved, is reported instead of applied
fn not_applied(app: &mut MaterialCraftApp) {
    app.toast(tr("Not applied: the active graph or package changed while the file dialog was open"), true);
}

/// Save `document` to the chosen path, activating it first when another package became active meanwhile
fn save_to(app: &mut MaterialCraftApp, document: u64, copy: bool, picked: &str) -> bool {
    if app.session.document_id != document && app.run("package.activate", json!({"document": document})).is_none() {
        return false;
    }

    let path = materialcraft_engine::file_cmds::package_path(picked);
    if app.run(if copy { "file.saveCopyAs" } else { "file.save" }, json!({"path": path})).is_none() {
        return false;
    }

    if !copy {
        app.ui.add_recent(&path);
    }
    app.toast(format!("{} {path}", tr("Saved")), false);
    true
}

fn save_image(app: &mut MaterialCraftApp, image: &Image, path: &std::path::Path) {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    let depth = if ext == "exr" { materialcraft_engine::formats::Depth::F32 } else { materialcraft_engine::formats::Depth::U16 };
    let result =
        materialcraft_engine::formats::encode(image, &ext, depth, !image.is_gray()).and_then(|b| materialcraft_engine::formats::write_atomic(path, &b));
    match result {
        Ok(()) => app.toast(format!("{} {}", tr("Saved"), path.display()), false),
        Err(e) => app.toast(e.to_string(), true),
    }
}

#[cfg(test)]
#[path = "native_dialogs.test.rs"]
mod tests;
