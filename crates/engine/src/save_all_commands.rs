//! File ▸ Save all: write every open package that has unsaved edits to its own file

use crate::commands::CommandSpec;

#[cfg(not(target_arch = "wasm32"))]
use crate::commands::has_package;
#[cfg(not(target_arch = "wasm32"))]
use crate::{Result, Session, cmd};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::{Value, json};

/// Register the command that saves every modified package
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "file.saveAll",
        "Save all",
        ["File"],
        None,
        "{} saves every modified package that has a file; returns the saved paths, the packages that still need Save as (no file yet), and any that failed",
        has_package,
        save_all
    )]
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn specs() -> Vec<CommandSpec> {
    Vec::new()
}

/// Save each modified package in turn, then make the package that was active the active one again
#[cfg(not(target_arch = "wasm32"))]
fn save_all(session: &mut Session, _parameters: &Value) -> Result<Value> {
    // Publishing the active edit can mark linked packages modified
    session.end_group();
    let active = session.document_id;
    let modified: Vec<(u64, String, Option<String>)> = session
        .open_packages()
        .into_iter()
        .filter(|(_, _, _, modified)| *modified)
        .map(|(id, package, path, _)| (id, package.name.clone(), path.map(str::to_string)))
        .collect();

    let (mut saved, mut without_file, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for (document, name, path) in modified {
        // A package from a library alias or with no file yet needs a name chosen in Save as
        let Some(path) = path.filter(|path| !path.contains("://")) else {
            without_file.push(name);
            continue;
        };

        let result = session.activate_document(document).and_then(|()| crate::file_cmds::save_to_path(session, &path));
        match result {
            Ok(reply) => saved.push(reply["path"].clone()),
            Err(error) => failed.push(json!({"package": name, "error": error.to_string()})),
        }
    }

    if session.document_id != active {
        session.activate_document(active)?;
    }

    Ok(json!({"saved": saved, "withoutFile": without_file, "failed": failed}))
}
