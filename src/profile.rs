use std::io::{self, Result};

use lz4::{Decoder, EncoderBuilder};
use pyo3::{pyclass, pymethods};

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
    pub(crate) fn from_json_vec(profile: &[u8]) -> Result<Self> {
        let min_prof: MinimumProfile = serde_json::from_slice(profile)?;
        match min_prof.version {
            None => {
                let android: AndroidChunk = serde_json::from_slice(profile)?;
                Ok(ProfileChunk {
                    profile: Box::new(android),
                })
            }
            Some(_) => {
                let sample: SampleChunk = serde_json::from_slice(profile)?;
                Ok(ProfileChunk {
                    profile: Box::new(sample),
                })
            }
        }
    }

    pub fn decompress(source: &[u8]) -> Result<Self> {
        let bytes = decompress(source)?;
        Self::from_json_vec(bytes.as_ref())
    }
}

#[pymethods]
impl ProfileChunk {
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

    pub fn compress(&self) -> Result<Vec<u8>> {
        let prof = self.profile.to_json_vec()?;
        compress(&mut prof.as_slice())
    }
}

fn compress(source: &mut &[u8]) -> Result<Vec<u8>> {
    let b: Vec<u8> = vec![];
    let mut encoder = EncoderBuilder::new()
        .block_checksum(lz4::liblz4::BlockChecksum::NoBlockChecksum)
        .level(9)
        .build(b)?;
    io::copy(source, &mut encoder)?;
    let (compressed_data, res) = encoder.finish();
    match res {
        Ok(_) => Ok(compressed_data),
        Err(error) => Err(error),
    }
}

fn decompress(source: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = Decoder::new(source)?;
    let mut decoded_data: Vec<u8> = vec![];
    io::copy(&mut decoder, &mut decoded_data)?;
    Ok(decoded_data)
}

#[cfg(test)]
mod tests {
    use crate::{
        android::chunk::AndroidChunk, profile::ProfileChunk, sample::v2::SampleChunk,
        types::Platform,
    };

    #[test]
    fn test_from_json_vec() {
        struct TestStruct {
            name: String,
            profile_json: &'static [u8],
            want: Platform,
        }

        let test_cases = [
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/sample/v2/valid_cocoa.json"),
                want: Platform::Cocoa,
            },
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/sample/v2/valid_python.json"),
                want: Platform::Python,
            },
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/android/chunk/valid.json"),
                want: Platform::Android,
            },
        ];

        for test in test_cases {
            let prof = ProfileChunk::from_json_vec(test.profile_json);
            assert!(prof.is_ok());
            assert_eq!(
                prof.unwrap().get_platform(),
                test.want,
                "test `{}` failed",
                test.name
            )
        }
    }

    #[test]
    fn test_compress_decompress() {
        struct TestStruct {
            name: String,
            payload: &'static [u8],
        }

        let test_cases = [
            TestStruct {
                name: "compressing and decompressing cocoa (V2)".to_string(),
                payload: include_bytes!("../tests/fixtures/sample/v2/valid_cocoa.json"),
            },
            TestStruct {
                name: "compressing and decompressing python (V2)".to_string(),
                payload: include_bytes!("../tests/fixtures/sample/v2/valid_python.json"),
            },
            TestStruct {
                name: "compressing and decompressing android chunk".to_string(),
                payload: include_bytes!("../tests/fixtures/android/chunk/valid.json"),
            },
        ];

        for test in test_cases {
            let profile = ProfileChunk::from_json_vec(test.payload).unwrap();

            let compressed_profile_bytes = profile.compress().unwrap();
            let decompressed_profile =
                ProfileChunk::decompress(compressed_profile_bytes.as_slice()).unwrap();

            let equals = if profile.get_platform() == Platform::Android {
                let original_sample = profile
                    .profile
                    .as_any()
                    .downcast_ref::<AndroidChunk>()
                    .unwrap();
                let final_sample = decompressed_profile
                    .profile
                    .as_any()
                    .downcast_ref::<AndroidChunk>()
                    .unwrap();
                original_sample == final_sample
            } else {
                let original_sample = profile
                    .profile
                    .as_any()
                    .downcast_ref::<SampleChunk>()
                    .unwrap();
                let final_sample = decompressed_profile
                    .profile
                    .as_any()
                    .downcast_ref::<SampleChunk>()
                    .unwrap();
                original_sample == final_sample
            };

            assert!(equals, "test `{}` failed", test.name);
        }
    }
}
