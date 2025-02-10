use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientSDK {
    pub name: String,
    pub version: String,
}
