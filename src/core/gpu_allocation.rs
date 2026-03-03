use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// GPU assignment strategy when selecting from currently available GPU indices.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, EnumString, Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum GpuAllocationStrategy {
    /// Keep deterministic ordering (lowest indices first).
    #[default]
    Sequential,
    /// Randomize assignment order to spread allocations across devices.
    Random,
}
