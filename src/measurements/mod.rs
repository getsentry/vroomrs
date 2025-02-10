use serde::{Serialize, Deserialize};

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
