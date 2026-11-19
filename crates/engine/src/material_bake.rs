//! Inspect and evaluate the published graph through the same CPU cook as an authoring document.
use std::collections::BTreeSet;

use craft_texture::Kind;
use materialcraft_core::{Graph, GraphIndex, NodeId};
use materialcraft_eval::{Cooked, outputs};
use materialcraft_formats::{Depth, encode};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};

use crate::commands::{CommandSpec, bad, has_graph};
use crate::{ExportRequest, Exported, MaterialOptions, Result, Session, cmd, graph_cmds::graph_index};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "material.info",
            "Inspect material",
            [],
            None,
            "{graph?} returns the material interface, published values, image inputs, outputs, presets and supported sizes",
            has_graph,
            |s, p| info(s, p)
        ),
        cmd!(
            "material.cook",
            "Cook material",
            [],
            None,
            "{graph?, size?, preset?, values?, inputs?} evaluates a variation and returns its output dimensions and means",
            has_graph,
            |s, p| cook(s, p, read_input)
        ),
    ]
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_input(path: &str) -> Result<Vec<u8>> {
    crate::linked_resource::read_regular(std::path::Path::new(path)).map_err(|error| bad("material.cook", format!("{path}: {error}")))
}

#[cfg(target_arch = "wasm32")]
pub fn read_input(_: &str) -> Result<Vec<u8>> {
    Err(bad("material.cook", "bind an image resource in the browser"))
}

pub fn info(s: &Session, p: &J) -> Result<J> {
    let index = graph_index(s, p)?;
    let graph = s.pkg.graph(index)?;
    let parameters: Vec<_> = graph
        .exposed
        .iter()
        .map(|param| {
            let mut json = json!(param);
            json["value"] = json!(graph.input_value(&param.id));
            json
        })
        .collect();
    Ok(json!({"version": materialcraft_formats::material::VERSION, "graph":graph.sbs_path(), "name":graph.display_name(),
        "size":graph.size_px(), "supportedSizes":(materialcraft_core::MIN_SIZE_LOG2..=materialcraft_core::MAX_SIZE_LOG2).map(|v| 1u32 << v).collect::<Vec<_>>(),
        "parameters":parameters,"presets":graph.presets,"inputs":crate::material_cmds::image_inputs(graph),
        "outputs":graph.nodes.iter().filter(|n|n.def=="output").map(|n|json!({"identifier":materialcraft_eval::identifier(n),"usage":materialcraft_eval::output_usage(n)})).collect::<Vec<_>>()
    }))
}

pub(crate) fn check_publish(s: &Session, index: usize) -> Result<()> {
    let graph = s.pkg.graph(index)?;
    if graph.function.is_some() {
        return Err(bad("file.exportMaterial", "select a material graph"));
    }

    let cooked = MaterialOptions::default().cook(s, index, None, Some(1 << materialcraft_core::MIN_SIZE_LOG2))?;
    if !outputs(graph, &cooked).into_iter().any(|(id, _, _, image)| image.is_some() && graph.connections.iter().any(|wire| wire.to == id)) {
        return Err(bad("file.exportMaterial", format!("graph `{}` has no connected, cookable material outputs", graph.sbs_path())));
    }

    Ok(())
}

pub fn cook(s: &Session, p: &J, read: impl Fn(&str) -> Result<Vec<u8>>) -> Result<J> {
    let index = graph_index(s, p)?;
    let graph = s.pkg.graph(index)?;
    let cooked = MaterialOptions::from_params(graph, p, read)?.cook(s, index, None, None)?;
    Ok(json!({"graph":graph.sbs_path(),"ms":cooked.ms,
        "outputs":outputs(graph,&cooked).into_iter().filter_map(|(_,usage,id,image)| {
            let image = image?;
            Some(json!({"identifier":id,"usage":usage,"size":[image.width(),image.height()],"mean":image.mean()}))
        }).collect::<Vec<_>>() }))
}

