use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Features {
    pub has_debug_info: bool,
    pub has_sources: bool,
    pub has_symbols: bool,
    pub has_unwind_info: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Image {
    pub arch: Option<String>,
    pub code_file: Option<String>,
    pub debug_id: Option<String>,
    pub debug_status: Option<String>,
    pub features: Option<Features>,
    pub image_addr: Option<String>,
    pub image_size: Option<u64>,
    pub image_vmaddr: Option<String>,
    pub r#type: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct DebugMeta {
    pub images: Vec<Image>,
}

impl DebugMeta {
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}
