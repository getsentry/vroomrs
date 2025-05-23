use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    sample::v1::Measurement,
    types::{self, ClientSDK, DebugMeta, Platform, ProfileInterface},
};

use super::Android;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct AndroidProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    android_api_level: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    architecture: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    build_id: Option<String>,

    client_sdk: Option<ClientSDK>,

    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    debug_meta: DebugMeta,

    device_classification: String,

    device_locale: String,

    device_manufacturer: String,

    device_model: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    device_os_build_number: Option<String>,

    device_os_name: String,

    device_os_version: String,

    duration_ns: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    js_profile: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    measurements: Option<HashMap<String, Measurement>>,

    organization_id: u64,

    platform: Platform,

    profile: Android,

    profile_id: String,

    project_id: u64,

    received: f64,

    retention_days: i32,

    timestamp: DateTime<Utc>,

    trace_id: String,

    transaction_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    transaction_metadata: Option<types::TransactionMetadata>,

    transaction_name: String,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    transaction_tags: HashMap<String, String>,

    version_code: String,

    version_name: String,
}

impl ProfileInterface for AndroidProfile {
    fn get_platform(&self) -> Platform {
        self.platform
    }
}

#[cfg(test)]
mod tests {
    use serde_path_to_error::Error;

    use super::AndroidProfile;

    #[test]
    fn test_android_valid() {
        let payload = include_bytes!("../../tests/fixtures/android/profile/valid.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<AndroidProfile, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }
}
