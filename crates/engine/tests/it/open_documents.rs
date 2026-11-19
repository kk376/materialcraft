use materialcraft_engine::{Session, core::Package, execute, file_cmds, formats, inspect};
use serde_json::json;

#[test]
fn closing_the_last_package_leaves_no_document_and_opening_does_not_restore_it() {
    let mut session = Session::new(Package::new("Untitled package"));
    let closed = session.document_id;
    execute(&mut session, "package.close", &json!({})).unwrap();

    assert!(session.open_packages().is_empty());
    assert!(inspect::session(&session, None)["package"].is_null());
    assert!(session.package(closed).is_err());
    assert!(execute(&mut session, "package.activate", &json!({"document": closed})).is_err());
    assert!(file_cmds::save_bytes(&session).is_err());
    assert!(execute(&mut session, "folder.create", &json!({"name": "Hidden"})).is_err());
    assert!(execute(&mut session, "file.new", &json!({"template": "invalid"})).is_err());
    assert!(session.open_packages().is_empty());

    let bytes = formats::sbs::write_sbs(&Package::new("Reopened")).unwrap();
    file_cmds::open_bytes(&mut session, &bytes, Some("reopened.sbs".into())).unwrap();

    assert_eq!(session.open_packages().len(), 1);
    assert_ne!(session.document_id, closed);
    assert_eq!(session.pkg.name, "Reopened");

    execute(&mut session, "package.closeAll", &json!({})).unwrap();

    assert!(session.open_packages().is_empty());

    execute(&mut session, "file.new", &json!({"template": "empty", "name": "New graph"})).unwrap();

    assert_eq!(session.open_packages().len(), 1);
    assert_eq!(session.graph().unwrap().name, "New graph");
    assert!(execute(&mut session, "package.close", &json!({})).is_err());

    execute(&mut session, "package.close", &json!({"discard": true})).unwrap();

    assert!(session.open_packages().is_empty());
    assert!(!session.can_undo());

    execute(&mut session, "file.newPackage", &json!({})).unwrap();

    assert_eq!(session.open_packages().len(), 1);
}

#[test]
fn closing_all_packages_checks_edits_and_leaves_no_placeholder() {
    let mut session = materialcraft_engine::new_session();
    execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    execute(&mut session, "file.newPackage", &json!({"name": "Second"})).unwrap();

    assert!(execute(&mut session, "package.closeAll", &json!({})).is_err());
    assert_eq!(session.open_packages().len(), 2);

    execute(&mut session, "package.closeAll", &json!({"discard": true})).unwrap();

    assert!(session.open_packages().is_empty());
    assert!(session.graph().is_err());
    assert!(!session.can_undo());
    assert!(!session.modified);

    execute(&mut session, "package.closeAll", &json!({})).unwrap();

    assert!(session.open_packages().is_empty());
}

#[test]
fn opening_another_package_preserves_edits_selection_and_undo() {
    let mut session = Session::new(Package::new("First"));
    execute(&mut session, "file.new", &json!({"template": "empty", "name": "Source"})).unwrap();
    let added = execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    let history = session.history();
    let selection = session.selection.clone();
    let original = file_cmds::save_bytes(&session).unwrap();
    let bytes = formats::sbs::write_sbs(&Package::new("Second")).unwrap();

    file_cmds::open_bytes(&mut session, &bytes, Some("second.sbs".into())).unwrap();

    assert_eq!(inspect::session(&session, None)["documents"].as_array().unwrap().len(), 2);
    execute(&mut session, "package.activate", &json!({"document": 0})).unwrap();
    // Other open packages extend the runtime dependency registry without editing this document
    assert_eq!(file_cmds::save_bytes(&session).unwrap(), original);
    assert_eq!(session.selection, selection);
    assert_eq!(session.history(), history);
    assert!(session.modified);

    execute(&mut session, "edit.undo", &json!({})).unwrap();
    assert!(session.graph().unwrap().nodes.iter().all(|node| node.id.0 != added["id"].as_u64().unwrap()));
}

