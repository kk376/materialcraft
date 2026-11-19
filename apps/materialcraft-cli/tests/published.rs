//! Exercise the distribution workflow through the public CLI, including reopening the archive in fresh processes.
use std::path::Path;
use std::process::Command;

use materialcraft_engine::formats;
use serde_json::{Value, json};

fn run(dir: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_materialcraft-cli")).current_dir(dir).args(args).output().unwrap();
    assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn a_published_graph_bakes_openpbr_variations_after_the_project_is_removed() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    run(dir, &["new", "pbr", "source.sbs", "--name", "Oak", "--size", "32"]);
    run(dir, &["run", "source.sbs", "graph.exposed.add", r#"{"id":"tint","bind":{"node":1,"param":"color"}}"#, "-o", "studio.mcraft"]);
    run(dir, &["publish", "studio.mcraft", "oak.mcmat", "--graph", "Oak"]);
    std::fs::remove_file(dir.join("studio.mcraft")).unwrap();
    std::fs::remove_file(dir.join("source.sbs")).unwrap();

    let invalid_scale = Command::new(env!("CARGO_BIN_EXE_materialcraft-cli"))
        .current_dir(dir)
        .args(["export", "oak.mcmat", "invalid", "--profile", "materialx", "--displacement-scale", "NaN"])
        .output()
        .unwrap();

    assert!(!invalid_scale.status.success(), "an invalid displacement scale produced a material");
    assert!(String::from_utf8_lossy(&invalid_scale.stderr).contains("displacement-scale"));
    assert!(!dir.join("invalid").exists());

    let initial = run(dir, &["info", "oak.mcmat"]);
    assert_eq!(initial["parameters"][0]["id"], "tint");
    for (folder, size, tint) in [("small", 32, "#ff0000"), ("large", 64, "#0000ff")] {
        let report =
            run(dir, &["export", "oak.mcmat", "--out", folder, "--profile", "materialx", "--size", &size.to_string(), "--set", &format!("tint={tint}")]);
        let maps = report["maps"].as_array().unwrap();
        assert_eq!(maps.len(), 6, "every output of the material template is exported");
        let base = maps.iter().find(|m| m["usage"] == "baseColor").unwrap();
        let relative = base["file"].as_str().unwrap();
        assert_eq!(base["size"], json!([size, size]));
        let root = dir.join(folder);
        let xml = std::fs::read_to_string(root.join("material.mtlx")).unwrap();
        assert!(xml.contains("<open_pbr_surface") && xml.contains("<normalmap") && xml.contains("<displacement"));
        assert!(xml.contains(&format!("value=\"{relative}\"")) && !xml.contains(dir.to_string_lossy().as_ref()));
        let map = formats::decode(&std::fs::read(root.join(relative)).unwrap(), false).unwrap();
        let pixel = map.get(0, 0, false);
        let expected = if size == 32 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
        assert_eq!(&pixel[..3], &expected);
        let height = maps.iter().find(|m| m["usage"] == "height").unwrap()["file"].as_str().unwrap();
        let displacement = formats::decode(&std::fs::read(root.join(height)).unwrap(), true).unwrap();
        assert!((displacement.get(0, 0, false)[0] + 0.5).abs() < 1e-5);
        assert!(report["unmapped"].as_array().unwrap().iter().any(|id| id == "ambientocclusion"));
    }
    let rectangular = run(dir, &["cook", "oak.mcmat", "--size", "64x32"]);
    assert_eq!(rectangular["outputs"][0]["size"], json!([64, 32]));
    assert_eq!(run(dir, &["info", "oak.mcmat"]), initial);
}

#[test]
fn failed_materialx_reexport_preserves_the_last_complete_material() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    run(dir, &["new", "pbr", "source.sbs", "--name", "Oak", "--size", "32"]);
    let export = |folder, size| run(dir, &["export", "source.sbs", folder, "--profile", "materialx", "--size", size]);
    let first = export("material", "32");
    let added = run(dir, &["run", "source.sbs", "graph.node.add", r#"{"def":"output","params":{"identifier":"extra"}}"#, "-o", "source.sbs"]);
    let wire = json!({"from":1,"to":added["id"],"toPort":"input"}).to_string();
    run(dir, &["run", "source.sbs", "graph.connect", &wire, "-o", "source.sbs"]);
    let next = export("reference", "64");
    let material = dir.join("material");
    let original_files: Vec<_> = first["maps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|map| {
            let name = map["file"].as_str().unwrap();
            (name, std::fs::read(material.join(name)).unwrap())
        })
        .collect();
    let document = std::fs::read(material.join("material.mtlx")).unwrap();
    let extra = next["maps"].as_array().unwrap().iter().find(|map| map["identifier"] == "extra").unwrap();
    let blocked = material.join(extra["file"].as_str().unwrap());
    std::fs::create_dir(&blocked).unwrap();

    let failed = Command::new(env!("CARGO_BIN_EXE_materialcraft-cli"))
        .current_dir(dir)
        .args(["export", "source.sbs", "material", "--profile", "materialx", "--size", "64"])
        .output()
        .unwrap();

    assert!(!failed.status.success());
    let error = String::from_utf8_lossy(&failed.stderr);
    assert!(error.contains(blocked.file_name().unwrap().to_str().unwrap()), "{error}");
    assert_eq!(std::fs::read(material.join("material.mtlx")).unwrap(), document);
    for (name, bytes) in original_files {
        assert_eq!(std::fs::read(material.join(name)).unwrap(), bytes, "{name}");
    }
}