/// Encode every file before the caller writes to its destination. Paths in the report are relative.
pub fn export(s: &Session, p: &J, read: impl Fn(&str) -> Result<Vec<u8>>) -> Result<(Vec<Exported>, J)> {
    let option = |key| p.get(key).filter(|value| !value.is_null());
    let request = ExportRequest::from_params(s, p)?;
    let graph = s.pkg.graph(request.graph)?;
    let selected = request.outputs.as_deref();
    let materialx = match option("profile") {
        Some(value) if value == "materialx" => true,
        Some(value) if value == "maps" => false,
        Some(value) => return Err(bad("file.export", format!("unknown export profile {value}"))),
        None => {
            if option("normalFormat").is_some() || option("displacementScale").is_some() {
                return Err(bad("file.export", "normalFormat and displacementScale require an explicit export profile"));
            }

            let options = MaterialOptions::from_params(graph, p, read)?;
            let cooked = options.cook(s, request.graph, selected, None)?;
            return Ok((crate::file_cmds::encode_outputs(graph, &cooked, &request)?, json!({})));
        }
    };

    if option("format").is_some() || option("pattern").is_some() {
        return Err(bad("file.export", "named profiles choose their own format and filenames; omit format and pattern"));
    }

    let scale = match option("displacementScale") {
        Some(_) if !materialx => return Err(bad("file.export", "`displacementScale` requires the materialx profile")),
        Some(value) => {
            value.as_f64().map(|x| x as f32).filter(|x| x.is_finite() && *x >= 0.0).ok_or_else(|| bad("file.export", "invalid displacementScale"))?
        }
        None => 1.0,
    };
    let normal_y_down = match option("normalFormat") {
        None => None,
        Some(value) if value == "opengl" => Some(false),
        Some(value) if value == "directx" => Some(true),
        Some(_) => return Err(bad("file.export", "normalFormat must be opengl or directx")),
    };

    let options = MaterialOptions::from_params(graph, p, read)?;
    let cooked = options.cook(s, request.graph, selected, None)?;
    let (files, mut report) = encode_maps(graph, &cooked, &request, materialx, scale, normal_y_down)?;

    // Report the same graph selected by the legacy numeric coercion
    let mut interface = info(s, &json!({"graph": request.graph}))?;
    if let Some(params) = interface["parameters"].as_array_mut() {
        for parameter in params {
            if let Some(value) = parameter["id"].as_str().and_then(|id| options.values.get(id)) {
                parameter["value"] = json!(value);
            }
        }
    }

    report["material"] = interface;
    Ok((files, report))
}

fn shader_input(usage: &str) -> Option<&'static str> {
    Some(match usage {
        "baseColor" | "diffuse" => "base_color",
        "roughness" => "specular_roughness",
        "metallic" => "base_metalness",
        "normal" => "geometry_normal",
        "emissive" => "emission_color",
        "opacity" => "geometry_opacity",
        "height" => "height",
        "coatOpacity" => "coat_weight",
        "coatRoughness" => "coat_roughness",
        "transmissive" => "transmission_weight",
        "scattering" => "subsurface_weight",
        "sheenOpacity" => "fuzz_weight",
        "sheenColor" => "fuzz_color",
        "sheenRoughness" => "fuzz_roughness",
        "anisotropyLevel" => "specular_roughness_anisotropy",
        _ => return None,
    })
}

/// Direct normal generators expose their evaluated convention. Raw images default to OpenGL.
fn normal_y_down(index: &GraphIndex<'_>, cooked: &Cooked, output: NodeId) -> bool {
    let mut source = output;
    for _ in 0..cooked.nodes.len() {
        let Some(node) = index.input_of(source, "input").and_then(|wire| index.node(wire.from)) else { break };
        if node.def == "normal" && !node.disabled {
            return cooked.nodes.get(&node.id).and_then(|result| result.parameters.get("opengl")).and_then(materialcraft_core::Value::as_bool) == Some(false);
        }

        if node.def != "dot" {
            break;
        }

        source = node.id;
    }

    false
}

