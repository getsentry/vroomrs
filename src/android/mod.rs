mod chunk;

use serde::{Deserialize, Serialize};

use crate::types::Platform;

#[derive(Serialize, Deserialize, Debug)]
pub struct AndroidThread {
    pub id: u64,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AndroidMethod {
    pub class_name: String,
    pub data: Data,
    // method_id is not optional, but in our Vroom service,
    // the field was defined with the json tag `json:"id,omitempty"`
    // which means we (wrongly) skip the serialization of such
    // field if it's 0. By using a default value, we can safely deserialize
    // profiles that were stored previously through the vroom service.
    #[serde(default)]
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_frames: Option<Vec<AndroidMethod>>,
    pub name: String,
    pub signature: String,
    pub source_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_app: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Data {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deobfuscation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js_symbolicated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_in_app: Option<i8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Duration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nanos: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventMonotonic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<Duration>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monotonic: Option<EventMonotonic>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Enter,
    Exit,
    Unwind,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Clock {
    Global,
    Cpu,
    Wall,
    Dual,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AndroidEvent {
    pub action: Option<String>,
    pub thread_id: u64,
    // method_id is not optional, but in our Vroom service,
    // the field was defined with the json tag `json:"id,omitempty"`
    // which means we (wrongly) skip the serialization of such
    // field if it's 0. By using a default value, we can safely deserialize
    // profiles that were stored previously through the vroom service.
    #[serde(default)]
    pub method_id: u64,
    pub time: EventTime,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Android {
    pub clock: Clock,
    pub events: Vec<AndroidEvent>,
    pub methods: Vec<AndroidMethod>,
    pub start_time: u64,
    pub threads: Vec<AndroidThread>,
}
