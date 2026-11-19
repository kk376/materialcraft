//! Copy reachable SBS definitions with local, collision-free identities.
use std::collections::{BTreeMap, BTreeSet};

use materialcraft_core::{Graph, GraphIndex, Package, Value};

use crate::{FormatError, Result};

// Bound the combined graph and function traversal independently of cooking
const MAX_DEPENDENCY_DEPTH: usize = 32;

fn bad(message: impl Into<String>) -> FormatError {
    FormatError::BadPackage(message.into())
}

/// The location of the entry in the source project is independent of its location in the publication.
pub fn snapshot(package: &Package, entry: usize) -> Result<Package> {
    let graph = package.graph(entry).map_err(|e| bad(e.to_string()))?;
    if graph.function.is_some() {
        return Err(bad(format!("select a material graph with connected outputs: `{}`", graph.sbs_path())));
    }

    let mut result = Package::new(&graph.name);
    result.format_version.clone_from(&package.format_version);
    let mut builder = SnapshotBuilder { root: package, result, completed: BTreeMap::new(), active: BTreeSet::new(), resources: BTreeMap::new(), nodes: 0 };
    builder.copy_graph("", entry)?;
    Ok(builder.result)
}

type GraphKey = (String, usize);

struct SnapshotBuilder<'a> {
    root: &'a Package,
    result: Package,
    completed: BTreeMap<GraphKey, (String, usize)>,
    active: BTreeSet<GraphKey>,
    resources: BTreeMap<(String, String), String>,
    nodes: usize,
}

impl<'a> SnapshotBuilder<'a> {
    fn package(&self, owner: &str) -> Result<&'a Package> {
        if owner.is_empty() {
            return Ok(self.root);
        }

        self.root.dependencies.get(owner).map(AsRef::as_ref).ok_or_else(|| bad(format!("missing source package `{owner}`")))
    }

    fn reference(&mut self, owner: &str, name: &str) -> Result<(String, usize)> {
        let (index, _) = self.package(owner)?.graph_by_name(name).ok_or_else(|| bad(format!("missing or ambiguous graph `{name}` in `{owner}`")))?;
        self.copy_graph(owner, index)
    }

    fn copy_graph(&mut self, owner: &str, index: usize) -> Result<(String, usize)> {
        let key = (owner.to_owned(), index);
        let source = self.package(owner)?.graph(index).map_err(|e| bad(e.to_string()))?;
        if self.active.contains(&key) {
            return Err(bad(format!("cyclic graph reference at `{}`", source.sbs_path())));
        }

        if let Some((path, depth)) = self.completed.get(&key) {
            // A shared subtree can be reached from a longer path than its first visit
            if self.active.len() + depth > MAX_DEPENDENCY_DEPTH {
                return Err(bad("published graph dependencies exceed the depth limit"));
            }

            return Ok((path.clone(), *depth));
        }

        let graph_count = self.completed.len() + self.active.len();
        if self.active.len() >= MAX_DEPENDENCY_DEPTH || graph_count >= crate::sbs::MAX_GRAPHS {
            return Err(bad("published graph dependencies exceed the depth or graph count limit"));
        }

        let mut graph = source.clone();
        if graph.function.is_none() {
            let outputs: Vec<_> = graph.nodes.iter().filter(|node| node.def == "output").map(|node| node.id).collect();
            let index = GraphIndex::new(&graph);
            if graph_count == 0 && !outputs.iter().any(|id| !index.incoming(*id).is_empty()) {
                return Err(bad(format!("select a material graph with connected outputs: `{}`", graph.sbs_path())));
            }

            let needed = index.required_nodes(&outputs);
            graph.nodes.retain(|node| needed.contains(&node.id) || matches!(node.def.as_str(), "input_color" | "input_grayscale" | "input_value"));
            graph.connections.retain(|c| needed.contains(&c.from) && needed.contains(&c.to));
            if graph.topo_order().len() != graph.nodes.len() {
                return Err(bad(format!("cycle in graph `{}`", graph.sbs_path())));
            }
        }

        self.nodes = self.nodes.saturating_add(graph.nodes.len());
        if self.nodes > crate::sbs::MAX_NODES {
            return Err(bad("published graph exceeds the node count limit"));
        }

        graph.folder = if graph_count == 0 { String::new() } else { format!("Dependencies/g{graph_count}") };
        let path = graph.sbs_path();
        self.active.insert(key.clone());
        let mut max_child_depth = 0;
        for instance in graph.instances_mut() {
            let package = if instance.package.is_empty() { owner } else { &instance.package };
            let (local_path, depth) = self.reference(package, &instance.graph)?;
            instance.graph = local_path;
            max_child_depth = max_child_depth.max(depth);
            instance.package.clear();
        }
        for node in &mut graph.nodes {
            if node.def == materialcraft_core::MISSING_NODE {
                return Err(bad(format!("unsupported node `{}` in `{path}`", node.label)));
            }

            if node.def == "graph_instance" {
                if let Some(instance) = &node.instance {
                    node.params.insert("graph".into(), Value::Text(instance.graph.clone()));
                } else if let Some(Value::Text(target)) = node.params.get_mut("graph") {
                    let (local_path, depth) = self.reference(owner, target)?;
                    *target = local_path;
                    max_child_depth = max_child_depth.max(depth);
                } else {
                    return Err(bad(format!("missing graph reference in `{path}`")));
                }
            }

            match node.def.as_str() {
                "bitmap" | "svg" => {
                    let Some(Value::Text(name)) = node.params.get_mut("resource").filter(|v| !matches!(v, Value::Text(s) if s.is_empty())) else {
                        return Err(bad(format!("missing resource for {} node {} in `{path}`", node.def, node.id.0)));
                    };

                    *name = self.resource(owner, name)?;
                }
                "text" => {
                    if let Some(Value::Text(name)) = node.params.get_mut("fontdata")
                        && !name.is_empty()
                    {
                        *name = self.resource(owner, name)?;
                    }
                }
                _ => {}
            }
        }
        for name in graph.input_images.values_mut() {
            *name = self.resource(owner, name)?;
        }

        let depth = max_child_depth + 1;
        self.active.remove(&key);
        self.completed.insert(key, (path.clone(), depth));
        self.result.graphs.push(graph);
        Ok((path, depth))
    }

    fn resource(&mut self, owner: &str, name: &str) -> Result<String> {
        let key = (owner.to_string(), name.to_string());
        if let Some(local) = self.resources.get(&key) {
            return Ok(local.clone());
        }

        let mut resource = self.package(owner)?.resource(name).cloned().ok_or_else(|| bad(format!("missing resource `{name}` in `{owner}`")))?;
        if resource.bytes.is_empty() {
            return Err(bad(format!("missing resource bytes for `{name}` ({})", resource.source)));
        }

        let local = format!("asset_{}", self.resources.len() + 1);
        resource.name.clone_from(&local);
        resource.folder = "Resources".into();
        resource.source.clear();
        self.resources.insert(key, local.clone());
        self.result.resources.push(resource);
        Ok(local)
    }
}

