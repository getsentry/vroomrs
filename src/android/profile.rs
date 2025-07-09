use std::{borrow::Cow, collections::HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    android::AndroidError,
    sample::v1::{Measurement, Profile, RuntimeMetadata, SampleProfile},
    types::{
        CallTreeError, ClientSDK, DebugMeta, Platform, ProfileInterface, Transaction,
        TransactionMetadata,
    },
};

use super::Android;

static MAX_PROFILE_DURATION_FOR_CALL_TREES: u64 = 15_000_000_000;

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

    sampled: bool,

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

// NestedProfile is used to deserialize the js_profile
// when one is present.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct NestedProfile {
    profile: Profile,
    #[serde(skip_serializing_if = "Option::is_none")]
    processed_by_symbolicator: Option<bool>,
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

    fn get_timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn normalize(&mut self) {
        if let Some(js_profile_json) = &mut self.js_profile {
            let mut js_profile: NestedProfile = serde_json::from_value(js_profile_json.clone())
                .expect("error while deserializing js_profile");
            let mut sample_profile = SampleProfile {
                platform: Platform::JavaScript,
                profile: js_profile.profile,
                ..Default::default()
            };
            sample_profile.normalize();
            js_profile.profile = sample_profile.profile;
            let js_profile_value = serde_json::to_value(js_profile);
            if let Ok(value) = js_profile_value {
                self.js_profile = Some(value);
            }
        }
        if self
            .build_id
            .as_mut()
            .is_some_and(|build_id| !build_id.is_empty())
        {
            if let Some(images) = &mut self.debug_meta.images {
                images.push(crate::debug_images::Image {
                    r#type: Some("proguard".to_string()),
                    uuid: Some(self.build_id.as_ref().unwrap().into()),
                    ..Default::default()
                });
            }
        }
        self.build_id = None;
    }

    fn call_trees(&mut self) -> Result<crate::types::CallTreesU64, crate::types::CallTreeError> {
        // Profiles longer than 15s contain a lot of call trees and it produces a lot of noise for the aggregation.
        // The majority of them might also be timing out and we want to ignore them for the aggregation.
        if self.duration_ns > MAX_PROFILE_DURATION_FOR_CALL_TREES {
            return Ok(HashMap::new());
        }
        // this is to handle only the Reactnative (android + js)
        // use case. If it's an Android profile but there is no
        // js profile, we'll skip this entirely
        if let Some(js_profile_json) = &mut self.js_profile {
            let js_profile: NestedProfile = serde_json::from_value(js_profile_json.clone())
                .expect("error while deserializing js_profile");
            let mut sample_profile = SampleProfile {
                platform: Platform::JavaScript,
                profile: js_profile.profile,
                ..Default::default()
            };
            // if we're in this branch we know for sure that here
            // we're dealing with a react-native profile so we can
            // set the runtime for this profile to hermes.
            // This way, we'll be able to differentiate in other parts
            // of the codebase between normal js frames and react-native
            // js frames when we traverse the call trees
            sample_profile.runtime = Some(RuntimeMetadata {
                name: "hermes".to_string(),
                ..Default::default()
            });
            match fill_sample_profile_metadata(&mut sample_profile) {
                Ok(_) => return sample_profile.call_trees(),
                Err(err) => {
                    return Err(CallTreeError::Android(
                        AndroidError::FillSampleMetadataError(err),
                    ))
                }
            }
        } // end if js_profile
        self.profile.call_trees()
    }

    fn storage_path(&self) -> String {
        format!(
            "{}/{}/{}",
            self.organization_id, self.project_id, self.profile_id
        )
    }

    fn sdk_name(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.name.as_str())
    }

    fn sdk_version(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.version.as_str())
    }

    fn duration_ns(&self) -> u64 {
        self.duration_ns
    }

    fn get_transaction(&self) -> Cow<Transaction> {
        Cow::Owned(Transaction {
            active_thread_id: self.profile.active_thread_id(),
            duration_ns: Some(self.duration_ns),
            id: self.transaction_id.clone(),
            name: self.transaction_name.clone(),
            trace_id: self.trace_id.clone(),
            segment_id: self
                .transaction_metadata
                .as_ref()
                .unwrap()
                .segment_id
                .as_ref()
                .map_or("".to_string(), |segment| segment.clone()),
        })
    }

    fn get_transaction_tags(&self) -> &HashMap<String, String> {
        &self.transaction_tags
    }

    fn get_debug_meta(&self) -> &DebugMeta {
        &self.debug_meta
    }

    fn get_measurements(&self) -> Option<&HashMap<String, Measurement>> {
        self.measurements.as_ref()
    }

    fn is_sampled(&self) -> bool {
        self.sampled
    }

    fn set_profile_id(&mut self, profile_id: String) {
        self.profile_id = profile_id
    }
}

