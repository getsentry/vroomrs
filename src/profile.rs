use std::collections::HashMap;

use pyo3::{pyclass, pymethods, PyErr, PyResult};

use crate::{
    android::profile::AndroidProfile,
    nodetree::CallTreeFunction,
    occurrence::{self, Occurrence},
    sample::v1::SampleProfile,
    types::{CallTreeError, CallTreesU64, Metadata, ProfileInterface, Transaction},
    utils::{compress_lz4, decompress_lz4},
};

#[pyclass]
pub struct Profile {
    pub profile: Box<dyn ProfileInterface + Send + Sync>,
}

#[derive(serde::Deserialize)]
struct MinimumProfile {
    version: Option<String>,
}

impl Profile {
    pub(crate) fn from_json_vec(profile: &[u8]) -> Result<Self, serde_json::Error> {
        let min_prof: MinimumProfile = serde_json::from_slice(profile)?;
        match min_prof.version {
            None => {
                let android: AndroidProfile = serde_json::from_slice(profile)?;
                Ok(Profile {
                    profile: Box::new(android),
                })
            }
            Some(_) => {
                let sample: SampleProfile = serde_json::from_slice(profile)?;
                Ok(Profile {
                    profile: Box::new(sample),
                })
            }
        }
    }

    pub(crate) fn from_json_vec_and_platform(
        profile: &[u8],
        platform: &str,
    ) -> Result<Self, serde_json::Error> {
        match platform {
            "android" => {
                let android: AndroidProfile = serde_json::from_slice(profile)?;
                Ok(Profile {
                    profile: Box::new(android),
                })
            }
            _ => {
                let sample: SampleProfile = serde_json::from_slice(profile)?;
                Ok(Profile {
                    profile: Box::new(sample),
                })
            }
        }
    }

    pub(crate) fn decompress(source: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = decompress_lz4(source)?;
        Self::from_json_vec(bytes.as_ref())
            .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)
    }
}

#[pyclass]
pub struct Occurrences {
    #[pyo3(get)]
    pub occurrences: Vec<Occurrence>,
}