#[test]
fn saving_a_new_file_preserves_its_identity_across_path_aliases() {
    let directory = std::env::temp_dir().join(format!("materialcraft-save-identity-{}", std::process::id()));
    let files = directory.join("files");
    std::fs::create_dir_all(&files).unwrap();
    // Unix aliases exercise the same mismatch as Windows short paths and canonical prefixes
    #[cfg(unix)]
    let requested = {
        let alias = directory.join("alias");
        std::os::unix::fs::symlink(&files, &alias).unwrap();
        alias.join("source.sbs")
    };
    #[cfg(not(unix))]
    let requested = files.join("source.sbs");

    let mut session = Session::new(Package::new("Source"));
    execute(&mut session, "file.new", &json!({"template": "empty", "name": "Source"})).unwrap();
    let saved = execute(&mut session, "file.save", &json!({"path": requested})).unwrap();
    let canonical = requested.canonicalize().unwrap();
    assert_eq!(saved["path"], json!(canonical));
    assert_eq!(session.path.as_deref(), canonical.to_str());

    let document = session.document_id;
    execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    let edited = session.pkg.clone();
    let history = session.history();
    for path in [&requested, &canonical] {
        file_cmds::open_file(&mut session, path.to_str().unwrap()).unwrap();
        assert_eq!(session.document_id, document);
        assert_eq!(session.open_packages().len(), 1);
        assert_eq!(session.pkg, edited);
        assert_eq!(session.history(), history);
        assert!(session.modified);
        assert_eq!(session.path.as_deref(), canonical.to_str());
    }
    assert_eq!(execute(&mut session, "file.save", &json!({})).unwrap()["path"], saved["path"]);

    execute(&mut session, "file.newPackage", &json!({"name": "Other"})).unwrap();
    let bytes = std::fs::read(&canonical).unwrap();
    for path in [&requested, &canonical] {
        let error = execute(&mut session, "file.saveAs", &json!({"path": path})).unwrap_err();
        assert!(error.to_string().contains("another open package already uses this file"));
        assert_eq!(std::fs::read(&canonical).unwrap(), bytes);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn save_all_writes_every_modified_package_with_a_file_and_lists_the_rest() {
    let folder = std::env::temp_dir().join(format!("materialcraft-save-all-{}", std::process::id()));
    std::fs::create_dir(&folder).unwrap();
    let first_path = folder.join("first.sbs");
    let saved_path = folder.canonicalize().unwrap().join("first.sbs");
    let second_path = folder.join("second.sbs");

    let mut session = Session::new(Package::new("First"));
    execute(&mut session, "file.save", &json!({"path": first_path.to_str().unwrap()})).unwrap();
    execute(&mut session, "file.new", &json!({"template": "empty"})).unwrap();
    let first = session.document_id;

    execute(&mut session, "file.newPackage", &json!({"name": "Second"})).unwrap();
    execute(&mut session, "file.save", &json!({"path": second_path.to_str().unwrap()})).unwrap();
    let unchanged = std::fs::metadata(&second_path).unwrap().modified().unwrap();

    execute(&mut session, "file.newPackage", &json!({"name": "Unsaved"})).unwrap();
    execute(&mut session, "graph.node.add", &json!({"def": "uniform"})).unwrap();
    let active = session.document_id;

    let reply = execute(&mut session, "file.saveAll", &json!({})).unwrap();

    assert_eq!(reply["saved"], json!([saved_path.to_str().unwrap()]));
    assert_eq!(reply["withoutFile"], json!(["Unsaved"]));
    assert_eq!(reply["failed"], json!([]));
    assert_eq!(session.document_id, active, "the package that was active stays active");
    assert!(session.open_packages().iter().any(|entry| entry.0 == first && !entry.3), "the first package is saved");
    assert!(session.open_packages().iter().any(|entry| entry.0 == active && entry.3), "the package without a file keeps its edits");
    assert_eq!(std::fs::metadata(&second_path).unwrap().modified().unwrap(), unchanged, "an unmodified package is not written again");

    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_new_graph_can_start_its_own_package_and_take_any_size() {
    let mut session = Session::new(Package::new("Existing"));
    let existing = session.document_id;

    let reply = execute(&mut session, "file.new", &json!({"template": "pbr", "name": "Tall", "size": 512, "height": 2048, "newPackage": true})).unwrap();

    assert_eq!(reply["name"], "Tall");
    assert_ne!(session.document_id, existing);
    assert_eq!(session.open_packages().len(), 2);
    assert_eq!(session.graph().unwrap().size, [9, 11]);

    // A refused request opens nothing
    assert!(execute(&mut session, "file.new", &json!({"template": "invalid", "newPackage": true})).is_err());
    assert!(execute(&mut session, "file.new", &json!({"folder": "Missing", "newPackage": true})).is_err());
    assert_eq!(session.open_packages().len(), 2);
}

#[test]
fn a_refused_graph_leaves_the_active_package_unchanged() {
    let mut session = Session::new(Package::new("First"));
    let destination = session.document_id;
    execute(&mut session, "file.newPackage", &json!({"name": "Second"})).unwrap();
    let original = session.document_id;

    let refused = execute(&mut session, "file.new", &json!({"document": destination, "template": "invalid"}));

    assert!(refused.is_err());
    assert_eq!(session.document_id, original);
    assert!(session.package(destination).unwrap().graphs.is_empty());

    execute(&mut session, "file.new", &json!({"document": destination, "template": "empty", "name": "New graph"})).unwrap();

    assert_eq!(session.document_id, destination);
    assert_eq!(session.graph().unwrap().name, "New graph");
}

#[test]
fn a_new_package_resolves_reserved_graph_names() {
    for new_package in [true, false] {
        let mut session = Session::new(Package::new("Existing"));
        let original = session.document_id;
        if !new_package {
            execute(&mut session, "package.close", &json!({})).unwrap();
            assert!(session.open_packages().is_empty());
        }

        let mut parameters = json!({"template": "empty", "name": "Resources"});
        if new_package {
            parameters["newPackage"] = json!(true);
        }
        let reply = execute(&mut session, "file.new", &parameters).unwrap();

        assert_eq!(reply["name"], "Resources_1", "newPackage: {new_package}");
        assert_eq!(session.graph().unwrap().name, "Resources_1");
        assert!(session.pkg.folder_paths().contains("Resources"));
        assert_ne!(session.document_id, original);
        assert_eq!(session.open_packages().len(), if new_package { 2 } else { 1 });
    }
}
