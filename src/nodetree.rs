use std::hash::Hasher;

use crate::frame::Frame;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Node {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,

    pub duration_ns: u64,

    pub fingerprint: u64,

    pub is_application: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,

    pub name: String,

    pub package: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(skip)]
    pub end_ns: u64,

    #[serde(skip)]
    pub frame: Frame,

    #[serde(skip)]
    pub sample_count: u64,

    #[serde(skip)]
    pub start_ns: u64,
}

impl Node {
    pub fn from_frame(f: &Frame, start: u64, end: u64, fingerprint: u64) -> Node {
        let is_application = f.in_app.unwrap_or(true);

        let mut node = Node {
            children: Vec::new(),
            duration_ns: 0,
            end_ns: end,
            fingerprint,
            frame: f.clone(),
            is_application,
            line: f.line,
            name: f.function.as_deref().unwrap_or_default().into(),
            package: f.module_or_package(),
            path: f.path.clone(),
            sample_count: 1,
            start_ns: start,
        };

        if end > 0 {
            node.duration_ns = node.end_ns - node.start_ns;
        }

        node
    }

    pub fn update(&mut self, timestamp: u64) {
        self.sample_count += 1;
        self.set_duration(timestamp);
    }

    pub fn to_frame(&self) -> Frame {
        let mut frame = self.frame.clone();
        if let Some(mut data) = frame.data {
            data.symbolicator_status = frame.status.clone();
            frame.data = Some(data);
        }
        frame
    }

    pub fn set_duration(&mut self, t: u64) {
        self.end_ns = t;
        self.duration_ns = self.end_ns - self.start_ns;
    }

    pub fn write_to_hash<H: Hasher>(&self, h: &mut H) {
        if self.package.is_empty() && self.name.is_empty() {
            h.write(b"-");
        } else {
            h.write(self.package.as_bytes());
            h.write(self.name.as_bytes());
        }
    }
}
