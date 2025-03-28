use std::{
    collections::HashMap,
    io::{self},
};

use lz4::{Decoder, EncoderBuilder};
use pyo3::{pyclass, pymethods, PyErr, PyResult};

use crate::{
    android::chunk::AndroidChunk,
    nodetree::CallTreeFunction,
    sample::v2::SampleChunk,
    types::{CallTreesStr, ChunkInterface, Platform},
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
    pub(crate) fn from_json_vec(profile: &[u8]) -> Result<Self, serde_json::Error> {
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

    pub fn decompress(source: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = decompress(source)?;
        Self::from_json_vec(bytes.as_ref())
            .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)
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

    pub fn compress(&self) -> PyResult<Vec<u8>> {
        let prof = self
            .profile
            .to_json_vec()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        compress(&mut prof.as_slice())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    #[pyo3(signature = (min_depth, filter_system_frames, max_unique_functions=None))]
    pub fn extract_functions_metrics(
        &mut self,
        min_depth: u16,
        filter_system_frames: bool,
        max_unique_functions: Option<usize>,
    ) -> PyResult<Vec<CallTreeFunction>> {
        let call_trees: CallTreesStr = self.profile.call_trees(None)?;
        let mut functions: HashMap<u32, CallTreeFunction> = HashMap::new();

        for (tid, call_trees_for_thread) in &call_trees {
            for call_tree in call_trees_for_thread {
                call_tree
                    .borrow_mut()
                    .collect_functions(&mut functions, tid, 0, min_depth);
            }
        }

        let mut functions_list: Vec<CallTreeFunction> = Vec::with_capacity(functions.len());
        for (_fingerprint, function) in functions {
            if function.sample_count <= 1 || (filter_system_frames && !function.in_app) {
                // if there's only ever a single sample for this function in
                // the profile, or the function represents a system frame, and we
                // decided to ignore system frames, we skip over it to reduce the
                //amount of data
                continue;
            }
            functions_list.push(function);
        }

        // sort the list in descending order, and take the top N results
        functions_list.sort_by(|a, b| b.sum_self_time_ns.cmp(&a.sum_self_time_ns));

        functions_list.truncate(max_unique_functions.unwrap_or(functions_list.len()));
        Ok(functions_list)
    }
}

fn compress(source: &mut &[u8]) -> Result<Vec<u8>, std::io::Error> {
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

fn decompress(source: &[u8]) -> Result<Vec<u8>, std::io::Error> {
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
