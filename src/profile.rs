use pyo3::pyclass;
use serde_json::Error;

use crate::{
    android::chunk::AndroidChunk,
    sample::v2::SampleChunk,
    types::{ChunkInterface, Platform},
};

#[pyclass]
pub struct ProfileChunk {
    pub profile: Box<dyn ChunkInterface + Send + Sync>,
}

#[derive(serde::Deserialize)]
struct MinimumProfile {
    version: Option<String>,
}

impl ProfileChunk {
    pub fn from_json_string(profile: &str) -> Result<Self, Error> {
        let min_prof: MinimumProfile = serde_json::from_str(profile)?;
        match min_prof.version {
            None => {
                let android: AndroidChunk = serde_json::from_str(profile)?;
                Ok(ProfileChunk {
                    profile: Box::new(android),
                })
            }
            Some(_) => {
                let sample: SampleChunk = serde_json::from_str(profile)?;
                Ok(ProfileChunk {
                    profile: Box::new(sample),
                })
            }
        }
    }

    pub fn normalize(&mut self) {
        self.profile.normalize();
    }

    pub fn get_environment(&self) -> Option<&str> {
        self.profile.get_environment()
    }

    pub fn get_id(&self) -> &str {
        self.profile.get_chunk_id()
    }

    pub fn get_organization_id(&self) -> u64 {
        self.profile.get_organization_id()
    }

    pub fn get_platform(&self) -> Platform {
        self.profile.get_platform()
    }

    pub fn get_profiler_id(&self) -> &str {
        self.profile.get_profiler_id()
    }

    pub fn get_project_id(&self) -> u64 {
        self.profile.get_project_id()
    }

    pub fn get_received(&self) -> f64 {
        self.profile.get_received()
    }

    pub fn get_release(&self) -> Option<&str> {
        self.profile.get_release()
    }

    pub fn get_retention_days(&self) -> i32 {
        self.profile.get_retention_days()
    }

    pub fn duration_ms(&self) -> u64 {
        self.profile.duration_ms()
    }

    pub fn start_timestamp(&self) -> f64 {
        self.profile.start_timestamp()
    }

    pub fn end_timestamp(&self) -> f64 {
        self.profile.end_timestamp()
    }

    pub fn sdk_name(&self) -> Option<&str> {
        self.profile.sdk_name()
    }

    pub fn sdk_version(&self) -> Option<&str> {
        self.profile.sdk_version()
    }

    pub fn storage_path(&self) -> String {
        self.profile.storage_path()
    }
}

#[cfg(test)]
mod tests {
    use crate::{profile::ProfileChunk, types::Platform};

    #[test]
    fn test_from_json_string() {
        struct TestStruct {
            name: String,
            profile_json: &'static str,
            want: Platform,
        }

        let test_cases = [
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_str!("../tests/fixtures/sample/v2/valid_cocoa.json"),
                want: Platform::Cocoa,
            },
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_str!("../tests/fixtures/sample/v2/valid_python.json"),
                want: Platform::Python,
            },
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_str!("../tests/fixtures/android/chunk/valid.json"),
                want: Platform::Android,
            },
        ];

        for test in test_cases {
            let prof = ProfileChunk::from_json_string(test.profile_json);
            assert!(prof.is_ok());
            assert_eq!(
                prof.unwrap().get_platform(),
                test.want,
                "test `{}` failed",
                test.name
            )
        }
    }
}
