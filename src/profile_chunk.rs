use std::collections::HashMap;

use pyo3::{pyclass, pymethods, PyErr, PyResult};

use crate::{
    android::chunk::AndroidChunk,
    nodetree::CallTreeFunction,
    sample::v2::SampleChunk,
    types::{Attachment, CallTreesStr, ChunkInterface},
    utils::{compress_lz4, decompress_lz4},
};

/// This is a :class:`ProfileChunk` class
#[pyclass]
pub struct ProfileChunk {
    pub profile: Box<dyn ChunkInterface + Send + Sync>,
}

#[derive(serde::Deserialize)]
struct MinimumProfile {
    #[serde(default)]
    platform: String,
    version: Option<String>,
    // Present only on legacy android profiles: the raw, unparsed profile.
    // Ignored here, we only care whether it's set.
    sampled_profile: Option<serde::de::IgnoredAny>,
}

impl ProfileChunk {
    pub(crate) fn from_json_vec(profile: &[u8]) -> Result<Self, serde_json::Error> {
        let min_prof: MinimumProfile = serde_json::from_slice(profile)?;
        Self::from_json_vec_with_profile(profile, &min_prof.platform, &min_prof)
    }

    pub(crate) fn from_json_vec_and_platform(
        profile: &[u8],
        platform: &str,
    ) -> Result<Self, serde_json::Error> {
        let min_prof: MinimumProfile = serde_json::from_slice(profile)?;
        Self::from_json_vec_with_profile(profile, platform, &min_prof)
    }

    fn from_json_vec_with_profile(
        profile: &[u8],
        platform: &str,
        min_prof: &MinimumProfile,
    ) -> Result<Self, serde_json::Error> {
        // A chunk is a legacy android chunk when its platform is android and it
        // either carries no version (the original legacy format) or still contains
        // a `sampled_profile`. The latter is an interim fix: some legacy android
        // profiles are sent with version "2" even though they don't follow the
        // sample v2 format.
        let is_legacy_android = platform == "android"
            && (min_prof.version.is_none() || min_prof.sampled_profile.is_some());
        if is_legacy_android {
            let android: AndroidChunk = serde_json::from_slice(profile)?;
            Ok(ProfileChunk {
                profile: Box::new(android),
            })
        } else {
            let sample: SampleChunk = serde_json::from_slice(profile)?;
            Ok(ProfileChunk {
                profile: Box::new(sample),
            })
        }
    }

    pub(crate) fn decompress(source: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = decompress_lz4(source)?;
        Self::from_json_vec(bytes.as_ref())
            .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)
    }
}

#[pymethods]
impl ProfileChunk {
    /// Applies the various normalization steps,
    /// depending on the profile's platform.
    pub fn normalize(&mut self) {
        self.profile.normalize();
    }

    /// Returns the environment.
    ///
    /// Returns:
    ///     str
    ///         The environment, or None, if release is not available.
    pub fn get_environment(&self) -> Option<&str> {
        self.profile.get_environment()
    }

    /// Returns the profile chunk ID.
    ///
    /// Returns:
    ///     str
    ///         The profile chunk ID.
    pub fn get_chunk_id(&self) -> &str {
        self.profile.get_chunk_id()
    }

    /// Returns the organization ID.
    ///
    /// Returns:
    ///     int
    ///         The organization ID to which the profile belongs.
    pub fn get_organization_id(&self) -> u64 {
        self.profile.get_organization_id()
    }

    /// Returns the profile platform.
    ///
    /// Returns:
    ///     str
    ///         The profile's platform.
    pub fn get_platform(&self) -> String {
        self.profile.get_platform().to_string()
    }

    /// Returns the profiler ID.
    ///
    /// Returns:
    ///     str
    ///         The profile ID of the profile chunk.
    pub fn get_profiler_id(&self) -> &str {
        self.profile.get_profiler_id()
    }

