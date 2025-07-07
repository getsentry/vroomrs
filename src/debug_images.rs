use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Features {
    pub has_debug_info: bool,
    pub has_sources: bool,
    pub has_symbols: bool,
    pub has_unwind_info: bool,
}

#[derive(Serialize, Clone, Default, Deserialize, Debug, PartialEq)]
pub struct Image {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Features>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_addr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_vmaddr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}
