use serde::{Deserialize, Serialize};

use crate::platform;

#[derive(Serialize, Deserialize, Debug)]
pub struct Frame {
    #[serde(rename = "colno")]
    pub column: Option<u32>,

    pub data: Option<Data>,

    #[serde(rename = "filename")]
    pub file: Option<String>,

    #[serde(rename = "function")]
    pub function: Option<String>,

    #[serde(rename = "in_app")]
    pub in_app: Option<bool>,

    #[serde(rename = "instruction_addr")]
    pub instruction_addr: Option<String>,

    #[serde(rename = "lang")]
    pub lang: Option<String>,

    #[serde(rename = "lineno")]
    pub line: Option<u32>,

    pub module: Option<String>,

    pub package: Option<String>,

    #[serde(rename = "abs_path")]
    pub path: Option<String>,

    pub status: Option<String>,

    pub sym_addr: Option<String>,

    pub symbol: Option<String>,

    pub platform: Option<platform::Platform>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Data {
    #[serde(rename = "deobfuscation_status")]
    pub deobfuscation_status: Option<String>,

    #[serde(rename = "symbolicator_status")]
    pub symbolicator_status: Option<String>,

    #[serde(rename = "symbolicated")]
    pub js_symbolicated: Option<bool>,
}
