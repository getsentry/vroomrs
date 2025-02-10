use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientSDK {
    pub name: String,
    pub version: String,
}