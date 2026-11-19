//! Named SBS input presets refer to the same identifiers and UIDs as the graph interface.
use std::collections::HashMap;
use std::fmt::Write;

use materialcraft_core::{Graph, Preset};
use roxmltree::Node;

use super::sbs_value::{constant_xml, exposed_type_code, sbs_constant, to_param_value};
use super::xml::{child_element, child_value, children_named, xml_escape};

pub(super) fn write(out: &mut String, graph: &Graph, uids: &HashMap<String, u64>) {
    if graph.presets.is_empty() {
        return;
    }

    out.push_str("<sbspresets>");
    for preset in &graph.presets {
        let _ = write!(out, "<sbspreset><label v=\"{}\"/><presetinputs>", xml_escape(&preset.name));
        for (id, value) in &preset.values {
            if let (Some(uid), Some(constant)) = (uids.get(id), constant_xml(value)) {
                let _ = write!(
                    out,
                    "<presetinput><uid v=\"{uid}\"/><identifier v=\"{}\"/><paramValue>{constant}</paramValue><type v=\"{}\"/></presetinput>",
                    xml_escape(id),
                    exposed_type_code(value)
                );
            }
        }
        out.push_str("</presetinputs></sbspreset>");
    }
    out.push_str("</sbspresets>");
}

pub(super) fn read(element: Node<'_, '_>, graph: &mut Graph) {
    let Some(presets) = child_element(element, "sbspresets") else {
        return;
    };

    let exposed: HashMap<_, _> = graph.exposed.iter().map(|p| (p.id.as_str(), p)).collect();
    for preset in children_named(presets, "sbspreset") {
        let Some(name) = child_value(preset, "label") else {
            continue;
        };

        let values = child_element(preset, "presetinputs")
            .into_iter()
            .flat_map(|inputs| children_named(inputs, "presetinput"))
            .filter_map(|input| {
                let param = exposed.get(child_value(input, "identifier")?)?;
                let raw = sbs_constant(child_element(input, "paramValue")?)?;
                Some((param.id.clone(), to_param_value(&param.default, &raw, param.options.len())?))
            })
            .collect();
        graph.presets.push(Preset { name: name.into(), values });
    }
}