#[pymethods]
impl Occurrences {
    /// Serializes the occurrences to a JSON string.
    ///
    /// Returns:
    ///     str
    ///         A JSON string representation of the occurrences list.
    ///
    /// Raises:
    ///     ValueError
    ///         If the serialization fails due to invalid data.
    ///
    /// Example:
    ///     >>> occurrences = profile.find_occurrences()
    ///     >>> json_str = occurrences.to_json_str()
    ///     >>> print(json_str)
    pub fn to_json_str(&self) -> Result<String, PyErr> {
        serde_json::to_string(&self.occurrences)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    /// Filters occurrences to remove those with NONE_TYPE.
    ///
    /// This method removes all occurrences that have a type of NONE_TYPE,
    /// keeping only meaningful performance issues in the collection.
    ///
    /// Example:
    ///     >>> occurrences = profile.find_occurrences()
    ///     >>> occurrences.filter_none_type_issues()
    pub fn filter_none_type_issues(&mut self) {
        self.occurrences
            .retain(|occ| occ.r#type != occurrence::NONE_TYPE);
    }
}

#[pymethods]
impl Profile {
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
    ///         The profile's platform. One of the following values:
    ///             * android
    ///             * cocoa
    ///             * java
    ///             * javascript
    ///             * php
    ///             * node
    ///             * python
    ///             * rust
    ///             * none
    pub fn get_platform(&self) -> String {
        self.profile.get_platform().to_string()
    }

    /// Returns the profil ID.
    ///
    /// Returns:
    ///     str
    ///         The profile ID of the profile.
    pub fn get_profile_id(&self) -> &str {
        self.profile.get_profile_id()
    }

    /// Returns the project ID.
    ///
    /// Returns:
    ///     int
    ///         The project ID to which the profile belongs.
    pub fn get_project_id(&self) -> u64 {
        self.profile.get_project_id()
    }

    /// Returns the received timestamp.
    ///
    /// Returns:
    ///     int
    fn get_received(&self) -> i64 {
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

    /// Returns the duration of the profile in ns.
    ///
    /// Returns:
    ///     int
    ///         The duration of the profile in ns.
    pub fn duration_ns(&self) -> u64 {
        self.profile.duration_ns()
    }

    /// Returns the end timestamp of the profile.
    ///
    /// The timestamp is a Unix timestamp in seconds
    /// with millisecond precision.
    ///
    /// Returns:
    ///     float
    ///         The timestamp of the profile.
    pub fn get_timestamp(&self) -> f64 {
        self.profile.get_timestamp().timestamp_micros() as f64 / 1_000_000.0
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

    /// Extracts function metrics from the profile.
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
    ///
    /// Returns:
    ///     list[:class:`CallTreeFunction`]
    ///         A list of :class:`CallTreeFunction` objects, each containing metrics for a function in the call tree.
    ///
    /// Raises:
    ///     pyo3.exceptions.PyException: If an error occurs during the extraction process.
    ///
    /// Example:
    ///     >>> metrics = profile.extract_functions_metrics(min_depth=2, filter_system_frames=True, max_unique_functions=10)
    ///     >>> for function_metric in metrics:
    ///     ...     do_something(function_metric)
    #[pyo3(signature = (min_depth, filter_system_frames, max_unique_functions=None))]
    pub fn extract_functions_metrics(
        &mut self,
        min_depth: u16,
        filter_system_frames: bool,
        max_unique_functions: Option<usize>,
    ) -> PyResult<Vec<CallTreeFunction>> {
        let call_trees: CallTreesU64 = self.profile.call_trees()?;
        let mut functions: HashMap<u32, CallTreeFunction> = HashMap::new();

        for (tid, call_trees_for_thread) in &call_trees {
            for call_tree in call_trees_for_thread {
                call_tree.borrow_mut().collect_functions(
                    &mut functions,
                    tid.to_string().as_ref(),
                    0,
                    min_depth,
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
        functions_list.sort_by(|a, b| b.sum_self_time_ns.cmp(&a.sum_self_time_ns));

        functions_list.truncate(max_unique_functions.unwrap_or(functions_list.len()));
        Ok(functions_list)
    }

    /// Finds performance issues (occurrences) in the profile.
    ///
    /// This method analyzes the call tree to detect various performance issues such as:
    /// - Frame drops caused by main thread blocking
    /// - Slow operations on the main thread (e.g., I/O, compression, database operations)
    /// - SwiftUI performance issues (view inflation, layout, rendering)
    /// - Machine learning model operations
    /// - And other platform-specific performance patterns
    ///
    /// Returns:
    ///     :class:`Occurrence`
    ///         An :class:`Occurrences` object, a wrapper containing a list of :class:`Occurrences`, each representing a detected performance issue.
    ///
    /// Raises:
    ///     pyo3.exceptions.PyException: If an error occurs during the detection process.
    pub fn find_occurrences(&mut self) -> Result<Occurrences, CallTreeError> {
        let call_trees = self.profile.call_trees()?;
        Ok(Occurrences {
            occurrences: occurrence::find_occurences(self.profile.as_ref(), &call_trees),
        })
    }

    /// Returns whether the profile is sampled.
    ///
    /// Returns:
    ///     bool
    ///         True if the profile is sampled, False otherwise.
    pub fn is_sampled(&self) -> bool {
        self.profile.is_sampled()
    }

    /// Sets the profile ID.
    ///
    /// This method updates the profile's unique identifier.
    ///
    /// Args:
    ///     profile_id (str): The new profile ID to set.
    ///
    /// Example:
    ///     >>> profile.set_profile_id("06ccc59502e64154a352e25cb59ccf08")
    pub fn set_profile_id(&mut self, profile_id: String) {
        self.profile.set_profile_id(profile_id);
    }

    /// Returns the transaction information associated with the profile.
    ///
    /// Returns:
    ///     Transaction
    ///         The transaction data including ID, name, trace ID, segment ID,
    ///         active thread ID, and optional duration in nanoseconds.
    pub fn get_transaction(&self) -> Transaction {
        self.profile.get_transaction().into_owned()
    }

    /// Returns metadata information associated with the profile.
    ///
    /// This method extracts comprehensive metadata about the profile including
    /// device information, SDK details, transaction data, and system specifications.
    ///
    /// Returns:
    ///     Metadata
    ///         A metadata object containing device characteristics, SDK information,
    ///         transaction details, and other profile-specific data.
    pub fn get_metadata(&self) -> Metadata {
        self.profile.get_metadata()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        android::profile::AndroidProfile, profile::Profile, sample::v1::SampleProfile,
        types::Platform,
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
                profile_json: include_bytes!("../tests/fixtures/sample/v1/valid_cocoa.json"),
                want: Platform::Cocoa.to_string(),
            },
            TestStruct {
                name: "python profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/sample/v1/valid_python.json"),
                want: Platform::Python.to_string(),
            },
            TestStruct {
                name: "android profile".to_string(),
                profile_json: include_bytes!("../tests/fixtures/android/profile/valid.json"),
                want: Platform::Android.to_string(),
            },
        ];

        for test in test_cases {
            let prof = Profile::from_json_vec(test.profile_json);
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
                profile_json: include_bytes!("../tests/fixtures/sample/v1/valid_cocoa.json"),
                want: Platform::Cocoa.to_string(),
            },
            TestStruct {
                name: "python profile".to_string(),
                platform: "python",
                profile_json: include_bytes!("../tests/fixtures/sample/v1/valid_python.json"),
                want: Platform::Python.to_string(),
            },
            TestStruct {
                name: "android profile".to_string(),
                platform: "android",
                profile_json: include_bytes!("../tests/fixtures/android/profile/valid.json"),
                want: Platform::Android.to_string(),
            },
        ];

        for test in test_cases {
            let prof = Profile::from_json_vec_and_platform(test.profile_json, test.platform);
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
                name: "compressing and decompressing cocoa (V1)".to_string(),
                payload: include_bytes!("../tests/fixtures/sample/v1/valid_cocoa.json"),
            },
            TestStruct {
                name: "compressing and decompressing python (V1)".to_string(),
                payload: include_bytes!("../tests/fixtures/sample/v1/valid_python.json"),
            },
            TestStruct {
                name: "compressing and decompressing android profile".to_string(),
                payload: include_bytes!("../tests/fixtures/android/profile/valid.json"),
            },
        ];

        for test in test_cases {
            let profile = Profile::from_json_vec(test.payload).unwrap();

            let compressed_profile_bytes = profile.compress().unwrap();
            let decompressed_profile =
                Profile::decompress(compressed_profile_bytes.as_slice()).unwrap();

            let equals = if profile.get_platform() == Platform::Android.to_string() {
                let original_sample = profile
                    .profile
                    .as_any()
                    .downcast_ref::<AndroidProfile>()
                    .unwrap();
                let final_sample = decompressed_profile
                    .profile
                    .as_any()
                    .downcast_ref::<AndroidProfile>()
                    .unwrap();
                original_sample == final_sample
            } else {
                let original_sample = profile
                    .profile
                    .as_any()
                    .downcast_ref::<SampleProfile>()
                    .unwrap();
                let final_sample = decompressed_profile
                    .profile
                    .as_any()
                    .downcast_ref::<SampleProfile>()
                    .unwrap();
                original_sample == final_sample
            };

            assert!(equals, "test `{}` failed", test.name);
        }
    }
}
