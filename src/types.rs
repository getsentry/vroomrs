use chrono::{DateTime, Utc};

use pyo3::exceptions::PyValueError;
use pyo3::{pyclass, PyErr};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::android::AndroidError;
use crate::debug_images::Image;
use crate::nodetree::Node;
use crate::sample::v1::Measurement;
use crate::sample::SampleError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChunkMeasurement {
    unit: MeasurementUnit,
    values: Vec<ChunkMeasurementValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementUnit {
    #[serde(alias = "ns")]
    Nanosecond,
    #[serde(alias = "hz")]
    Hertz,
    Byte,
    Percent,
    #[serde(alias = "nj")]
    Nanojoule,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChunkMeasurementValue {
    // UNIX timestamp in seconds as a float
    timestamp: f64,

    value: f64,
}

#[pyclass(eq, eq_int)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Android,
    Cocoa,
    Java,
    JavaScript,
    Node,
    Php,
    Python,
    Rust,
    #[default]
    None,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Android => write!(f, "android"),
            Platform::Cocoa => write!(f, "cocoa"),
            Platform::Java => write!(f, "java"),
            Platform::JavaScript => write!(f, "javascript"),
            Platform::Node => write!(f, "node"),
            Platform::Php => write!(f, "php"),
            Platform::Python => write!(f, "python"),
            Platform::Rust => write!(f, "rust"),
            Platform::None => write!(f, "none"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ClientSDK {
    pub name: String,
    pub version: String,
}

impl AsRef<ClientSDK> for ClientSDK {
    fn as_ref(&self) -> &ClientSDK {
        self
    }
}

impl std::ops::Deref for ClientSDK {
    type Target = ClientSDK;

    fn deref(&self) -> &Self::Target {
        self
    }
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct DebugMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<Image>>,
}

impl DebugMeta {
    pub fn is_empty(&self) -> bool {
        self.images.as_ref().is_none_or(|images| images.is_empty())
    }
}

#[derive(Debug)]
pub enum CallTreeError {
    Sample(SampleError),
    Android(AndroidError),
}

impl fmt::Display for CallTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallTreeError::Sample(sample_error) => match sample_error {
                SampleError::InvalidStackId => write!(f, "invalid stack id"),
                SampleError::InvalidFrameId => write!(f, "invalid frame id"),
            },
            CallTreeError::Android(android_error) => match android_error {
                AndroidError::FillSampleMetadataError(error) => write!(f, "{error}"),
            },
        }
    }
}

impl From<CallTreeError> for PyErr {
    fn from(error: CallTreeError) -> Self {
        PyValueError::new_err(error.to_string())
    }
}

pub type CallTreesU64 = HashMap<u64, Vec<Rc<RefCell<Node>>>>;
pub type CallTreesStr<'a> = HashMap<Cow<'a, str>, Vec<Rc<RefCell<Node>>>>;

pub trait ChunkInterface {
    fn get_environment(&self) -> Option<&str>;
    fn get_chunk_id(&self) -> &str;
    fn get_organization_id(&self) -> u64;
    fn get_platform(&self) -> Platform;
    fn get_profiler_id(&self) -> &str;
    fn get_project_id(&self) -> u64;
    fn get_received(&self) -> f64;
    fn get_release(&self) -> Option<&str>;
    fn get_retention_days(&self) -> i32;
    fn call_trees(&mut self, active_thread_id: Option<&str>)
        -> Result<CallTreesStr, CallTreeError>;

    fn duration_ms(&self) -> u64;
    fn end_timestamp(&self) -> f64;
    fn start_timestamp(&self) -> f64;
    fn sdk_name(&self) -> Option<&str>;
    fn sdk_version(&self) -> Option<&str>;

    fn storage_path(&self) -> String;

    fn normalize(&mut self);

    /// Serialize the given data structure as a JSON byte vector.
    fn to_json_vec(&self) -> Result<Vec<u8>, serde_json::Error>;

    fn as_any(&self) -> &dyn Any;
}

#[pyclass]
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Transaction {
    #[pyo3(get)]
    pub active_thread_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[pyo3(get)]
    pub duration_ns: Option<u64>,
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub trace_id: String,
    #[pyo3(get)]
    pub segment_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TransactionMetadata {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "app.identifier"
    )]
    pub app_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "http.method"
    )]
    pub http_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction: Option<String>,
    #[serde(rename = "transaction.end")]
    pub transaction_end: DateTime<Utc>,
    #[serde(rename = "transaction.start")]
    pub transaction_start: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transaction.op"
    )]
    pub transaction_op: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transaction.status"
    )]
    pub transaction_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<String>,
}

#[pyclass]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Metadata {
    #[pyo3(get)]
    pub android_api_level: Option<u32>,
    #[pyo3(get)]
    pub architecture: String,
    #[pyo3(get)]
    pub device_classification: Option<String>,
    #[pyo3(get)]
    pub device_locale: Option<String>,
    #[pyo3(get)]
    pub device_manufacturer: Option<String>,
    #[pyo3(get)]
    pub device_model: String,
    #[pyo3(get)]
    pub device_os_build_number: Option<String>,
    #[pyo3(get)]
    pub device_os_name: String,
    #[pyo3(get)]
    pub device_os_version: String,
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub project_id: String,
    #[pyo3(get)]
    pub sdk_name: Option<String>,
    #[pyo3(get)]
    pub sdk_version: Option<String>,
    #[pyo3(get)]
    pub timestamp: i64,
    #[pyo3(get)]
    pub trace_duration_ms: f64,
    #[pyo3(get)]
    pub transaction_id: String,
    #[pyo3(get)]
    pub transaction_name: String,
    #[pyo3(get)]
    pub version_code: Option<String>,
    #[pyo3(get)]
    pub version_name: Option<String>,
}

pub trait ProfileInterface {
    fn get_environment(&self) -> Option<&str>;
    fn get_profile_id(&self) -> &str;
    fn get_organization_id(&self) -> u64;
    fn get_platform(&self) -> Platform;
    fn get_project_id(&self) -> u64;
    fn get_received(&self) -> i64;
    fn get_release(&self) -> Option<&str>;
    fn get_retention_days(&self) -> i32;
    fn get_timestamp(&self) -> DateTime<Utc>;
    fn normalize(&mut self);
    fn call_trees(&mut self) -> Result<CallTreesU64, CallTreeError>;
    fn storage_path(&self) -> String;
    fn sdk_name(&self) -> Option<&str>;
    fn sdk_version(&self) -> Option<&str>;
    fn duration_ns(&self) -> u64;
    fn get_transaction(&self) -> Cow<Transaction>;
    fn get_transaction_tags(&self) -> &HashMap<String, String>;
    fn get_debug_meta(&self) -> &DebugMeta;
    fn get_measurements(&self) -> Option<&HashMap<String, Measurement>>;
    fn is_sampled(&self) -> bool;
    fn set_profile_id(&mut self, profile_id: String);
    fn get_metadata(&self) -> Metadata;

    /// Serialize the given data structure as a JSON byte vector.
    fn to_json_vec(&self) -> Result<Vec<u8>, serde_json::Error>;

    fn as_any(&self) -> &dyn Any;
}
