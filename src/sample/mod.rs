use serde::{Deserialize, Serialize};

pub mod v1;
pub mod v2;

#[derive(Debug)]
pub enum SampleError {
    InvalidStackId,
    InvalidFrameId,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct ThreadMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i32>,
}
