use serde::{Deserialize, Serialize};

use crate::types::{ClientSDK, DebugMeta, Platform};

use super::Android;

#[derive(Serialize, Deserialize, Debug)]
pub struct AndroidChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    pub chunk_id: String,
    pub profiler_id: String,
    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    pub debug_meta: DebugMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_sdk: Option<ClientSDK>,
    pub duration_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub platform: Platform,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    pub timestamp: f64,

    pub profile: Android,
    pub measurements: Option<serde_json::Value>,

    pub organization_id: u64,
    pub project_id: u64,
    pub received: f64,
    pub retention_days: i32,
}

#[cfg(test)]
mod tests {
    use serde_path_to_error::Error;

    use super::AndroidChunk;

    #[test]
    fn test_android_valid() {
        let payload = include_bytes!("../../tests/fixtures/android/chunk/valid.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<AndroidChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }
}
