use serde::{Deserialize, Serialize};

use crate::debug_images::Image;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChunkMeasurement {
    unit: MeasurementUnit,
    values: Vec<ChunkMeasurementValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementUnit {
    #[serde(alias = "ns")]
    Nanosecond,
    #[serde(alias = "hz")]
    Hertz,
    Byte,
    Percent,
    #[serde(alias = "nj")]
    Nanojoule,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChunkMeasurementValue {
    // UNIX timestamp in seconds as a float
    timestamp: f64,

    value: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Android,
    Cocoa,
    Java,
    JavaScript,
    Node,
    Php,
    Python,
    Rust,
    #[default]
    None,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientSDK {
    pub name: String,
    pub version: String,
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct DebugMeta {
    pub images: Vec<Image>,
}

impl DebugMeta {
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}
