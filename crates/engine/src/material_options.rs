use craft_texture::Image;
use materialcraft_core::{BaseParameter, Graph, SizeMode, Value};
use materialcraft_eval::{Cooked, Options};
use serde_json::Value as J;
use std::{borrow::Cow, collections::BTreeMap};

use crate::commands::bad;
use crate::{ExportSize, Result, Session};

/// Per-invocation inputs. The authoring graph and the source archive retain their own values.
#[derive(Clone, Debug, Default)]
pub struct MaterialOptions {
    pub size: Option<ExportSize>,
    pub values: BTreeMap<String, Value>,
    pub inputs: BTreeMap<String, Image>,
}

impl MaterialOptions {
    pub fn from_params(graph: &Graph, p: &J, read: impl Fn(&str) -> Result<Vec<u8>>) -> Result<Self> {
        let invalid = |msg| bad("material.cook", msg);
        let mut options = Self { size: crate::export_request::export_size("material.cook", p.get("size"))?, ..Default::default() };

        if let Some(value) = p.get("preset").filter(|v| !v.is_null()) {
            let name = value.as_str().ok_or_else(|| invalid("preset must be a name".into()))?;
            let preset = graph.presets.iter().find(|preset| preset.name == name).ok_or_else(|| invalid(format!("unknown preset `{name}`")))?;
            options.values.extend(
                preset
                    .values
                    .iter()
                    .filter(|(id, value)| crate::graph_cmds::preset_value_matches(graph, id, value))
                    .map(|(id, value)| (id.clone(), value.clone())),
            );
        }

        if let Some(value) = p.get("values").filter(|v| !v.is_null()) {
            for (id, v) in value.as_object().ok_or_else(|| invalid("values must map parameter identifiers to values".into()))? {
                let param = graph.exposed.iter().find(|e| e.id == *id).ok_or_else(|| invalid(format!("unknown exposed parameter `{id}`")))?;
                let value = param.default.coerce_json(v).ok_or_else(|| invalid(format!("`{id}` takes a {}", param.default.type_name())))?;
                options.values.insert(id.clone(), value);
            }
        }

        if let Some(value) = p.get("inputs").filter(|v| !v.is_null()) {
            let declared = crate::material_cmds::image_inputs(graph);
            const MAX_DECODED_INPUT_BYTES: usize = 1 << 30;
            let mut remaining = MAX_DECODED_INPUT_BYTES;
            for (id, path) in value.as_object().ok_or_else(|| invalid("inputs must map image identifiers to file paths".into()))? {
                let input = declared.iter().find(|input| input.identifier == *id).ok_or_else(|| invalid(format!("unknown image input `{id}`")))?;
                let linear = if graph.input_images.contains_key(id) {
                    graph.input_images_linear.contains(id)
                } else {
                    let usage = if input.usage == "any" { id } else { &input.usage };
                    !materialcraft_formats::material::srgb_usage(usage)
                };
                let path = path.as_str().ok_or_else(|| invalid(format!("inputs.{id} must be an image path")))?;
                let image = materialcraft_formats::decode(&read(path)?, linear)?;
                remaining = remaining.checked_sub(std::mem::size_of_val(image.data())).ok_or_else(|| invalid("decoded image inputs exceed 1 GiB".into()))?;
                options.inputs.insert(id.clone(), image);
            }
        }

        Ok(options)
    }

    pub(crate) fn cook(&self, session: &Session, index: usize, selected: Option<&[String]>, max_size: Option<u32>) -> Result<Cooked> {
        let graph = session.pkg.graph(index)?;
        let roots = graph
            .nodes
            .iter()
            .filter(|node| node.def == "output")
            .filter(|node| selected.is_none_or(|ids| ids.contains(&materialcraft_eval::identifier(node))))
            .map(|node| node.id)
            .collect();
        let mut package = Cow::Borrowed(&session.pkg);
        if let Some(size) = self.size {
            let graph = package.to_mut().graph_mut(index)?;
            graph.size = size.log2_size(graph.size);

            // Fixed-size generators keep their native detail while final output bridges resample to the requested size
            for output in graph.nodes.iter_mut().filter(|node| node.def == "output") {
                output.base.size_mode = SizeMode::RelativeToParent;
                output.base.size = [0, 0];
                output.base.functions.remove(&BaseParameter::OutputSize);
            }
        }

        let decoder = session.resource_decoder();
        let mut options = Options::new(&decoder);
        options.environment = session.environment;
        options.max_size = max_size;
        options.only = Some(roots);
        options.inputs.clone_from(&self.inputs);
        options.overrides.clone_from(&self.values);

        let cooked = materialcraft_eval::cook(&package, index, &options, &session.cache);
        if let Some((id, error)) = cooked.nodes.iter().find_map(|(id, r)| Some((id, r.error.as_ref()?))) {
            return Err(bad("material.cook", format!("graph `{}`, node {}: {error}", graph.sbs_path(), id.0)));
        }

        Ok(cooked)
    }
}
