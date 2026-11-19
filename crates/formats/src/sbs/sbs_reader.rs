use super::base_parameters::read_base_params;
use super::component_usage;
use super::connection::connect_all;
use super::constant::{MAX_NODES, MAX_WIRES};
use super::constant::{NODE_HALF_SIZE, SELF_DEPENDENCY};
use super::embedded::{decode_embedded, decode_embedded_bounded};
use super::embedded_error::EmbeddedError;
use super::filter_map::{BASE_PARAMS, FilterMap, HANDLED_PARAMS, filter_by_sbs_name};
use super::function_graph;
use super::input_bridge::InputBridge;
use super::instance_interface::MAX_INPUTS;
use super::placeholder::{add_placeholder_input, fill_placeholder, placeholder_outputs};
use super::read_node::ReadNode;
use super::sbs_value::{SbsValue, exposed_default, sbs_constant, to_natural_value, to_param_value};
use super::type_code;
use super::warnings::Warnings;
use super::xml::{child_element, child_value, children_named, parse_floats};
use materialcraft_core::{
    BaseParameter, ComponentUsage, Connection, ExposedParameter, Graph, MAX_SIZE_LOG2, MIN_SIZE_LOG2, MISSING_NODE, Node, NodeId, OutputFormat, Package,
    ParameterInheritance, Resource, ResourceBytes, Value,
};
use materialcraft_nodes::find;
use roxmltree::Node as XmlNode;
use std::collections::{HashMap, HashSet};

pub(super) struct SbsReader<'text, 'warnings> {
    pub(super) inflation_work: std::cell::Cell<u64>,
    pub(super) resource_budget: Option<&'warnings std::cell::Cell<u64>>,
    pub(super) text: &'text str,
    pub(super) warnings: &'warnings mut Warnings,
    pub(super) dependencies: HashMap<String, String>,
    pub(super) graph_names: HashMap<String, String>,
    pub(super) resource_names: HashMap<String, String>,
    pub(super) external_resource_names: HashMap<(String, String), String>,
    pub(super) direct_resource_names: HashMap<String, String>,
    pub(super) used_resource_names: HashSet<String>,
    pub(super) node_count: usize,
    pub(super) wire_count: usize,
}

