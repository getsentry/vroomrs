use std::{cell::RefCell, hash::Hasher, rc::Rc};

use crate::frame::Frame;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    pub children: Vec<Rc<RefCell<Node>>>,

    pub duration_ns: u64,

    pub fingerprint: u64,

    pub is_application: bool,

    pub line: Option<u32>,

    pub name: String,

    pub package: String,

    pub path: Option<String>,

    pub end_ns: u64,

    pub frame: Frame,

    pub sample_count: u64,

    pub start_ns: u64,
}

impl Node {
    pub fn from_frame(f: &Frame, start: u64, end: u64, fingerprint: u64) -> Rc<RefCell<Node>> {
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

        Rc::new(RefCell::new(node))
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
