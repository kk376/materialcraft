use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
// Some MCP clients reject boolean sub-schemas
#[schemars(extend("additionalProperties" = {"not": {}}))]
pub struct MaterialExportParams {
    /// Forward-slash path relative to the write root (`.mcmat` is appended if missing). The
    /// parent folder must exist
    pub path: String,
    /// Graph index or full SBS path; defaults to the open graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn material_publication_rejects_bake_options() {
        for (option, value) in [("size", json!(256)), ("size", json!(null)), ("values", json!({})), ("inputs", json!({})), ("preset", json!("Worn"))] {
            let mut request = json!({"path":"material"});
            request[option] = value;

            let error = serde_json::from_value::<MaterialExportParams>(request).unwrap_err();

            assert!(error.to_string().contains(&format!("unknown field `{option}`")));
        }

        let params = serde_json::from_value::<MaterialExportParams>(json!({"path":"material", "graph":"Materials/Oak"})).unwrap();
        assert_eq!(params.graph, Some(json!("Materials/Oak")));
    }
}
