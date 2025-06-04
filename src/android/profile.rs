use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    sample::v1::Measurement,
    types::{ClientSDK, DebugMeta, Platform, ProfileInterface, TransactionMetadata},
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

    release: Option<String>,

    retention_days: i32,

    timestamp: DateTime<Utc>,

    trace_id: String,

    transaction_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    transaction_metadata: Option<TransactionMetadata>,

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

    /// Serialize the given data structure as a JSON byte vector.
    fn to_json_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    fn get_profile_id(&self) -> &str {
        &self.profile_id
    }

    fn get_organization_id(&self) -> u64 {
        self.organization_id
    }

    fn get_project_id(&self) -> u64 {
        self.project_id
    }

    fn get_received(&self) -> f64 {
        self.received
    }

    fn get_release(&self) -> Option<&str> {
        self.release.as_deref()
    }

    fn get_retention_days(&self) -> i32 {
        self.retention_days
    }

    fn get_timestamp(&self) -> f64 {
        self.timestamp.timestamp_micros() as f64 / 1_000_000.0
    }

    fn normalize(&mut self) {
        todo!()
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
