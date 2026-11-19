use crate::commands::{CommandSpec, bad, has_graph, strp};
use crate::{Result, Session, cmd};
use materialcraft_core::Graph;
use materialcraft_formats::material::ImageInput;
#[cfg(not(target_arch = "wasm32"))]
use materialcraft_formats::material::MATERIAL_EXT;
use serde_json::{Value as J, json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "file.exportMaterial",
            "Export material...",
            ["File"],
            None,
            "{path, graph? (index or SBS path)} publishes one root graph with its source dependencies in a v3 .mcmat",
            has_graph,
            export_material
        ),
        cmd!(
            "file.sendToTextureCraft",
            "Send to TextureCraft",
            ["File"],
            None,
            "{graph?} exports the graph as a portable material into TextureCraft's library folder (Documents/TextureCraft/library), where its shelf lists it",
            has_graph,
            send_to_texturecraft
        ),
        cmd!(
            "graph.input.bind",
            "Set input preview image",
            [],
            None,
            "{identifier, resource? | path?, linear?} shows a bitmap resource (or an image file, embedded) on the graph's input node `identifier`, e.g. a mesh map baked in TextureCraft",
            has_graph,
            bind
        ),
        cmd!("graph.input.unbind", "Clear input preview image", [], None, "{identifier}", has_graph, unbind),
    ]
}

/// Publish a self-contained source snapshot after checking the selected outputs can cook.
pub fn material_bytes(session: &Session, graph: usize) -> Result<Vec<u8>> {
    crate::material_bake::check_publish(session, graph)?;
    Ok(materialcraft_formats::material::write_material(&session.pkg, graph)?)
}

/// Publish with an additional budget for the combined uncompressed source assets.
pub fn material_bytes_with_resource_limit(session: &Session, graph: usize, maximum_resource_bytes: u64) -> Result<Vec<u8>> {
    crate::material_bake::check_publish(session, graph)?;
    Ok(materialcraft_formats::material::write_material_with_resource_limit(&session.pkg, graph, maximum_resource_bytes)?)
}

pub fn publish_graph(s: &Session, p: &J) -> Result<usize> {
    if ["size", "values", "inputs", "preset"].iter().any(|key| p.get(key).is_some_and(|v| !v.is_null())) {
        return Err(bad("file.exportMaterial", "pass size, values, preset and image inputs when cooking or exporting outputs"));
    }

    crate::graph_cmds::graph_index(s, p)
}