fn encode_maps(
    graph: &Graph,
    cooked: &Cooked,
    request: &ExportRequest,
    materialx: bool,
    scale: f32,
    source_y_down: Option<bool>,
) -> Result<(Vec<Exported>, J)> {
    let index = GraphIndex::new(graph);
    let mut files = Vec::new();
    let mut maps = Vec::new();
    let mut textures = Vec::new();
    let mut unmapped = Vec::new();
    let mut names = BTreeSet::new();
    let mut shader_inputs = BTreeSet::new();
    for (id, usage, identifier, image) in outputs(graph, cooked) {
        let Some(image) = image.filter(|_| request.includes(&identifier) && !index.incoming(id).is_empty()) else { continue };

        let shader = shader_input(usage);
        let color = materialcraft_formats::material::srgb_usage(usage);
        let space = request.color_space(&identifier, usage);
        if materialx && shader.is_some() && !color && space.encodes_srgb() {
            return Err(bad("file.export", format!("OpenPBR requires linear data for `{identifier}`; select raw or linear")));
        }

        let height = materialx && usage == "height";

        // MaterialX takes signed scene-unit displacement and tangent-space Y-up normals
        let converted;
        let image = if height {
            converted = image.map(Kind::Gray, |pixel| [(pixel[0] - 0.5) * scale, 0.0, 0.0, 1.0]);
            &converted
        } else if matches!(usage, "normal" | "coatNormal") && source_y_down.unwrap_or_else(|| normal_y_down(&index, cooked, id)) {
            converted = image.map(Kind::Color, |[r, g, b, a]| [r, 1.0 - g, b, a]);
            &converted
        } else {
            image
        };

        let float = height || request.depth == Some(Depth::F32) || image.data().iter().any(|value| !(0.0..=1.0).contains(value));
        if float && request.color_spaces.get(&identifier).is_some_and(|space| space.encodes_srgb()) {
            return Err(bad("file.export", format!("EXR output `{identifier}` requires linear values; omit the sRGB override or select raw or linear")));
        }

        let (ext, depth) = if float {
            ("exr", Depth::F32)
        } else {
            let depth = request.depth.unwrap_or(if color { Depth::U8 } else { Depth::U16 });
            ("png", depth)
        };
        let stem = materialcraft_formats::safe_file_name(&identifier);
        if !names.insert(format!("{stem}.{ext}").to_lowercase()) {
            return Err(bad("file.export", format!("duplicate output filename `{stem}.{ext}`; give each output a unique identifier")));
        }

        let srgb = !float && space.encodes_srgb();
        let bytes = encode(image, ext, depth, srgb)?;
        let file = if materialx {
            // Keep every texture referenced by the last complete document until the new write succeeds
            const MAX_STEM_CHARS: usize = 32;
            let digest = Sha256::new().chain_update(stem.as_bytes()).chain_update([0]).chain_update(&bytes).finalize();
            let prefix: String = stem.chars().take(MAX_STEM_CHARS).collect();
            format!("{prefix}-{digest:x}.{ext}")
        } else {
            format!("{stem}.{ext}")
        };
        maps.push(json!({"identifier":identifier,"usage":usage,"file":file,"size":[image.width(),image.height()],"colorSpace":if srgb {"srgb"}else{"linear"}}));
        if materialx {
            if let Some(input) = shader.filter(|i| shader_inputs.insert(*i)) {
                textures.push(craft_materialx::Texture {
                    input: input.into(),
                    file: file.clone(),
                    colorspace: if srgb { "srgb_texture" } else { "lin_rec709" }.into(),
                });
            } else {
                unmapped.push(identifier);
            }
        }
        files.push(Exported { file, bytes });
    }

    if files.is_empty() {
        return Err(bad("file.export", "no connected image outputs were selected"));
    }

    if materialx {
        if textures.is_empty() {
            return Err(bad("file.export", format!("no selected output maps to OpenPBR: {}", unmapped.join(", "))));
        }

        let doc = craft_materialx::write_open_pbr(&graph.name, &textures, None);
        files.push(Exported { file: "material.mtlx".into(), bytes: doc.into_bytes() });
    }

    Ok((files, json!({"maps":maps,"unmapped":unmapped,"normalOrientation":"Y+","displacementScale":scale})))
}
