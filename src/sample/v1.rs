use crate::{
    frame::Frame,
    types::{self, ClientSDK, DebugMeta, Platform, ProfileInterface},
};

use super::ThreadMetadata;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct OSMetadata {
    name: String,
    version: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    build_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Measurement {
    unit: String,
    values: Vec<MeasurementValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MeasurementValue {
    elapsed_since_start_ns: u64,
    value: f64,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Device {
    architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    classification: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RuntimeMetadata {
    name: String,
    version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueueMetadata {
    label: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sample {
    stack_id: usize,
    thread_id: u64,
    elapsed_since_start_ns: u64,

    // cocoa only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    queue_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sate: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    frames: Vec<Frame>,
    queue_metadata: HashMap<String, QueueMetadata>,
    samples: Vec<Sample>,
    stacks: Vec<Vec<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_metadata: Option<HashMap<String, ThreadMetadata>>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct SampleProfile {
    client_sdk: Option<ClientSDK>,

    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    debug_meta: DebugMeta,

    device: Device,

    environment: Option<String>,

    event_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    measurements: Option<HashMap<String, Measurement>>,

    os: OSMetadata,

    organization_id: u64,

    platform: Platform,

    project_id: u64,

    received: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    release: Option<String>,

    retention_days: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<RuntimeMetadata>,

    timestamp: DateTime<Utc>,

    transaction: types::Transaction,

    #[serde(skip_serializing_if = "Option::is_none")]
    transaction_metadata: Option<types::TransactionMetadata>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    transaction_tags: HashMap<String, String>,

    version: String,
}

impl ProfileInterface for SampleProfile {
    fn get_platform(&self) -> Platform {
        self.platform
    }
}

#[cfg(test)]
mod tests {

    use serde_path_to_error::Error;

    use crate::sample::v1::SampleProfile;

    #[test]
    fn test_sample_format_v1_cocoa() {
        let payload = include_bytes!("../../tests/fixtures/sample/v1/valid_cocoa.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleProfile, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }

    #[test]
    fn test_sample_format_v1_python() {
        let payload = include_bytes!("../../tests/fixtures/sample/v1/valid_python.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleProfile, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }
}
