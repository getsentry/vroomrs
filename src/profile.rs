use pyo3::{pyclass, PyErr, PyResult};

use crate::{
    android::profile::AndroidProfile,
    sample::v1::SampleProfile,
    types::ProfileInterface,
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

    pub fn compress(&self) -> PyResult<Vec<u8>> {
        let prof = self
            .profile
            .to_json_vec()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        compress_lz4(&mut prof.as_slice())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    pub fn get_platform(&self) -> String {
        self.profile.get_platform().to_string()
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
