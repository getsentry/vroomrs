use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
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
}