    /// Returns the project ID.
    ///
    /// Returns:
    ///     int
    ///         The project ID to which the profile belongs.
    pub fn get_project_id(&self) -> u64 {
        self.profile.get_project_id()
    }

    /// Returns the attachments related to this chunk.
    ///
    /// Returns:
    ///     list[Attachment]
    ///         The attachments (e.g. a raw profile) related to this chunk.
    ///         Empty if no attachments are available.
    pub fn get_attachments(&self) -> Vec<Attachment> {
        self.profile.get_attachments().to_vec()
    }

    /// Sets the attachments related to this chunk.
    ///
    /// Attachments are only supported for sample chunks:
    /// this is a no-op for Android chunks.
    ///
    /// Args:
    ///     attachments (list[Attachment]): The attachments related to this
    ///         chunk, replacing any existing ones. An empty list clears them.
    pub fn set_attachments(&mut self, attachments: Vec<Attachment>) {
        self.profile.set_attachments(attachments);
    }

    /// Returns the received timestamp.
    ///
    /// Returns:
    ///     float
    pub fn get_received(&self) -> f64 {
        self.profile.get_received()
    }

    /// Returns the release.
    ///
    /// Returns:
    ///     str
    ///         The release of the SDK used to collect this profile,
    ///         or None, if release is not available.
    pub fn get_release(&self) -> Option<&str> {
        self.profile.get_release()
    }

    /// Returns the retention days.
    ///
    /// Returns:
    ///     int
    ///         The retention days.
    pub fn get_retention_days(&self) -> i32 {
        self.profile.get_retention_days()
    }

    /// Returns the duration of the profile in ms.
    ///
    /// Returns:
    ///     int
    ///         The duration of the profile in ms.
    pub fn duration_ms(&self) -> u64 {
        self.profile.duration_ms()
    }

    /// Returns the start timestamp of the profile.
    ///
    /// The timestamp is a Unix timestamp in seconds
    /// with millisecond precision.
    ///
    /// Returns:
    ///     float
    ///         The start timestamp of the profile.
    pub fn start_timestamp(&self) -> f64 {
        self.profile.start_timestamp()
    }

    /// Returns the end timestamp of the profile.
    ///
    /// The timestamp is a Unix timestamp in seconds
    /// with millisecond precision.
    ///
    /// Returns:
    ///     float
    ///         The end timestamp of the profile.
    pub fn end_timestamp(&self) -> f64 {
        self.profile.end_timestamp()
    }

    /// Returns the SDK name.
    ///
    /// Returns:
    ///     str
    ///         The name of the SDK used to collect this profile,
    ///         or None, if version is not available.
    pub fn sdk_name(&self) -> Option<&str> {
        self.profile.sdk_name()
    }

    /// Returns the SDK version.
    ///
    /// Returns:
    ///     str
    ///         The version of the SDK used to collect this profile,
    ///         or None, if version is not available.
    pub fn sdk_version(&self) -> Option<&str> {
        self.profile.sdk_version()
    }

    /// Returns the storage path of the profile.
    ///
    /// Returns:
    ///     str
    ///         The storage path of the profile.
    pub fn storage_path(&self) -> String {
        self.profile.storage_path()
    }

