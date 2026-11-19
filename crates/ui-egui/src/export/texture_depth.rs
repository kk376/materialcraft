use serde::{Deserialize, Serialize};

/// PNG precision of the published textures; HDR and signed data always use lossless EXR
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureDepth {
    /// 16 bits for data textures and 8 bits for color
    #[default]
    Automatic,
    Eight,
    Sixteen,
}

impl TextureDepth {
    pub const ALL: [TextureDepth; 3] = [TextureDepth::Automatic, TextureDepth::Eight, TextureDepth::Sixteen];

    pub fn label(self) -> &'static str {
        match self {
            TextureDepth::Automatic => "Automatic (16-bit data, 8-bit color)",
            TextureDepth::Eight => "8 bits",
            TextureDepth::Sixteen => "16 bits",
        }
    }
}
