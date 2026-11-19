use crate::{Cache, Cooked, Options, now_ms};
use crate::{node_execution::cook_node, prepared_graph::PreparedGraph};
use craft_texture::Image;
use materialcraft_core::{Graph, Node, Package, Value};
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::Ordering},
};

pub fn cook(package: &Package, graph_index: usize, options: &Options, cache: &Mutex<Cache>) -> Cooked {
    let started_ms = now_ms();
    let mut cooked = Cooked::default();
    let Some(graph) = package.graphs.get(graph_index) else { return cooked };

    let mut previews = BTreeMap::new();
    let prepared = PreparedGraph::new(package, graph, options);

    // Exposed parameter values: the graph's own, then the caller's overrides
    let mut variables: BTreeMap<String, Value> =
        graph.exposed.iter().filter_map(|exposed| graph.input_value(&exposed.id).map(|value| (exposed.id.clone(), value.clone()))).collect();
    for (name, value) in &options.overrides {
        variables.insert(name.clone(), value.clone());
    }
    variables.insert("$time".into(), Value::Float(options.environment.time_seconds));
    variables.insert("$uvtile".into(), Value::Int2(options.environment.uv_tile));
    variables.insert("$normalformat".into(), Value::Int(options.environment.normal_format));
    variables.insert("$physicalsize".into(), Value::Float3(graph.physical_size));

    for &id in &prepared.order {
        if options.cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
            cooked.cancelled = true;
            break;
        }

        let Some(node) = prepared.index.node(id) else { continue };

        let input = input_image(graph, node, options, &mut previews);
        let result = cook_node(package, graph_index, &prepared, node, &cooked, &variables, options, input, cache);
        cooked.nodes.insert(id, result);
    }

    cooked.ms = (now_ms() - started_ms) as f32;
    cooked
}

fn input_image<'a>(graph: &Graph, node: &Node, options: &'a Options<'_>, previews: &'a mut BTreeMap<String, Option<Image>>) -> Option<&'a Image> {
    if !node.def.starts_with("input_") {
        return None;
    }

    let identifier = crate::identifier(node);
    if let Some(image) = options.inputs.get(&identifier) {
        return Some(image);
    }

    let resource = graph.input_images.get(&identifier)?;
    let linear = graph.input_images_linear.contains(&identifier);

    // Nodes sharing an input also share its first lookup, including a missing preview
    previews.entry(identifier).or_insert_with(|| (options.resource)(&craft_texture::resource_key(resource, linear))).as_ref()
}
