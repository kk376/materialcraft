pub(super) const MAX_XML_NODES: u32 = 8_000_000;

pub(crate) const MAX_GRAPHS: usize = 4096;

pub(crate) const MAX_NODES: usize = 100_000;

pub(super) const UPDATER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const SBS_EXT: &str = "sbs";
pub(super) const MAX_WARNINGS: usize = 600;

/// Largest SBS document read (audit: MaterialCraft #2).
pub(super) const MAX_SBS_BYTES: u64 = 256 << 20;

/// Most wires one SBS may carry, across all graphs (audit: MaterialCraft #2).
pub(super) const MAX_WIRES: usize = 1_000_000;
pub(super) const SNIFF_BYTES: usize = 4096;
pub(super) const SELF_DEPENDENCY: &str = "?himself";
pub(super) const FIRST_UID: u64 = 1_000_000_000;
pub(super) const NODE_HALF_SIZE: [f32; 2] = [50.0, 40.0];
