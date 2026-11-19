use super::TextureDepth;
use serde::{Deserialize, Serialize};

/// Destinations and bake settings for the selected source graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MaterialExportSettings {
    /// Folder the MaterialX document and its resources go to
    pub folder: String,
    /// The .mcmat archive path
    pub archive: String,
    /// Pixels of the larger side; 0 keeps the graph's size
    pub size: u32,
    /// Source tangent convention when the output graph does not identify it
    pub normal_y_down: Option<bool>,
    pub texture_depth: TextureDepth,
    #[serde(skip)]
    pub root: String,
    /// The open package this dialog was created for; never restore an identity from preferences.
    #[serde(skip)]
    pub document: Option<u64>,
    /// Height in scene units of a white displacement texel
    pub displacement_scale: f32,
}

impl Default for MaterialExportSettings {
    fn default() -> Self {
        Self {
            folder: String::new(),
            archive: String::new(),
            size: 0,
            normal_y_down: None,
            texture_depth: TextureDepth::default(),
            root: String::new(),
            document: None,
            displacement_scale: 1.0,
        }
    }
}