// CallTree generation expect activeThreadID to be set in
// order to be able to choose which samples should be used
// for the aggregation.
// Here we set it to the only thread that the js profile has.
fn fill_sample_profile_metadata(
    sample_profile: &mut SampleProfile,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some((thread_id, _)) = sample_profile
        .profile
        .thread_metadata
        .as_ref()
        .expect("cannot fill profile metadata: missing thread metadata")
        .iter()
        .next()
    {
        sample_profile.transaction = Transaction {
            active_thread_id: thread_id.parse::<u64>()?,
            ..Default::default()
        };
        return Ok(());
    }
    Err("cannot fill profile metadata: missing thread metadata".into())
}

#[cfg(test)]
mod tests {
    use serde_path_to_error::Error;

    use crate::{
        debug_images::Image,
        types::{DebugMeta, ProfileInterface},
    };

    use super::AndroidProfile;

    #[test]
    fn test_android_valid() {
        let payload = include_bytes!("../../tests/fixtures/android/profile/valid.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<AndroidProfile, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{r:#?}")
    }

    #[test]
    fn test_normalize_android_profile_with_js_profile() {
        use pretty_assertions::assert_eq;
        struct TestStruct {
            name: String,
            profile: AndroidProfile,
            want: AndroidProfile,
        }

        let mut test_cases = [
            TestStruct {
                name: "Classify [Native] frames as system frames".to_string(),
                profile: AndroidProfile {
                    js_profile: Some(serde_json::from_str(r#"{
                        "profile":{
                            "frames":[
                                {"function":"[Native] functionPrototypeApply"}
                            ],
                            "samples":[
                                {"stack_id": 0, "thread_id": 1, "elapsed_since_start_ns": 1000}
                            ],
                            "stacks":[
                                [0]
                            ]
                        }
                    }"#).expect("failed to parse JSON string into serde_json::Value")),
                    ..Default::default()
                },
                want: AndroidProfile {
                    js_profile: Some(serde_json::from_str(r#"{"profile":{"frames":[{"data":null,"function":"[Native] functionPrototypeApply","in_app":false,"platform":"javascript"}],"samples":[{"elapsed_since_start_ns":1000,"stack_id":0,"thread_id":1}],"stacks":[[0]]}}"#).expect("failed to parse JSON string into serde_json::Value")),
                    ..Default::default()
                },
            },
        ];

        for test_case in test_cases.as_mut() {
            //let call_trees = test_case.chunk.call_trees(None).unwrap();
            test_case.profile.normalize();
            assert_eq!(
                test_case.profile.js_profile, test_case.want.js_profile,
                "test: {} failed.",
                test_case.name
            );
        }
    }

    #[test]
    fn test_normalize_android_profile_build_id() {
        use pretty_assertions::assert_eq;
        struct TestStruct {
            name: String,
            profile: AndroidProfile,
            want: AndroidProfile,
        }

        let mut test_cases = [TestStruct {
            name: "set build_id in the debug images".to_string(),
            profile: AndroidProfile {
                build_id: Some("a1bd-e45t".to_string()),
                debug_meta: DebugMeta {
                    images: Some(vec![]),
                },
                ..Default::default()
            },
            want: AndroidProfile {
                debug_meta: DebugMeta {
                    images: Some(vec![Image {
                        r#type: Some("proguard".to_string()),
                        uuid: Some("a1bd-e45t".to_string()),
                        ..Default::default()
                    }]),
                },
                ..Default::default()
            },
        }];
        for test_case in test_cases.as_mut() {
            test_case.profile.normalize();
            assert_eq!(
                test_case.profile, test_case.want,
                "test: {} failed.",
                test_case.name
            );
        }
    }
}
