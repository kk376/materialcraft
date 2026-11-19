use super::{
    constant::{MAX_GRAPHS, SNIFF_BYTES},
    placeholder::report_placeholders,
    sbs_read::SbsRead,
    sbs_reader::SbsReader,
    warnings::Warnings,
    xml::{child_element, child_value, children_named},
};
use crate::Result;
use materialcraft_core::Package;
use roxmltree::Node as XmlNode;
use std::collections::{HashMap, HashSet};

pub fn read_sbs(bytes: &[u8], name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<SbsRead> {
    super::SbsDocument::parse(bytes)?.read(name, linked)
}

pub(super) fn read_document(
    doc: &roxmltree::Document<'_>,
    name: &str,
    linked: &dyn Fn(&str) -> Option<Vec<u8>>,
    resource_budget: Option<&std::cell::Cell<u64>>,
) -> Result<SbsRead> {
    let text = doc.input_text();
    let root = doc.root_element();

    let mut warnings = Warnings::default();
    let file_stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let file_stem = file_stem.rsplit_once('.').map_or(file_stem, |(stem, _)| stem).trim();
    let package_name = match child_value(root, "identifier").map(str::trim) {
        Some(id) if !id.is_empty() && id != "Unsaved Package" => id.to_string(),
        _ if !file_stem.is_empty() => file_stem.to_string(),
        _ => "Untitled package".to_string(),
    };

    // Include one probe byte so a valid inline asset can use the exact retained limit
    let inflation_work = std::cell::Cell::new(resource_budget.map_or(0, |budget| budget.get().saturating_add(1)));
    let mut reader = SbsReader {
        inflation_work,
        resource_budget,
        text,
        warnings: &mut warnings,
        dependencies: HashMap::new(),
        graph_names: HashMap::new(),
        resource_names: HashMap::new(),
        external_resource_names: HashMap::new(),
        direct_resource_names: HashMap::new(),
        used_resource_names: HashSet::new(),
        node_count: 0,
        wire_count: 0,
    };

    for dependency in child_element(root, "dependencies").into_iter().flat_map(|d| children_named(d, "dependency")) {
        if let (Some(uid), Some(file)) = (child_value(dependency, "uid"), child_value(dependency, "filename")) {
            reader.dependencies.insert(uid.to_string(), file.to_string());
        }
    }

    // Load resources before resolving graph references
    let content = child_element(root, "content");
    let mut pkg = Package::new(&package_name);
    pkg.format_version = child_value(root, "formatVersion").filter(|version| !version.trim().is_empty()).map(str::to_string);
    let mut graph_elements: Vec<(String, XmlNode)> = Vec::new();
    if let Some(content) = content {
        reader.collect_items(content, "", &mut pkg, &mut graph_elements, linked);
        super::linked_resources::read(&mut reader, content, &mut pkg, linked);
        super::resource_replacement::read_direct(&mut reader, content, &mut pkg, linked);
    }

    // Name every graph first so instances can refer to graphs defined later in the file
    let mut names: Vec<String> = Vec::new();
    for (path, graph) in graph_elements.iter().take(MAX_GRAPHS) {
        let identifier = child_value(*graph, "identifier").unwrap_or("").trim();
        let name = if identifier.is_empty() { "Graph" } else { identifier }.to_string();
        reader.graph_names.insert(path.clone(), path.clone());
        names.push(name);
    }

    if graph_elements.len() > MAX_GRAPHS {
        reader.warnings.add(format!("only the first {MAX_GRAPHS} graphs were read"));
    }

    for ((path, graph), name) in graph_elements.iter().zip(names) {
        let mut graph = reader.read_graph(*graph, name);
        graph.folder = path.rsplit_once('/').map_or("", |(folder, _)| folder).into();
        pkg.graphs.push(graph);
    }

    report_placeholders(&pkg, reader.warnings);
    Ok(SbsRead { pkg, warnings: warnings.finish() })
}

pub fn is_sbs(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(SNIFF_BYTES)]);
    let mut rest = head.trim_start_matches('\u{feff}');

    // Skip the XML declaration and comments before the root element
    loop {
        rest = rest.trim_start();
        let (skip_open, skip_close) = if rest.starts_with("<?") {
            ("<?", "?>")
        } else if rest.starts_with("<!--") {
            ("<!--", "-->")
        } else {
            break;
        };

        match rest[skip_open.len()..].find(skip_close) {
            Some(end) => rest = &rest[skip_open.len() + end + skip_close.len()..],
            None => return false,
        }
    }

    rest.strip_prefix("<package").is_some_and(|after| after.starts_with(|c: char| c == '>' || c == '/' || c.is_whitespace()))
}
