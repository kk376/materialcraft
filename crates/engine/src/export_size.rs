/// The pixel size `file.export` cooks a graph at
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ExportSize {
    /// The larger side in pixels; the other keeps the graph's proportions
    LargerSide(u32),
    /// Width and height in pixels
    Exact([u32; 2]),
}

impl ExportSize {
    /// The graph's log2 size at this export size: pixels round down to powers of two from 16 to 8192
    pub fn log2_size(self, graph_size: [u32; 2]) -> [u32; 2] {
        match self {
            ExportSize::LargerSide(pixels) => {
                let target = log2_pixels(pixels);
                let bounded = graph_size.map(|side| side.clamp(materialcraft_core::MIN_SIZE_LOG2, materialcraft_core::MAX_SIZE_LOG2));
                let larger = bounded[0].max(bounded[1]);
                bounded.map(|side| (side + target).saturating_sub(larger).max(materialcraft_core::MIN_SIZE_LOG2))
            }
            ExportSize::Exact(pixels) => pixels.map(log2_pixels),
        }
    }
}

fn log2_pixels(pixels: u32) -> u32 {
    pixels.clamp(1 << materialcraft_core::MIN_SIZE_LOG2, 1 << materialcraft_core::MAX_SIZE_LOG2).ilog2()
}