pub fn entry_graph(pkg: &Package) -> Result<usize> {
    let mut roots = pkg.graphs.iter().enumerate().filter(|(_, g)| g.folder.is_empty());
    match (roots.next(), roots.next()) {
        (Some((index, Graph { function: None, .. })), None) => Ok(index),
        _ => Err(bad("source.sbs must have exactly one root material graph; put helper graphs and functions in a dependency folder")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connect_reference(graph: &mut Graph, target: &str) {
        let instance = graph.add_node("graph_instance", [0.0; 2]);
        graph.node_mut(instance).unwrap().params.insert("graph".into(), Value::Text(target.into()));
        let output = graph.add_node("output", [0.0; 2]);
        graph.connect(instance, "output", output, "input").unwrap();
    }

    #[test]
    fn depth_limit_applies_to_both_paths_through_a_shared_subtree() {
        for depth in [MAX_DEPENDENCY_DEPTH, MAX_DEPENDENCY_DEPTH + 1] {
            for cached_first in [true, false] {
                let mut package = Package::new("Shared subtree");
                let mut root = Graph::new("Root");
                let targets = if cached_first { ["Shared", "Link0"] } else { ["Link0", "Shared"] };
                for target in targets {
                    connect_reference(&mut root, target);
                }

                package.graphs.push(root);
                let link_count = depth - 3;
                for index in 0..link_count {
                    let mut link = Graph::new(&format!("Link{index}"));
                    let target = if index + 1 == link_count { "Shared".into() } else { format!("Link{}", index + 1) };
                    connect_reference(&mut link, &target);
                    package.graphs.push(link);
                }

                let mut shared = Graph::new("Shared");
                connect_reference(&mut shared, "Leaf");
                package.graphs.extend([shared, Graph::new("Leaf")]);

                let published = snapshot(&package, 0);

                if depth == MAX_DEPENDENCY_DEPTH {
                    assert_eq!(published.unwrap().graphs.len(), depth);
                } else {
                    assert!(published.unwrap_err().to_string().contains("depth"), "cached first: {cached_first}");
                }
            }
        }
    }
}
