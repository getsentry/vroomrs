use pyo3::pyclass;

use crate::{android::profile::AndroidProfile, sample::v1::SampleProfile, types::ProfileInterface};

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

    pub fn get_platform(&self) -> String {
        self.profile.get_platform().to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::{profile::Profile, types::Platform};

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
}