    /// Compresses the profile with lz4.
    ///
    /// This method serializes the profile to json and then compresses it with lz4,
    /// returning the bytes representing the lz4 encoded profile.
    ///
    /// Returns:
    ///     bytes
    ///         A bytes object representing the lz4 encoded profile.
    ///
    /// Raises:
    ///     pyo3.exceptions.PyException: If an error occurs during the extraction process.
    ///
    /// Example:
    ///     >>> compressed_profile = profile.compress()
    ///     >>> with open("profile_compressed.lz4", "wb+") as binary_file:
    ///     ...     binary_file.write(compressed_profile)
    pub fn compress(&self) -> PyResult<Vec<u8>> {
        let prof = self
            .profile
            .to_json_vec()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        compress_lz4(&mut prof.as_slice())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    /// Extracts function metrics from the profile chunk.
    ///
    /// This method analyzes the call tree and extracts metrics for each function,
    /// returning a list of `CallTreeFunction` objects.
    ///
    /// Args:
    ///     min_depth (int): The minimum depth of the node in the call tree.
    ///         When computing slowest functions, ignore frames/node whose depth in the callTree
    ///         is less than min_depth (i.e. if min_depth=1, we'll ignore root frames).
    ///     filter_system_frames (bool): If `True`, system frames (e.g., standard library calls) will be filtered out.
    ///     max_unique_functions (int): An optional maximum number of unique functions to extract.
    ///         If provided, only the top `max_unique_functions` slowest functions will be returned.
    ///         If `None`, all functions will be returned.
    ///     filter_non_leaf_functions (bool): If `True`, functions with zero self-time (non-leaf functions) will be filtered out.
    ///         If `False`, all functions including non-leaf functions with zero self-time will be included.
    ///         Defaults to `True`.
    ///     generate_stack_fingerprints (bool): If `True`, the fingerprint of the stack up to the current function and the parent function's fingerprint will be generated.
    ///         If `False`, only the fingerprint of the current function will be generated.
    ///         Defaults to `False`.
    ///
    /// Returns:
    ///     list[:class:`CallTreeFunction`]
    ///         A list of :class:`CallTreeFunction` objects, each containing metrics for a function in the call tree.
    ///
    /// Raises:
    ///     pyo3.exceptions.PyException: If an error occurs during the extraction process.
    ///
    /// Example:
    ///     >>> metrics = profile_chunk.extract_functions_metrics(min_depth=2, filter_system_frames=True, max_unique_functions=10, filter_non_leaf_functions=False)
    ///     >>> for function_metric in metrics:
    ///     ...     do_something(function_metric)
    #[pyo3(signature = (min_depth, filter_system_frames, max_unique_functions=None, filter_non_leaf_functions=true, generate_stack_fingerprints=false))]
    pub fn extract_functions_metrics(
        &mut self,
        min_depth: u16,
        filter_system_frames: bool,
        max_unique_functions: Option<usize>,
        filter_non_leaf_functions: bool,
        generate_stack_fingerprints: bool,
    ) -> PyResult<Vec<CallTreeFunction>> {
        let call_trees: CallTreesStr = self.profile.call_trees(None)?;
        let mut functions: HashMap<u32, CallTreeFunction> = HashMap::new();

        for (tid, call_trees_for_thread) in &call_trees {
            for call_tree in call_trees_for_thread {
                call_tree.borrow_mut().collect_functions(
                    &mut functions,
                    tid,
                    0,
                    min_depth,
                    filter_system_frames,
                    filter_non_leaf_functions,
                    generate_stack_fingerprints,
                    None,
                );
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
        functions_list.sort_by_key(|f| std::cmp::Reverse(f.sum_self_time_ns));

        functions_list.truncate(max_unique_functions.unwrap_or(functions_list.len()));
        Ok(functions_list)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        android::chunk::AndroidChunk, profile_chunk::ProfileChunk, sample::v2::SampleChunk,
    };

    #[test]
    fn test_from_json_vec() {
        struct TestStruct {
            name: String,
            profile_json: &'static [u8],
            want: String,
        }

        let test_cases = [
            TestStruct {
                name: "cocoa profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/sample/v2/valid_cocoa.json"),
                want: "cocoa".to_string(),
            },
            TestStruct {
                name: "python profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/sample/v2/valid_python.json"),
                want: "python".to_string(),
            },
            TestStruct {
                name: "android profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/android/chunk/valid.json"),
                want: "android".to_string(),
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
    fn test_android_platform_with_version_is_sample_chunk() {
        // A chunk with platform=android but a version set uses the
        // sample v2 format and must not be treated as a legacy
        // android chunk, no matter how it's deserialized.
        let payload = include_bytes!("../tests/fixtures/sample/v2/valid_cocoa.json");
        let mut value: serde_json::Value = serde_json::from_slice(payload).unwrap();
        value["platform"] = "android".into();
        let json = serde_json::to_vec(&value).unwrap();

        for chunk in [
            ProfileChunk::from_json_vec(&json).unwrap(),
            ProfileChunk::from_json_vec_and_platform(&json, "android").unwrap(),
        ] {
            assert_eq!(chunk.get_platform(), "android");
            assert!(chunk
                .profile
                .as_any()
                .downcast_ref::<SampleChunk>()
                .is_some());
        }

        // Legacy android chunks (no version) still deserialize as such.
        let payload = include_bytes!("../tests/fixtures/android/chunk/valid.json");
        let chunk = ProfileChunk::from_json_vec_and_platform(payload, "android").unwrap();
        assert!(chunk
            .profile
            .as_any()
            .downcast_ref::<AndroidChunk>()
            .is_some());
    }

    #[test]
    fn test_legacy_android_with_sampled_profile_is_android_chunk() {
        // Interim fix: some legacy android profiles are sent with a version
        // (e.g. "2") even though they carry a `sampled_profile` and don't
        // follow the sample v2 format. They must be treated as android chunks.
        let payload = include_bytes!("../tests/fixtures/android/chunk/valid.json");
        let mut value: serde_json::Value = serde_json::from_slice(payload).unwrap();
        value["version"] = "2".into();
        value["sampled_profile"] = "AAAAA".into();
        let json = serde_json::to_vec(&value).unwrap();

        for chunk in [
            ProfileChunk::from_json_vec(&json).unwrap(),
            ProfileChunk::from_json_vec_and_platform(&json, "android").unwrap(),
        ] {
            assert!(chunk
                .profile
                .as_any()
                .downcast_ref::<AndroidChunk>()
                .is_some());
        }
    }

    #[test]
    fn test_attachments_survive_compression() {
        use crate::types::Attachment;

        // The sentry writer flow: deserialize the chunk, stamp the
        // attachments, compress and store. The attachments must survive
        // into the stored chunk representation.
        let payload = include_bytes!("../tests/fixtures/sample/v2/valid_cocoa.json");
        let mut chunk = ProfileChunk::from_json_vec(payload).unwrap();
        let attachments = vec![Attachment {
            name: "raw_profile".to_string(),
            content_type: Some("application/x-perfetto".to_string()),
            stored_id: "aef123345".to_string(),
        }];
        chunk.set_attachments(attachments.clone());
        assert_eq!(chunk.get_attachments(), attachments);

        let compressed = chunk.compress().unwrap();
        let decompressed = ProfileChunk::decompress(compressed.as_slice()).unwrap();
        assert_eq!(decompressed.get_attachments(), attachments);
    }

    #[test]
    fn test_from_json_vec_and_platform() {
        struct TestStruct<'a> {
            name: String,
            platform: &'a str,
            profile_json: &'static [u8],
            want: String,
        }

        let test_cases = [
            TestStruct {
                name: "cocoa profile".to_string(),
                platform: "cocoa",
                profile_json: include_bytes!("../tests/fixtures/sample/v2/valid_cocoa.json"),
                want: "cocoa".to_string(),
            },
            TestStruct {
                name: "python profile".to_string(),
                platform: "python",
                profile_json: include_bytes!("../tests/fixtures/sample/v2/valid_python.json"),
                want: "python".to_string(),
            },
            TestStruct {
                name: "android profile".to_string(),
                platform: "android",
                profile_json: include_bytes!("../tests/fixtures/android/chunk/valid.json"),
                want: "android".to_string(),
            },
        ];

        for test in test_cases {
            let prof = ProfileChunk::from_json_vec_and_platform(test.profile_json, test.platform);
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

            let equals = if profile.get_platform().as_str() == "android" {
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
