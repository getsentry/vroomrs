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
                let sample: AndroidProfile = serde_json::from_slice(profile)?;
                Ok(Profile {
                    profile: Box::new(sample),
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
}