/// The graph's input nodes (one entry per identifier)
pub fn image_inputs(g: &Graph) -> Vec<ImageInput> {
    let mut out: Vec<ImageInput> = Vec::new();
    for n in g.nodes.iter().filter(|n| matches!(n.def.as_str(), "input_grayscale" | "input_color")) {
        let identifier = materialcraft_eval::identifier(n);
        if !out.iter().any(|i| i.identifier == identifier) {
            let usage = if n.params.contains_key("usage") { materialcraft_eval::output_usage(n) } else { "any" };
            out.push(ImageInput { identifier, kind: if n.def == "input_grayscale" { "gray" } else { "color" }.into(), usage: usage.into() });
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn export_material(s: &mut Session, p: &J) -> Result<J> {
    const CMD: &str = "file.exportMaterial";
    let path = strp(p, "path").ok_or_else(|| bad(CMD, "`path` is required"))?;
    let path = if path.to_ascii_lowercase().ends_with(&format!(".{MATERIAL_EXT}")) { path.to_string() } else { format!("{path}.{MATERIAL_EXT}") };
    let bytes = material_bytes(s, publish_graph(s, p)?)?;
    materialcraft_formats::write_atomic(std::path::Path::new(&path), &bytes)?;
    s.log(format!("Exported material {path}"));
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

#[cfg(target_arch = "wasm32")]
fn export_material(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.exportMaterial", "the browser exports with a download"))
}

/// TextureCraft's library folder: `Documents/TextureCraft/library` in the home folder
#[cfg(not(target_arch = "wasm32"))]
pub fn texturecraft_library() -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("Documents").join("TextureCraft").join("library")
}

#[cfg(not(target_arch = "wasm32"))]
fn send_to_texturecraft(s: &mut Session, p: &J) -> Result<J> {
    const CMD: &str = "file.sendToTextureCraft";
    let gi = publish_graph(s, p)?;
    let bytes = material_bytes(s, gi)?;
    let dir = texturecraft_library();
    std::fs::create_dir_all(&dir).map_err(|e| bad(CMD, format!("{}: {e}", dir.display())))?;
    let path = dir.join(format!("{}.{MATERIAL_EXT}", materialcraft_formats::safe_file_name(&s.pkg.graph(gi)?.name)));
    materialcraft_formats::write_atomic(&path, &bytes)?;
    let path = path.display().to_string();
    s.log(format!("Sent to TextureCraft: {path}"));
    Ok(json!({"path": path}))
}

#[cfg(target_arch = "wasm32")]
fn send_to_texturecraft(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("file.sendToTextureCraft", "the browser has no shared library folder: export the material instead"))
}

fn identifier_param(s: &Session, cmd: &str, p: &J) -> Result<String> {
    let id = strp(p, "identifier").ok_or_else(|| bad(cmd, "`identifier` is required"))?;
    if !image_inputs(s.graph()?).iter().any(|i| i.identifier == id) {
        return Err(bad(cmd, format!("the graph has no input node `{id}`")));
    }
    Ok(id.to_string())
}

fn bind(s: &mut Session, p: &J) -> Result<J> {
    const CMD: &str = "graph.input.bind";
    let id = identifier_param(s, CMD, p)?;
    let file = match (strp(p, "resource"), strp(p, "path")) {
        (Some(r), _) if s.pkg.resource(r).is_some() => None,
        (Some(r), _) => return Err(bad(CMD, format!("no bitmap resource `{r}`"))),
        (None, Some(path)) => Some(read_image_file(CMD, path)?),
        (None, None) => return Err(bad(CMD, "give a `resource` or a `path`")),
    };
    s.edit("Set input preview image", |s| {
        let res = match file {
            Some((name, bytes)) => crate::file_cmds::push_resource(&mut s.pkg, &name, bytes, Some(&id)),
            None => strp(p, "resource").unwrap_or_default().to_string(),
        };
        let linear = p.get("linear").and_then(J::as_bool).unwrap_or(false);
        let g = s.graph_mut()?;
        g.input_images.insert(id.clone(), res.clone());
        if linear {
            g.input_images_linear.insert(id.clone());
        } else {
            g.input_images_linear.remove(&id);
        }
        Ok(json!({"identifier": id, "resource": res}))
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn read_image_file(cmd: &str, path: &str) -> Result<(String, Vec<u8>)> {
    let bytes = std::fs::read(path).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
    materialcraft_formats::decode(&bytes, false)?;
    let name = std::path::Path::new(path).file_name().and_then(|f| f.to_str()).unwrap_or("input.png").to_string();
    Ok((name, bytes))
}

#[cfg(target_arch = "wasm32")]
fn read_image_file(cmd: &str, _: &str) -> Result<(String, Vec<u8>)> {
    Err(bad(cmd, "the browser imports images with the file picker: import a bitmap, then pass its `resource`"))
}

fn unbind(s: &mut Session, p: &J) -> Result<J> {
    let id = strp(p, "identifier").ok_or_else(|| bad("graph.input.unbind", "`identifier` is required"))?.to_string();
    s.edit("Clear input preview image", |s| {
        s.graph_mut()?.input_images_linear.remove(&id);
        Ok(J::Bool(s.graph_mut()?.input_images.remove(&id).is_some()))
    })
}

#[cfg(test)]
#[path = "material_cmds.test.rs"]
mod tests;
