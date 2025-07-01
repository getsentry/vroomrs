use std::{borrow::Cow, cell::RefCell, collections::HashMap, ops::Mul, rc::Rc, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    nodetree::Node,
    types::{CallTreeError, CallTreesStr, ChunkInterface, ClientSDK, DebugMeta, Platform},
};

use super::Android;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AndroidChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    build_id: Option<String>,
    chunk_id: String,
    profiler_id: String,
    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    debug_meta: DebugMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_sdk: Option<ClientSDK>,
    duration_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    platform: Platform,
    #[serde(skip_serializing_if = "Option::is_none")]
    release: Option<String>,
    timestamp: f64,

    profile: Android,
    measurements: Option<serde_json::Value>,

    organization_id: u64,
    project_id: u64,
    received: f64,
    retention_days: i32,
}

impl ChunkInterface for AndroidChunk {
    fn call_trees(
        &mut self,
        _active_thread_id: Option<&str>,
    ) -> Result<CallTreesStr, CallTreeError> {
        self.profile.sdk_start_time = Some(self.timestamp.mul(1e9) as u64);
        let call_trees = self.profile.call_trees()?;

        let mut trees_by_thread_id: HashMap<Cow<str>, Vec<Rc<RefCell<Node>>>> = HashMap::new();
        for (tid, call_tree) in call_trees {
            trees_by_thread_id
                .entry(Cow::Owned(tid.to_string()))
                .insert_entry(call_tree);
        }
        Ok(trees_by_thread_id)
    }

    fn normalize(&mut self) {}

    fn get_environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    fn get_chunk_id(&self) -> &str {
        &self.chunk_id
    }

    fn get_organization_id(&self) -> u64 {
        self.organization_id
    }

    fn get_platform(&self) -> Platform {
        self.platform
    }

    fn get_profiler_id(&self) -> &str {
        &self.profiler_id
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

    fn duration_ms(&self) -> u64 {
        Duration::from_nanos(self.duration_ns).as_millis() as u64
    }

    fn end_timestamp(&self) -> f64 {
        self.timestamp + (self.duration_ns as f64 * 1e-9)
    }

    fn start_timestamp(&self) -> f64 {
        self.timestamp
    }

    fn sdk_name(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.name.as_str())
    }

    fn sdk_version(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.version.as_str())
    }

    fn storage_path(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.organization_id, self.project_id, self.profiler_id, self.chunk_id
        )
    }

    /// Serialize the given data structure as a JSON byte vector.
    fn to_json_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
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
        assert!(r.is_ok(), "{r:#?}")
    }
}
