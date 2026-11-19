use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExportParams {
    /// Output folder relative to the write root (created if missing; `.` = the root)
    pub dir: String,
    /// png (default), jpg, tga, bmp, webp, exr, hdr
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// File name pattern with $(graph), $(identifier) and $(usage) (default `$(graph)_$(identifier)`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// 8, 16 or 32 bits (default: the graph's output format)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// The larger side or [width, height] in pixels, rounded down to powers of two from 16 to 8192.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "size_schema")]
    pub size: Option<materialcraft_engine::ExportSize>,
    /// Graph index or full SBS path. Defaults to the open graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<serde_json::Value>,
    /// Output identifiers to write (default: every connected output)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<String>>,
    /// Color space per output identifier: srgb, linear or raw (default: sRGB for color usages, raw for data)
    #[serde(default, rename = "colorSpaces", skip_serializing_if = "Option::is_none")]
    pub color_spaces: Option<std::collections::BTreeMap<String, String>>,
    /// Export profile: materialx for OpenPBR with baked textures, maps for labeled textures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Optional named preset, applied before explicit values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Height range in scene units for MaterialX displacement.
    #[serde(default, rename = "displacementScale", skip_serializing_if = "Option::is_none")]
    pub displacement_scale: Option<f32>,
    /// Source tangent-space normal convention for baked maps. Omit to use the graph's convention.
    #[serde(default, rename = "normalFormat", skip_serializing_if = "Option::is_none")]
    pub normal_format: Option<String>,
    /// Typed exposed parameter overrides for this cook.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "values_schema")]
    pub values: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// Image input identifier to path below the read root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<std::collections::BTreeMap<String, String>>,
}

fn size_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"anyOf":[{"type":"integer","minimum":0},{"type":"array","minItems":2,"maxItems":2,"items":{"type":"integer","minimum":0}},{"type":"null"}]})
}

fn values_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":["object","null"],"additionalProperties":{}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_forwards_both_dimensions_and_the_source_normal_convention() {
        let request = json!({"dir":".", "profile":"materialx", "size":[128,64], "normalFormat":"directx"});

        let params: ExportParams = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(params).unwrap(), request);
    }
}