impl<'text> SbsReader<'text, '_> {
    pub(super) fn collect_items<'a>(
        &mut self,
        content: XmlNode<'a, 'text>,
        folder: &str,
        pkg: &mut Package,
        graphs: &mut Vec<(String, XmlNode<'a, 'text>)>,
        linked: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) {
        let mut pending = vec![(folder.to_string(), content)];
        while let Some((folder, content)) = pending.pop() {
            for item in content.children().filter(|child| child.is_element()) {
                let identifier = child_value(item, "identifier").unwrap_or("").trim().to_string();
                let path = if folder.is_empty() { identifier.clone() } else { format!("{folder}/{identifier}") };

                match item.tag_name().name() {
                    "graph" | "function" => graphs.push((path, item)),
                    "group" => {
                        if identifier.is_empty() || identifier.contains('/') {
                            self.warnings.add(format!("invalid folder `{path}` was flattened"));
                            if let Some(group_content) = child_element(item, "content") {
                                pending.push((folder.clone(), group_content));
                            }
                            continue;
                        }

                        if path.split('/').count() > 64 {
                            self.warnings.add(format!("excessively nested folder `{path}` was skipped"));
                            continue;
                        }

                        pkg.folders.push(path.clone());
                        if let Some(description) = child_value(item, "desc").filter(|text| !text.is_empty()) {
                            pkg.folder_descriptions.insert(path.clone(), description.into());
                        }

                        if let Some(group_content) = child_element(item, "content") {
                            pending.push((path, group_content));
                        }
                    }
                    "resource" => {
                        let name = self.read_resource(item, &identifier, &path, pkg, linked);
                        self.resource_names.insert(path, name);
                    }
                    "mdl_graph" | "mdlGraph" => self.warnings.add(format!("MDL graph `{path}` is not supported")),
                    other => self.warnings.add(format!("package item <{other}> `{path}` is not supported")),
                }
            }
        }
    }

    pub(super) fn read_resource(&mut self, item: XmlNode, identifier: &str, path: &str, pkg: &mut Package, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> String {
        let kind = child_value(item, "type").unwrap_or("bitmap");
        let format = child_value(item, "format").unwrap_or("png").trim().to_ascii_lowercase();
        let source = child_element(item, "source");
        let external_copy = source.and_then(|source| child_element(source, "externalcopy")).and_then(|copy| child_value(copy, "filename"));
        let fallback_filename = format!("{identifier}.{format}");
        if let Some(target) =
            [external_copy, child_value(item, "filepath"), Some(fallback_filename.as_str())].into_iter().flatten().find_map(super::resource_replacement::target)
        {
            return super::resource_replacement::load(self, pkg, identifier, path, &target, linked);
        }

        let external = source
            .and_then(|source| child_element(source, "externalcopy"))
            .and_then(|copy| child_value(copy, "filename"))
            .filter(|path| !path.is_empty())
            .or_else(|| child_value(item, "filepath"));
        let font = kind == "font" || super::resource_replacement::is_font(&fallback_filename) || external.is_some_and(super::resource_replacement::is_font);
        let mut bytes: Option<Vec<u8>> = None;
        let mut loaded_source = "";
        let mut why_missing = String::new();

        if let Some(embedded) = source.and_then(|s| child_element(s, "binembedded")) {
            let encoded = child_value(embedded, "strdata").unwrap_or("");
            let decoded = match self.resource_budget {
                Some(budget) => decode_embedded_bounded(encoded, budget.get(), &self.inflation_work),
                None => decode_embedded(encoded).map_err(EmbeddedError::Damaged),
            };
            match decoded {
                Ok(data) if !font || !data.is_empty() => {
                    if let Some(budget) = self.resource_budget {
                        // The decoder checked the retained length
                        budget.set(budget.get().saturating_sub(data.len() as u64));
                    }
                    bytes = Some(data);
                }
                Ok(_) => why_missing = "its embedded font data is empty".into(),
                Err(error @ (EmbeddedError::Budget | EmbeddedError::WorkBudget)) => {
                    // Reject a budget error to preserve inline asset precedence
                    self.warnings.add(format!("resource `{path}`: {error}"));
                    return unique_name(&mut self.used_resource_names, if identifier.is_empty() { "resource" } else { identifier });
                }
                Err(e) => why_missing = format!("its embedded data is damaged ({e})"),
            }
        }

        if bytes.is_none() {
            let external_copy = source.and_then(|s| child_element(s, "externalcopy")).and_then(|e| child_value(e, "filename"));
            for file in [external_copy, child_value(item, "filepath")].into_iter().flatten().filter(|p| !p.trim().is_empty()) {
                if file.contains("://") && !file.starts_with("sbs://") {
                    why_missing = format!("it links to `{file}`, which is not available");
                    continue;
                }

                match linked(file) {
                    Some(data) if !font || !data.is_empty() => {
                        bytes = Some(data);
                        loaded_source = file;
                        break;
                    }
                    Some(_) => why_missing = format!("its font file `{file}` is empty"),
                    None => why_missing = format!("its file `{file}` was not found next to the package"),
                }
            }
        }

        // Prefer supplied font bytes before trying the shared fallback
        if font && bytes.is_none() {
            if let Some(name) = super::resource_replacement::load_font(self, pkg, identifier, path, linked) {
                return name;
            }

            let why = if why_missing.is_empty() { "it has no font data" } else { &why_missing };
            why_missing = format!("{why}; fallback `{}` is unavailable", super::library::FALLBACK_FONT);
        }

        let name = unique_name(&mut self.used_resource_names, if identifier.is_empty() { "resource" } else { identifier });
        match bytes {
            Some(data) => {
                let ext = if kind == "svg" { "svg".to_string() } else { format };

                let mut resource = Resource::new(name.clone(), crate::safe_file_name(&format!("{name}.{ext}")), ResourceBytes::new(data));

                // Keep the link as well as the loaded bytes so library saves can retain shared assets
                resource.source = loaded_source.into();
                resource.folder = path.rsplit_once('/').map_or("", |(folder, _)| folder).into();
                pkg.resources.push(resource);
            }
            None => {
                let why = if why_missing.is_empty() { "it has no data".to_string() } else { why_missing };
                let extension = if kind == "svg" { "svg" } else { &format };
                let filename = format!("{name}.{extension}");
                let filename = if font {
                    external.and_then(|path| path.rsplit(['/', '\\']).next()).filter(|file| !file.is_empty()).unwrap_or(&filename)
                } else {
                    &filename
                };
                let mut resource = Resource::new(name.clone(), crate::safe_file_name(filename), ResourceBytes::new(Vec::new()));
                resource.source = external.unwrap_or("").into();
                resource.folder = path.rsplit_once('/').map_or("", |(folder, _)| folder).into();
                pkg.resources.push(resource);
                self.warnings.add(format!("resource `{path}` could not be loaded: {why}"));
            }
        }

        name
    }

    pub(super) fn resolve_reference(&self, url: &str) -> (String, Option<String>) {
        let url = url.trim();
        let rest = url.strip_prefix("pkg:///").or_else(|| url.strip_prefix("pkg://")).unwrap_or(url);
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let dependency = query.split('&').find_map(|kv| kv.strip_prefix("dependency=")).and_then(|uid| self.dependencies.get(uid).cloned());
        (path.trim_matches('/').to_string(), dependency)
    }

    pub(super) fn read_graph(&mut self, element: XmlNode<'_, 'text>, name: String) -> Graph {
        let mut graph = Graph::new(&name);
        let base = read_base_params(child_element(element, "baseParameters"), self.warnings);
        graph.format = if BaseParameter::Format.inheritance(&base) == ParameterInheritance::Absolute { base.format } else { OutputFormat::Inherit };
        graph.seed = base.seed;
        graph.pixel_ratio = base.pixel_ratio;
        graph.pixel_ratio_mode = base.pixel_ratio_mode;
        graph.tiling = base.tiling;
        graph.tiling_mode = base.tiling_mode;

        // The default parent size option ("11x11", log2 sizes)
        for option in child_element(element, "options").into_iter().flat_map(|o| children_named(o, "option")) {
            super::preview_binding::read(option, &mut graph);
            if child_value(option, "name") == Some("defaultParentSize")
                && let Some((width, height)) = child_value(option, "value").and_then(|v| v.split_once('x'))
            {
                let log2 = |s: &str| s.trim().parse::<i64>().unwrap_or(10).clamp(i64::from(MIN_SIZE_LOG2), i64::from(MAX_SIZE_LOG2)) as u32;
                graph.size = [log2(width), log2(height)];
            }
        }

        if let Some(attributes) = child_element(element, "attributes") {
            graph.attributes = super::graph_attributes::read(attributes);
            if let Some(size) = child_value(attributes, "physicalSize") {
                let mut components = size.split_whitespace().map(str::parse::<f32>);
                if let (Some(Ok(x)), Some(Ok(y)), Some(Ok(z)), None) = (components.next(), components.next(), components.next(), components.next())
                    && [x, y, z].iter().all(|component| component.is_finite() && *component >= 0.0)
                {
                    graph.physical_size = [x, y, z];
                } else {
                    self.warnings.add(format!("graph `{name}` has an invalid physical size"));
                }
            }
        }

        if element.has_tag_name("function") {
            graph.function_type = child_value(element, "type").and_then(|code| code.parse().ok()).unwrap_or(0);
            self.read_graph_inputs(element, &mut graph);
            let dynamic = child_element(element, "paramValue").and_then(|value| child_element(value, "dynamicValue"));
            let function = dynamic.ok_or_else(|| "missing function body".to_string()).and_then(super::function_graph::read);
            graph.function = Some(match function {
                Ok(mut function) => {
                    function.annotations = Default::default();
                    function
                }
                Err(error) => {
                    self.warnings.add(format!("function graph `{name}`: {error}"));
                    materialcraft_core::FunctionGraph::default()
                }
            });
            if let Some(dynamic) = dynamic {
                super::canvas::read_canvas_items(dynamic, &mut graph, self.warnings);
            }

            return graph;
        }

        let image_inputs = self.read_graph_inputs(element, &mut graph);
        let outputs = read_graph_outputs(element);

        let mut read_nodes: HashMap<String, ReadNode> = HashMap::new();
        let comp_nodes: Vec<XmlNode> = child_element(element, "compNodes").into_iter().flat_map(|c| children_named(c, "compNode")).collect();
        for comp_node in &comp_nodes {
            if self.node_count >= MAX_NODES {
                self.warnings.add(format!("only the first {MAX_NODES} nodes were read"));
                break;
            }

            let Some(uid) = child_value(*comp_node, "uid").map(str::to_string) else { continue };

            if read_nodes.contains_key(&uid) {
                self.warnings.add(format!("graph `{name}`: duplicate node uid {uid} skipped"));
                continue;
            }

            self.node_count += 1;
            let mut node = self.read_node(&mut graph, *comp_node, &image_inputs, &outputs);
            if let Some(document_node) = graph.nodes.get_mut(node.index) {
                document_node.disabled = child_value(*comp_node, "materialcraftDisabled") == Some("1");
            }
            for input in child_element(*comp_node, "inputValues").into_iter().flat_map(|inputs| children_named(inputs, "compInputValue")).take(64) {
                let identifier = child_value(input, "identifier").unwrap_or("");
                let value_type = child_value(input, "type").and_then(|code| code.parse().ok()).and_then(materialcraft_core::ValueType::from_code);
                if let Some(value_type) = value_type {
                    if let Some(document_node) = graph.nodes.get_mut(node.index) {
                        document_node.input_values.insert(identifier.to_string(), value_type);
                    }
                    node.inputs.insert(identifier.to_string(), identifier.to_string());
                }
            }
            read_nodes.insert(uid, node);
        }

        let wires = self.read_wires(&mut graph, &comp_nodes, &read_nodes);
        connect_all(&mut graph, wires, self.warnings);
        super::canvas::read_canvas_items(element, &mut graph, self.warnings);
        graph
    }

    pub(super) fn read_graph_inputs(&mut self, element: XmlNode, graph: &mut Graph) -> HashMap<String, InputBridge> {
        let mut image_inputs = HashMap::new();
        for input in child_element(element, "paraminputs").into_iter().flat_map(|p| children_named(p, "paraminput")) {
            let id = child_value(input, "identifier").unwrap_or("input").to_string();
            let code = child_value(input, "type").and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(type_code::FLOAT1);
            if code == type_code::COLOR_IMAGE || code == type_code::GRAY_IMAGE {
                let default = child_element(input, "defaultValue")
                    .and_then(sbs_constant)
                    .and_then(|value| to_param_value(&Value::Color([0.5, 0.5, 0.5, 1.0]), &value, 0))
                    .unwrap_or(Value::Color([0.5, 0.5, 0.5, 1.0]));
                image_inputs.insert(
                    child_value(input, "uid").unwrap_or("").to_string(),
                    InputBridge { identifier: id, type_code: code as u32, usage: component_usage::read(input), default },
                );
                continue;
            }

            if graph.exposed.iter().any(|e| e.id == id) {
                continue;
            }

            let default = exposed_default(code, child_element(input, "defaultValue").and_then(sbs_constant).as_ref());
            if child_value(input, "isConnectable") == Some("1") {
                image_inputs.insert(
                    child_value(input, "uid").unwrap_or("").to_string(),
                    InputBridge { identifier: id, type_code: code as u32, usage: component_usage::read(input), default },
                );
                continue;
            }

            let label = child_element(input, "attributes").and_then(|a| child_value(a, "label")).filter(|l| !l.is_empty()).unwrap_or(&id).to_string();
            let (mut min, mut max) = (0.0, 1.0);
            let widget_options = child_element(input, "defaultWidget").and_then(|w| child_element(w, "options"));
            for option in widget_options.into_iter().flat_map(|o| children_named(o, "option")) {
                let value =
                    child_value(option, "value").and_then(|v| v.split_whitespace().next()).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
                match (child_value(option, "name"), value) {
                    (Some("min"), Some(v)) => min = v,
                    (Some("max"), Some(v)) => max = v,
                    _ => {}
                }
            }

            let group = child_value(input, "group").unwrap_or("").to_string();
            let mut exposed = ExposedParameter {
                id,
                label,
                group,
                default,
                min,
                max,
                options: Vec::new(),
                description: String::new(),
                editor: None,
                clamp: false,
                step: None,
            };
            super::parameter_widget::read(input, &mut exposed);
            if let Some(current) =
                child_element(input, "materialcraftValue").and_then(sbs_constant).and_then(|raw| to_param_value(&exposed.default, &raw, exposed.options.len()))
            {
                graph.inputs.insert(exposed.id.clone(), current);
            }
            graph.exposed.push(exposed);
        }

        super::presets::read(element, graph);
        image_inputs
    }

    pub(super) fn read_wires(&mut self, graph: &mut Graph, comp_nodes: &[XmlNode], read_nodes: &HashMap<String, ReadNode>) -> Vec<Connection> {
        let name = graph.name.clone();
        let mut wires = Vec::new();

        for comp_node in comp_nodes {
            let Some(to) = child_value(*comp_node, "uid").and_then(|u| read_nodes.get(u)) else { continue };

            for connection in child_element(*comp_node, "connections").into_iter().flat_map(|c| children_named(c, "connection")) {
                let sbs_input = child_value(connection, "identifier").unwrap_or("");
                let Some(from) = child_value(connection, "connRef").and_then(|u| read_nodes.get(u)) else {
                    self.warnings.add(format!("graph `{name}`: a wire from a node that is not in the file was dropped"));
                    continue;
                };

                let output_uid = child_value(connection, "connRefOutput").unwrap_or("");
                let from_port = from.outputs.iter().find(|(uid, _)| uid == output_uid).or(from.outputs.first()).map(|(_, port)| port.clone());
                let Some(from_port) = from_port else {
                    self.warnings.add(format!("graph `{name}`: a wire from a node with no outputs was dropped"));
                    continue;
                };

                let port =
                    to.inputs.get(sbs_input).cloned().or_else(|| graph.nodes.get(to.index).and_then(|node| super::image_input::read(&node.def, sbs_input)));
                let to_port = match port {
                    Some(port) => port,
                    None if to.open_inputs => {
                        if let Some(node) = graph.nodes.get_mut(to.index) {
                            if let Some(instance) = &mut node.instance {
                                if !instance.inputs.iter().any(|port| port.identifier == sbs_input) {
                                    if instance.inputs.len() >= MAX_INPUTS || sbs_input.is_empty() {
                                        self.warnings.add(format!(
                                            "graph `{name}`: invalid input `{sbs_input}` or instance at its {MAX_INPUTS}-input limit; wire was dropped"
                                        ));
                                        continue;
                                    }

                                    instance.inputs.push(materialcraft_core::GraphPort { identifier: sbs_input.into(), type_code: 1 });
                                }
                            } else {
                                add_placeholder_input(node, sbs_input);
                            }
                        }

                        sbs_input.to_string()
                    }
                    None => {
                        let def = graph.nodes.get(to.index).map_or("", |n| n.def.as_str());
                        self.warnings.add(format!("graph `{name}`: `{def}` has no input `{sbs_input}`; its wire was dropped"));
                        continue;
                    }
                };

                if self.wire_count >= MAX_WIRES {
                    self.warnings.add(format!("graph `{name}`: the {MAX_WIRES} wire limit was reached; later wires were dropped"));
                    return wires;
                }

                self.wire_count += 1;
                wires.push(Connection { from: from.id, from_port, to: to.id, to_port });
            }
        }

        wires
    }

    pub(super) fn read_node(
        &mut self,
        graph: &mut Graph,
        comp_node: XmlNode<'_, 'text>,
        image_inputs: &HashMap<String, InputBridge>,
        outputs: &HashMap<String, (String, u32, u32)>,
    ) -> ReadNode {
        let gpos = child_element(comp_node, "GUILayout").and_then(|l| child_value(l, "gpos")).map(parse_floats).unwrap_or_default();
        let pos = [gpos.first().copied().unwrap_or(0.0) - NODE_HALF_SIZE[0], gpos.get(1).copied().unwrap_or(0.0) - NODE_HALF_SIZE[1]];
        let output_uids: Vec<(String, bool)> = child_element(comp_node, "compOutputs")
            .into_iter()
            .flat_map(|o| children_named(o, "compOutput"))
            .map(|o| (child_value(o, "uid").unwrap_or("").to_string(), child_value(o, "comptype") == Some("2")))
            .collect();
        let all_outputs_to = |port: &str| output_uids.iter().map(|(uid, _)| (uid.clone(), port.to_string())).collect::<Vec<_>>();
        let text = self.text;

        let Some(implementation) = child_element(comp_node, "compImplementation").and_then(|i| i.children().find(|c| c.is_element())) else {
            let (id, index, node) = push_node(graph, MISSING_NODE, pos);
            if let Some(node) = node {
                fill_placeholder(node, "other", "empty node", "", None, &output_uids, None);
            }

            return ReadNode { id, index, inputs: HashMap::new(), outputs: Vec::new(), open_inputs: true };
        };

        let params = child_element(implementation, "parameters");
        match implementation.tag_name().name() {
            "compInputBridge" => {
                let entry = child_value(implementation, "entry").unwrap_or("");
                let input = image_inputs.get(entry);
                let code = input.map_or(1, |input| input.type_code);
                let definition = match code {
                    1 => "input_color",
                    2 => "input_grayscale",
                    _ => "input_value",
                };

                let (id, index, node) = push_node(graph, definition, pos);
                if let Some(node) = node {
                    let identifier = input.map_or("input", |input| input.identifier.as_str());
                    node.params.insert("identifier".into(), Value::Text(identifier.to_string()));
                    node.params.insert("usage".into(), Value::Enum(input.map_or(ComponentUsage::ANY_INDEX, |input| input.usage)));
                    if let Some(input) = input {
                        node.params.insert("default".into(), input.default.clone());
                    }

                    node.base = read_base_params(params, self.warnings);
                }

                ReadNode { id, index, inputs: HashMap::new(), outputs: all_outputs_to("output"), open_inputs: false }
            }
            "compOutputBridge" => {
                let output = child_value(implementation, "output").unwrap_or("");
                let (identifier, usage, channels) = outputs.get(output).cloned().unwrap_or_else(|| ("output".into(), ComponentUsage::ANY_INDEX, 1));
                let (id, index, node) = push_node(graph, "output", pos);
                if let Some(node) = node {
                    node.params.insert("identifier".into(), Value::Text(identifier));
                    node.params.insert("usage".into(), Value::Enum(usage));
                    if matches!(channels, 1 | 2) {
                        node.params.insert("color_mode".into(), Value::Bool(channels == 1));
                    }
                }

                let inputs = HashMap::from([("inputNodeOutput".to_string(), "input".to_string())]);
                ReadNode { id, index, inputs, outputs: Vec::new(), open_inputs: false }
            }
            "compFilter" => {
                let filter_name = child_value(implementation, "filter").unwrap_or("");
                let graph_name = graph.name.clone();
                match filter_by_sbs_name(filter_name).filter(|f| find(f.def).is_some()) {
                    Some(filter) => {
                        let (id, index, node) = push_node(graph, filter.def, pos);
                        if let Some(node) = node {
                            self.read_filter_params(node, filter, params, &graph_name);

                            // Preserve a declared color output even when the color switch is omitted
                            if let [(_, gray)] = output_uids.as_slice()
                                && find(filter.def).is_some_and(|definition| {
                                    matches!(definition.param("color_mode").map(|parameter| &parameter.default), Some(Value::Bool(_)))
                                })
                                && !node.params.contains_key("color_mode")
                                && !node.functions.contains_key("color_mode")
                                && !node.bindings.contains_key("color_mode")
                            {
                                node.params.insert("color_mode".into(), Value::Bool(!gray));
                            }
                            super::parameter_array::read(node, implementation);
                            node.base = read_base_params(params, self.warnings);
                            if filter.sbs == "pixelprocessor"
                                && !params
                                    .into_iter()
                                    .flat_map(|parameters| children_named(parameters, "parameter"))
                                    .any(|p| child_value(p, "name") == Some("format"))
                            {
                                // A processor defaults to its parent format, even with an image input
                                node.base.format = OutputFormat::U8;
                                node.base.format_mode = ParameterInheritance::RelativeToParent;
                            }
                            if filter.sbs == "fxmaps"
                                && let Some(pattern) = child_element(implementation, "paramsGraphs").and_then(|graphs| child_element(graphs, "paramsGraph"))
                            {
                                match super::pattern_graph::read(pattern) {
                                    Ok(pattern) => {
                                        node.params.insert("pattern".into(), Value::PatternGraph(Box::new(pattern)));
                                    }
                                    Err(error) => self.warnings.add(format!("pattern graph: {error}")),
                                }
                            }
                        }

                        let inputs = filter.inputs.iter().map(|(sbs, port)| (sbs.to_string(), port.to_string())).collect();
                        ReadNode { id, index, inputs, outputs: all_outputs_to("output"), open_inputs: false }
                    }
                    None => {
                        let (id, index, node) = push_node(graph, MISSING_NODE, pos);
                        if let Some(node) = node {
                            fill_placeholder(node, "filter", filter_name, "", params.map(|p| &text[p.range()]), &output_uids, None);
                        }

                        ReadNode { id, index, inputs: HashMap::new(), outputs: placeholder_outputs(&output_uids, None), open_inputs: true }
                    }
                }
            }
            "compInstance" => self.read_instance(graph, implementation, pos, &output_uids),
            other => {
                let (id, index, node) = push_node(graph, MISSING_NODE, pos);
                if let Some(node) = node {
                    fill_placeholder(node, "other", other, "", None, &output_uids, None);
                }

                ReadNode { id, index, inputs: HashMap::new(), outputs: placeholder_outputs(&output_uids, None), open_inputs: true }
            }
        }
    }

    pub(super) fn read_instance(&mut self, graph: &mut Graph, implementation: XmlNode<'_, 'text>, pos: [f32; 2], output_uids: &[(String, bool)]) -> ReadNode {
        let graph_name = graph.name.clone();
        let (path, dependency) = self.resolve_reference(child_value(implementation, "path").unwrap_or(""));
        let params = child_element(implementation, "parameters");
        let bridgings: Vec<(String, String)> = child_element(implementation, "outputBridgings")
            .into_iter()
            .flat_map(|b| children_named(b, "outputBridging"))
            .map(|b| (child_value(b, "uid").unwrap_or("").to_string(), child_value(b, "identifier").unwrap_or("output").to_string()))
            .collect();
        let instance_params =
            || params.into_iter().flat_map(|p| children_named(p, "parameter")).filter(|p| !BASE_PARAMS.contains(&child_value(*p, "name").unwrap_or("")));

        let own_graph = dependency.as_deref().is_none_or(|d| d == SELF_DEPENDENCY).then(|| self.graph_names.get(&path).cloned()).flatten();
        let external = dependency.as_deref().filter(|file| *file != SELF_DEPENDENCY && file.to_ascii_lowercase().ends_with(".sbs"));
        if own_graph.is_some() || external.is_some() {
            let (id, index, node) = push_node(graph, "graph_instance", pos);
            if let Some(node) = node {
                let interface = super::instance_interface::read(implementation);
                let has_interface = interface.is_some();
                let inputs = match interface.transpose() {
                    Ok(inputs) => inputs.unwrap_or_default(),
                    Err(error) => {
                        self.warnings.add(error);
                        Vec::new()
                    }
                };
                if let Some(sub_graph) = &own_graph {
                    node.params.insert("graph".into(), Value::Text(sub_graph.clone()));
                }
                if own_graph.is_none() || has_interface {
                    let comp_node = implementation.parent().and_then(|parent| parent.parent());
                    let types: HashMap<_, _> = comp_node
                        .and_then(|node| child_element(node, "compOutputs"))
                        .into_iter()
                        .flat_map(|outputs| children_named(outputs, "compOutput"))
                        .map(|output| {
                            (child_value(output, "uid").unwrap_or(""), child_value(output, "comptype").and_then(|code| code.parse().ok()).unwrap_or(1))
                        })
                        .collect();
                    node.instance = Some(materialcraft_core::GraphInstance {
                        label: String::new(),
                        defaults: Default::default(),
                        package: external.unwrap_or("").into(),
                        graph: path.clone(),
                        inputs,
                        outputs: bridgings
                            .iter()
                            .map(|(uid, identifier)| materialcraft_core::GraphPort {
                                identifier: identifier.clone(),
                                type_code: types.get(uid.as_str()).copied().unwrap_or(1),
                            })
                            .collect(),
                    });
                }
                for param in instance_params() {
                    if let Some(value) = child_element(param, "paramValue").and_then(sbs_constant) {
                        node.params.insert(child_value(param, "name").unwrap_or("").to_string(), to_natural_value(&value));
                    } else {
                        self.set_param(node, child_value(param, "name").unwrap_or(""), &Value::Float(0.0), 0, param, &graph_name);
                    }
                }
                node.base = read_base_params(params, self.warnings);
            }
            return ReadNode { id, index, inputs: HashMap::new(), outputs: bridgings, open_inputs: true };
        }

        let short_name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let (id, index, node) = push_node(graph, MISSING_NODE, pos);
        if let Some(node) = node {
            let raw_params = params.map(|parameters| &self.text[parameters.range()]);
            fill_placeholder(node, "instance", &short_name, dependency.as_deref().unwrap_or(""), raw_params, output_uids, Some(&bridgings));
            node.params.insert("sbs_path".into(), Value::Text(path));
        }

        ReadNode { id, index, inputs: HashMap::new(), outputs: placeholder_outputs(output_uids, Some(&bridgings)), open_inputs: true }
    }

    pub(super) fn set_param(&mut self, node: &mut Node, id: &str, like: &Value, options: usize, param: XmlNode, graph_name: &str) {
        let Some(param_value) = child_element(param, "paramValue") else { return };

        if let Some(value) = sbs_constant(param_value) {
            // Preserve selector bits until the cook can resolve them against connected image kinds
            if node.def == "channels_shuffle" && matches!(id, "red" | "green" | "blue" | "alpha") {
                node.params.insert(id.to_string(), Value::Enum(value.int(0) as u32));
                return;
            }

            match to_param_value(like, &value, options) {
                Some(value) => {
                    node.params.insert(id.to_string(), value);
                }
                None => self.warnings.add(format!("`{}`: parameter `{id}` has a value type MaterialCraft can't read yet; kept the default", node.def)),
            }
            return;
        }

        let Some(dynamic) = child_element(param_value, "dynamicValue") else { return };

        match function_graph::read(dynamic) {
            Ok(graph) if matches!(like, Value::Function(_)) => {
                node.params.insert(id.to_string(), Value::Function(Box::new(graph)));
            }
            Ok(graph) => {
                node.functions.insert(id.to_string(), graph);
            }
            Err(error) => self.warnings.add(format!("graph `{graph_name}`: {} parameter `{id}`: {error}", node.def)),
        }
    }

    pub(super) fn read_filter_params(&mut self, node: &mut Node, filter: &FilterMap, params: Option<XmlNode>, graph_name: &str) {
        let Some(def) = find(filter.def) else { return };

        // Store omitted native values explicitly for imported node inspection and round trips
        if filter.sbs == "uniform" {
            node.params.insert("color".into(), Value::Color([0.0, 0.0, 0.0, 1.0]));
        }
        if filter.sbs == "normal" {
            node.params.insert("opengl".into(), Value::Bool(false));
            node.params.insert("height_alpha".into(), Value::Bool(true));
        }

        let filter_params: Vec<XmlNode> =
            params.into_iter().flat_map(|p| children_named(p, "parameter")).filter(|p| !BASE_PARAMS.contains(&child_value(*p, "name").unwrap_or(""))).collect();
        for param in &filter_params {
            let param_name = child_value(*param, "name").unwrap_or("");

            // Read the earlier writer's alias without emitting it alongside the native parameter
            let param_name = match (filter.sbs, param_name) {
                ("hsl", "luminance") => "luminosity",
                ("dyngradient", "position") => "coordinate",
                ("dyngradient", "orientation") => "uvselector",
                _ => param_name,
            };
            let value = child_element(*param, "paramValue").and_then(sbs_constant);

            if filter.sbs == "text"
                && param_name == "fontdata"
                && let Some(SbsValue::Text(url)) = &value
                && (url.starts_with("pkg://") || self.direct_resource_names.contains_key(url))
            {
                let name = self.resource_binding(url, graph_name).unwrap_or_else(|| url.clone());
                node.params.insert("fontdata".into(), Value::Text(name));
                continue;
            }

            if let Some(&(_, mc_name)) = filter.params.iter().find(|(sbs, _)| *sbs == param_name) {
                let Some(param_def) = def.param(mc_name) else { continue };

                self.set_param(node, mc_name, &param_def.default, param_def.options.len(), *param, graph_name);
                continue;
            }

            match (filter.sbs, param_name, value) {
                ("bitmap", "bitmapresourcepath", Some(SbsValue::Text(url))) | ("svg", "vectorresourcepath", Some(SbsValue::Text(url))) => {
                    if let Some(name) = self.resource_binding(&url, graph_name) {
                        node.params.insert("resource".into(), Value::Text(name));
                    }
                }
                ("grayscaleconversion", "channelsweights", Some(weights)) => {
                    for (i, channel) in ["red", "green", "blue", "alpha"].into_iter().enumerate() {
                        node.params.insert(channel.into(), Value::Float(weights.float(i)));
                    }
                }
                _ if HANDLED_PARAMS.contains(&(filter.sbs, param_name)) => {}
                _ => self.warnings.add(format!("`{}`: SBS parameter `{param_name}` is not supported yet and was ignored", filter.def)),
            }
        }
    }

    pub(super) fn resource_name(&self, url: &str) -> Option<&String> {
        if let Some(name) = self.direct_resource_names.get(url) {
            return Some(name);
        }

        let dependency_uid = url.split_once('?').and_then(|(_, query)| query.split('&').find_map(|part| part.strip_prefix("dependency=")));
        if dependency_uid.is_some_and(|uid| !self.dependencies.contains_key(uid)) {
            return None;
        }

        let (path, dependency) = self.resolve_reference(url);
        match dependency.filter(|file| file != SELF_DEPENDENCY) {
            Some(file) => self.external_resource_names.get(&(file, path)),
            None => self.resource_names.get(&path),
        }
    }

    fn resource_binding(&mut self, url: &str, graph_name: &str) -> Option<String> {
        if let Some(name) = self.resource_name(url) {
            return Some(name.clone());
        }

        let dependency_uid = url.split_once('?').and_then(|(_, query)| query.split('&').find_map(|part| part.strip_prefix("dependency=")));
        if let Some(uid) = dependency_uid.filter(|uid| !self.dependencies.contains_key(*uid)) {
            self.warnings.add(format!("graph `{graph_name}`: resource reference has unknown dependency `{uid}`"));
        } else {
            let (path, _) = self.resolve_reference(url);
            self.warnings.add(format!("graph `{graph_name}`: Node uses resource `{path}`, which is not in the package"));
        }

        None
    }
}

pub(super) fn unique_name(taken: &mut HashSet<String>, base: &str) -> String {
    if taken.insert(base.to_string()) {
        return base.to_string();
    }

    (1..).map(|i| format!("{base}_{i}")).find(|name| taken.insert(name.clone())).unwrap_or_else(|| base.to_string())
}

pub(super) fn push_node<'g>(graph: &'g mut Graph, def: &str, pos: [f32; 2]) -> (NodeId, usize, Option<&'g mut Node>) {
    let id = graph.add_node(def, pos);
    let index = graph.nodes.len() - 1;
    (id, index, graph.nodes.last_mut())
}

pub(super) fn read_graph_outputs(element: XmlNode) -> HashMap<String, (String, u32, u32)> {
    let mut outputs = HashMap::new();
    for output in child_element(element, "graphOutputs").into_iter().flat_map(|o| children_named(o, "graphoutput")) {
        let identifier = child_value(output, "identifier").unwrap_or("output").to_string();
        let usage = component_usage::read(output);
        let channels = child_value(output, "channels").and_then(|value| value.parse().ok()).unwrap_or(1);
        outputs.insert(child_value(output, "uid").unwrap_or("").to_string(), (identifier, usage, channels));
    }

    outputs
}
