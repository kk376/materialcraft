use serde_json::{Value, json};

use super::*;

/// One 16 px graph, New_graph, with two connected outputs: base_color (base color usage) and height
fn app() -> MaterialCraftApp {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
    app.run("graph.size", json!({"width": 16}));
    for (identifier, usage) in [("base_color", "baseColor"), ("height", "height")] {
        let usage = materialcraft_engine::core::ComponentUsage::index(usage);
        let source = app.run("graph.node.add", json!({"def": "uniform"})).unwrap()["id"].as_u64().unwrap();
        app.run("graph.node.add", json!({"def": "output", "params": {"identifier": identifier, "usage": usage}, "connectFrom": {"node": source}})).unwrap();
    }
    app
}

fn temp(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("mc-export-window-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

fn files(directory: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .map_or_else(|_| Vec::new(), |entries| entries.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect());
    names.sort();
    names
}

#[test]
fn selection_names_the_checked_outputs_and_their_color_spaces() {
    let app = app();
    let graph = app.session.graph().unwrap();
    let mut settings = ExportSettings::default();

    let (outputs, color_spaces) = settings.selection(graph).unwrap();
    assert_eq!(outputs, Value::Null, "with every output checked, file.export writes them all");
    assert_eq!(color_spaces, json!({}), "unpicked outputs follow the encoder's defaults");

    settings.skipped_outputs.insert(output_key("New_graph", "height"));
    settings.color_spaces.insert(output_key("New_graph", "base_color"), materialcraft_engine::ColorSpace::Linear);
    assert_eq!(settings.selection(graph).unwrap(), (json!(["base_color"]), json!({"base_color": "linear"})));

    settings.include_all(graph, false);
    assert_eq!(settings.selection(graph), None);
    settings.include_all(graph, true);
    assert!(settings.skipped_outputs.is_empty());
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn from_graph_writes_only_the_checked_outputs() {
    let mut app = app();
    let directory = temp("from-graph");
    let mut settings = ExportSettings { destination: directory.display().to_string(), pattern: "$(identifier)".into(), ..ExportSettings::default() };
    settings.skipped_outputs.insert(output_key("New_graph", "height"));
    app.ui.export_tab = ExportTab::FromGraph;

    assert!(export(&mut app, &settings));

    assert_eq!(files(&directory), ["base_color.png"]);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn batch_writes_every_material_graph_at_the_output_size() {
    let mut app = app();
    app.run("file.new", json!({"name": "Second", "template": "empty"})).unwrap();
    let source = app.run("graph.node.add", json!({"def": "uniform"})).unwrap()["id"].clone();
    app.run("graph.node.add", json!({"def": "output", "params": {"identifier": "mask"}, "connectFrom": {"node": source}})).unwrap();
    let directory = temp("batch");
    let mut settings = ExportSettings { destination: directory.display().to_string(), ..ExportSettings::default() };
    settings.batch = BatchExportSettings { width: 64, height: 32, linked: false };
    app.ui.export_tab = ExportTab::Batch;

    assert!(export(&mut app, &settings));

    assert_eq!(files(&directory), ["New_graph_base_color.png", "New_graph_height.png", "Second_mask.png"]);

    let image = materialcraft_engine::formats::decode(&std::fs::read(directory.join("Second_mask.png")).unwrap(), true).unwrap();

    assert_eq!((image.width(), image.height()), (64, 32));

    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn unwired_options_say_they_are_not_available_yet() {
    let mut app = app();
    let directory = temp("unwired");
    let planned = ExportSettings { destination: directory.display().to_string(), format: "dds".into(), ..ExportSettings::default() };

    app.ui.export_tab = ExportTab::FromGraph;
    assert!(!export(&mut app, &planned));
    assert!(app.toasts.last().unwrap().text.contains("isn't available yet"));
    assert!(!directory.exists(), "nothing is written");
}

#[test]
fn the_format_list_drops_tiff_and_lists_dds_as_planned() {
    let formats: Vec<ExportFormat> = ExportFormat::all().collect();

    assert!(formats.iter().all(|format| format.extension != "tif" && format.extension != "psd"));
    assert_eq!(ExportFormat::find("dds").map(|format| format.available), Some(false));
    assert_eq!(ExportFormat::find("png").map(ExportFormat::title).as_deref(), Some("PNG (*.png)"));
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn material_dialog_rejects_another_package_with_the_same_graph_path() {
    for tab in [ExportTab::Mcmat, ExportTab::MaterialX] {
        let mut app = app();
        open(&mut app, tab);
        let draft = app.ui.export_draft.clone().unwrap();
        let original = app.session.document_id;
        app.run("file.newPackage", json!({})).unwrap();
        app.run("file.new", json!({"name": draft.material.root, "template": "empty"})).unwrap();
        assert!(app.session.pkg.graph_by_name(&draft.material.root).is_some());
        assert!(!publish(&mut app, &draft));
        assert!(app.toasts.last().unwrap().text.contains("export package changed"));
        assert!(app.material_export_job.is_none());

        // Switching back preserves the original dialog and reaches destination validation
        app.run("package.activate", json!({"document": original})).unwrap();
        assert!(!publish(&mut app, &draft));
        assert!(app.toasts.last().unwrap().text.contains("export destination"));
    }
}

#[test]
fn material_dialog_does_not_restore_a_closed_package_identity() {
    let mut app = app();
    open(&mut app, ExportTab::Mcmat);
    let draft = app.ui.export_draft.clone().unwrap();
    app.run("package.closeAll", json!({"discard": true})).unwrap();
    assert!(!publish(&mut app, &draft));
    assert!(app.toasts.last().unwrap().text.contains("export package changed"));
    app.run("file.new", json!({"name": draft.material.root, "template": "empty"})).unwrap();
    app.toasts.clear();
    assert!(!publish(&mut app, &draft));
    assert!(app.toasts.last().unwrap().text.contains("export package changed"));
    open(&mut app, ExportTab::Mcmat);
    let reopened = app.ui.export_draft.as_ref().unwrap();
    assert_eq!(reopened.material.document, Some(app.session.document_id));
    assert_ne!(reopened.material.document, draft.material.document);
}

#[test]
fn material_dialog_identity_is_not_saved_in_preferences() {
    let mut app = app();
    open(&mut app, ExportTab::Mcmat);
    let draft = app.ui.export_draft.as_ref().unwrap();
    let stored = serde_json::to_value(draft).unwrap();
    assert!(stored["material"].get("document").is_none());
    let restored: ExportSettings = serde_json::from_value(stored).unwrap();
    assert_eq!(restored.material.document, None);
}

#[test]
fn folder_graphs_keep_independent_output_selection_and_color_spaces() {
    let app = app();
    let mut indoor = app.session.graph().unwrap().clone();
    indoor.name = "Oak".into();
    indoor.folder = "Indoor".into();
    let mut outdoor = indoor.clone();
    outdoor.folder = "Outdoor".into();
    let mut settings = ExportSettings::default();

    settings.include_all(&indoor, false);
    assert!(settings.selection(&indoor).is_none());
    assert_eq!(settings.selection(&outdoor).unwrap(), (Value::Null, json!({})));

    settings.include_all(&indoor, true);
    settings.color_spaces.insert(output_key(&indoor.sbs_path(), "base_color"), materialcraft_engine::ColorSpace::Linear);

    assert_eq!(settings.selection(&indoor).unwrap(), (Value::Null, json!({"base_color":"linear"})));
    assert_eq!(settings.selection(&outdoor).unwrap(), (Value::Null, json!({})));
}

#[test]
fn material_export_reports_missing_shader_connections_on_every_platform() {
    assert_eq!(material_export_message(&Value::Null), tr("Material exported"));
    let message = material_export_message(&json!({"unmapped":["custom_output"]}));
    assert!(message.contains(tr("Exported without shader connections")));
    assert!(message.contains("custom_output"));
}
