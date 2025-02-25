use serde::{Deserialize, Serialize};

use crate::frame::Frame;
use crate::types::{ClientSDK, DebugMeta, Platform};

#[derive(Serialize, Deserialize, Debug)]
pub struct SampleChunk {
    #[serde(rename = "chunk_id")]
    pub id: String,

    pub profiler_id: String,

    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    pub debug_meta: DebugMeta,

    pub client_sdk: Option<ClientSDK>,

    pub environment: Option<String>,

    pub platform: Platform,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,

    pub version: String,

    pub profile: SampleData,

    pub organization_id: u64,

    pub project_id: u64,

    pub received: f64,

    pub retention_days: i32,

    // `measurements` contains CPU/memory measurements we do during the capture of the chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurements: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ThreadMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SampleData {
    pub frames: Vec<Frame>,
    pub samples: Vec<Sample>,
    pub stacks: Vec<Vec<i32>>,
    pub thread_metadata: std::collections::HashMap<String, ThreadMetadata>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Sample {
    #[serde(rename = "stack_id")]
    pub stack_id: i32,
    #[serde(rename = "thread_id")]
    pub thread_id: String,
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
}

#[cfg(test)]
mod tests {
    use serde_path_to_error::Error;

    use super::SampleChunk;

    #[test]
    fn test_sample_format_v2_cocoa() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_cocoa.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }

    #[test]
    fn test_sample_format_v2_python() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_python.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }
}
