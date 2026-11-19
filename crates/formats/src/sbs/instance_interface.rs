//! Preserve typed graph inputs, including unconnected inputs, when localizing graph references.
use super::xml::{child_element, child_value, children_named, xml_escape};
use materialcraft_core::GraphPort;
use roxmltree::Node;
use std::{collections::BTreeSet, fmt::Write};

pub(super) const MAX_INPUTS: usize = 4096;

pub(super) fn read(element: Node<'_, '_>) -> Option<Result<Vec<GraphPort>, String>> {
    let interface = child_element(element, "materialcraftInterface")?;
    Some((|| {
        let mut inputs = Vec::new();
        let mut names = BTreeSet::new();
        for port in children_named(interface, "input") {
            if inputs.len() == MAX_INPUTS {
                return Err(format!("Graph instance exceeds the {MAX_INPUTS}-input limit"));
            }

            let identifier = child_value(port, "identifier").unwrap_or("");
            if identifier.is_empty() || !names.insert(identifier) {
                return Err("Invalid or duplicate graph input identifier".into());
            }

            let type_code = child_value(port, "type").and_then(|value| value.parse().ok()).ok_or("Invalid graph input type")?;
            inputs.push(GraphPort { identifier: identifier.into(), type_code });
        }
        Ok(inputs)
    })())
}

pub(super) fn write(out: &mut String, inputs: &[GraphPort]) -> Result<(), String> {
    if inputs.len() > MAX_INPUTS {
        return Err(format!("Graph instance exceeds the {MAX_INPUTS}-input limit"));
    }

    // Validate before writing because the XML escaper drops unsupported characters
    let mut names = BTreeSet::new();
    for port in inputs {
        if port.identifier.is_empty() || !port.identifier.chars().all(craft_xml::is_xml_char) || !names.insert(&port.identifier) {
            return Err("Invalid or duplicate graph input identifier".into());
        }
    }

    out.push_str("<materialcraftInterface>");
    for port in inputs {
        let _ = write!(out, "<input><identifier v=\"{}\"/><type v=\"{}\"/></input>", xml_escape(&port.identifier), port.type_code);
    }
    out.push_str("</materialcraftInterface>");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_rejects_duplicate_malformed_and_excess_inputs() {
        let parse = |body: &str| {
            let xml = format!("<compInstance><materialcraftInterface>{body}</materialcraftInterface></compInstance>");
            let document = roxmltree::Document::parse(&xml).unwrap();
            read(document.root_element()).unwrap()
        };

        assert!(parse("<input><identifier v=\"a\"/><type v=\"bad\"/></input>").is_err());
        assert!(parse("<input><identifier v=\"a\"/><type v=\"1\"/></input>".repeat(2).as_str()).is_err());

        let mut body = String::new();
        for index in 0..MAX_INPUTS {
            write!(body, "<input><identifier v=\"p{index}\"/><type v=\"2\"/></input>").unwrap();
        }

        assert_eq!(parse(&body).unwrap().len(), MAX_INPUTS);
        body.push_str("<input><identifier v=\"extra\"/><type v=\"2\"/></input>");
        assert!(parse(&body).is_err());
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;

    #[test]
    fn writer_preserves_identifiers_or_refuses_before_appending() {
        for invalid in ['\0', '\u{1f}', '\u{fffe}', '\u{ffff}'] {
            let inputs =
                vec![GraphPort { identifier: "roughness".into(), type_code: 2 }, GraphPort { identifier: format!("roughness{invalid}"), type_code: 2 }];
            let mut out = String::from("unchanged");

            assert!(write(&mut out, &inputs).is_err());
            assert_eq!(out, "unchanged");
            assert!(write(&mut out, &inputs[1..]).is_err());
            assert_eq!(out, "unchanged");
        }

        let inputs = vec![GraphPort { identifier: "é😀<&\"'\t\n\r".into(), type_code: 2 }];
        let mut xml = String::from("<compInstance>");

        write(&mut xml, &inputs).unwrap();
        xml.push_str("</compInstance>");
        let document = roxmltree::Document::parse(&xml).unwrap();

        assert_eq!(read(document.root_element()).unwrap().unwrap(), inputs);
    }

    #[test]
    fn writer_matches_reader_limits_and_refuses_before_appending() {
        let mut inputs: Vec<_> = (0..MAX_INPUTS).map(|index| GraphPort { identifier: format!("p{index}"), type_code: 2 }).collect();
        let mut xml = String::from("<compInstance>");

        write(&mut xml, &inputs).unwrap();
        xml.push_str("</compInstance>");
        let document = roxmltree::Document::parse(&xml).unwrap();

        assert_eq!(read(document.root_element()).unwrap().unwrap(), inputs);

        inputs.push(GraphPort { identifier: "extra".into(), type_code: 2 });
        let mut out = String::from("unchanged");
        assert!(write(&mut out, &inputs).unwrap_err().contains("4096"));
        assert_eq!(out, "unchanged");

        inputs.truncate(2);
        inputs[1].identifier = inputs[0].identifier.clone();
        assert!(write(&mut out, &inputs).is_err());

        inputs[1].identifier.clear();
        assert!(write(&mut out, &inputs).is_err());
        assert_eq!(out, "unchanged");
    }

    #[test]
    fn writer_rejects_input_identifiers_that_xml_would_change() {
        let mut xml = String::new();
        let inputs = [GraphPort { identifier: "roughness".into(), type_code: 2 }, GraphPort { identifier: "roughness\u{1f}".into(), type_code: 2 }];

        assert!(write(&mut xml, &inputs).is_err());
        assert!(xml.is_empty());
    }
}
