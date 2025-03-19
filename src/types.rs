use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::debug_images::Image;
use crate::nodetree::Node;
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
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

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct DebugMeta {
    pub images: Vec<Image>,
}

impl DebugMeta {
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

#[derive(Debug)]
pub enum CallTreeError {
    Sample(SampleError),
    Android,
}

impl fmt::Display for CallTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallTreeError::Sample(sample_error) => match sample_error {
                SampleError::InvalidStackId => write!(f, "invalid stack id"),
                SampleError::InvalidFrameId => write!(f, "invalid frame id"),
            },
            CallTreeError::Android => write!(f, "generic android call_tree error"),
        }
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
}
