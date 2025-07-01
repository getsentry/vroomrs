pub mod chunk;
pub mod profile;

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::Path;
use std::rc::Rc;

use fnv_rs::Fnv64;
use serde::{Deserialize, Serialize};

use crate::frame::{self, Frame};
use crate::nodetree;
use crate::types::{CallTreeError, CallTreesU64};
use crate::{nodetree::Node, types::Platform, MAX_STACK_DEPTH};

const MAIN_THREAD: &str = "main";
const ANDROID_PACKAGE_PREFIXES: [&str; 11] = [
    "android.",
    "androidx.",
    "com.android.",
    "com.google.android.",
    "com.motorola.",
    "java.",
    "javax.",
    "kotlin.",
    "kotlinx.",
    "retrofit2.",
    "sun.",
];

#[derive(Debug)]
pub enum AndroidError {
    FillSampleMetadataError(Box<dyn std::error::Error>),
}
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct AndroidThread {
    id: u64,
    name: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone)]
struct AndroidMethod {
    class_name: String,
    data: Option<Data>,
    // method_id is not optional, but in our Vroom service,
    // the field was defined with the json tag `json:"id,omitempty"`
    // which means we (wrongly) skip the serialization of such
    // field if it's 0. By using a default value, we can safely deserialize
    // profiles that were stored previously through the vroom service.
    #[serde(default)]
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_frames: Option<Vec<AndroidMethod>>,
    name: String,
    signature: String,
    source_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_col: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    in_app: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<Platform>,
}

impl AndroidMethod {
    fn full_method_name_from_android_method(&self) -> String {
        // when we we're dealing with js frame that were "converted"
        // to android methods (react-native) we don't have class name
        if self.class_name.is_empty() {
            return self.name.clone();
        }

        let mut result = String::with_capacity(
            self.class_name.len() + 1 + self.name.len() + self.signature.len(),
        );
        result.push_str(&self.class_name);

        // "<init>" refers to the constructor in which case it's more readable to omit the method name. Note the method name
        // can also be a static initializer "<clinit>" but I don't know of any better ways to represent it so leaving as is.
        if self.name != "<init>" {
            result.push('.');
            result.push_str(&self.name);
        }

        result.push_str(&self.signature);

        result
    }

    fn package_name(&self) -> &str {
        match self.class_name.rfind('.') {
            Some(index) => &self.class_name[..index],
            None => self.class_name.as_ref(),
        }
    }

    fn extract_package_name_and_simple_method_name(&self) -> (&str, String) {
        let full_method_name = self.full_method_name_from_android_method();
        let package_name = self.package_name();

        let simple_method_name =
            strip_package_name_from_full_method_name(&full_method_name, package_name);

        (package_name, simple_method_name)
    }

    fn frame(&self) -> Frame {
        let (package, _) = self.extract_package_name_and_simple_method_name();
        let method_name = self.full_method_name_from_android_method();
        let in_app: Option<bool> = self
            .in_app
            .or_else(|| Some(is_android_application_package(&self.class_name)));

        Frame {
            data: self.data.as_ref().map(|data| frame::Data {
                deobfuscation_status: data.deobfuscation_status.clone(),
                js_symbolicated: data.js_symbolicated,
                ..Default::default()
            }),
            file: Path::new(&self.source_file)
                .file_name()
                .map(|file_name| file_name.to_string_lossy().to_string()),
            function: Some(method_name),
            in_app,
            line: self.source_line,
            method_id: Some(self.id),
            column: self.source_col,
            package: Some(package.to_owned()),
            path: if self.source_file.is_empty() {
                None
            } else {
                Some(self.source_file.to_owned())
            },
            platform: self.platform,
            ..Default::default()
        }
    }
}

pub(crate) fn strip_package_name_from_full_method_name(full_name: &str, package: &str) -> String {
    let prefix = format!("{package}.");
    full_name
        .strip_prefix(&prefix)
        .unwrap_or(full_name)
        .to_string()
}

/// is_android_application_package checks if a symbol belongs to an Android system package.
fn is_android_application_package(package_name: &str) -> bool {
    for prefix in &ANDROID_PACKAGE_PREFIXES {
        if package_name.starts_with(prefix) {
            return false;
        }
    }
    true
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone)]
struct Data {
    #[serde(skip_serializing_if = "Option::is_none")]
    deobfuscation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    js_symbolicated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    orig_in_app: Option<i8>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
struct Duration {
    #[serde(skip_serializing_if = "Option::is_none")]
    secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nanos: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
struct EventMonotonic {
    #[serde(skip_serializing_if = "Option::is_none")]
    wall: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu: Option<Duration>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
struct EventTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    global: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "Monotonic")]
    monotonic: Option<EventMonotonic>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
enum Action {
    Enter,
    Exit,
    Unwind,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
enum Clock {
    Global,
    Cpu,
    Wall,
    Dual,
    #[default]
    None,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
struct AndroidEvent {
    action: Action,
    thread_id: u64,
    // method_id is not optional, but in our Vroom service,
    // the field was defined with the json tag `json:"id,omitempty"`
    // which means we (wrongly) skip the serialization of such
    // field if it's 0. By using a default value, we can safely deserialize
    // profiles that were stored previously through the vroom service.
    #[serde(default)]
    method_id: u64,
    time: EventTime,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
struct Android {
    clock: Clock,
    events: Vec<AndroidEvent>,
    methods: Vec<AndroidMethod>,
    start_time: u64,
    threads: Vec<AndroidThread>,
    // sdk_start_time, if set (manually), it's an absolute ts in Ns
    // whose value comes from the chunk timestamp set by the sentry SDK.
    // This is used to control the ts during callTree generation.
    #[serde(skip_serializing)]
    sdk_start_time: Option<u64>,
}

impl Android {
    /// Returns the thread ID of the main thread, or 0 if not found
    fn active_thread_id(&self) -> u64 {
        self.threads
            .iter()
            .find(|thread| thread.name == MAIN_THREAD)
            .map_or(0, |thread| thread.id)
    }

    /// Wall-clock time is supposed to be monotonic
    /// in a few rare cases we've noticed this was not the case.
    /// Due to some overflow happening client-side in the embedded
    /// profiler, the sequence might be decreasing at certain points.
    ///
    /// This is just a workaround to mitigate this issue, should it
    /// happen.
    fn fix_samples_time(&mut self) {
        if matches!(self.clock, Clock::Global | Clock::Cpu) {
            return;
        }

        let mut thread_max_time_ns: std::collections::HashMap<u64, u64> =
            std::collections::HashMap::new();
        let mut thread_latest_sample_time_ns: std::collections::HashMap<u64, u64> =
            std::collections::HashMap::new();
        let mut regression_index: Option<usize> = None;

        for (i, event) in self.events.iter().enumerate() {
            if let (Some(secs), Some(nanos)) = (
                event
                    .time
                    .monotonic
                    .as_ref()
                    .and_then(|m| m.wall.as_ref().and_then(|w| w.secs)),
                event
                    .time
                    .monotonic
                    .as_ref()
                    .and_then(|m| m.wall.as_ref().and_then(|w| w.nanos)),
            ) {
                let current = (secs * 1_000_000_000) + nanos;

                if let Some(latest) = thread_latest_sample_time_ns.get(&event.thread_id) {
                    if current < *latest {
                        regression_index = Some(i);
                        break;
                    }
                }

                thread_latest_sample_time_ns.insert(event.thread_id, current);
                thread_max_time_ns
                    .entry(event.thread_id)
                    .and_modify(|max| *max = std::cmp::max(*max, current))
                    .or_insert(current);
            }
        }

        if let Some(regression_idx) = regression_index {
            for i in regression_idx..self.events.len() {
                let event = &self.events[i];

                if let (Some(secs), Some(nanos)) = (
                    event
                        .time
                        .monotonic
                        .as_ref()
                        .and_then(|m| m.wall.as_ref().and_then(|w| w.secs)),
                    event
                        .time
                        .monotonic
                        .as_ref()
                        .and_then(|m| m.wall.as_ref().and_then(|w| w.nanos)),
                ) {
                    let current = (secs * 1_000_000_000) + nanos;
                    let thread_id = event.thread_id;

                    let max_time = *thread_max_time_ns.get(&thread_id).unwrap_or(&0);
                    let latest_time = *thread_latest_sample_time_ns.get(&thread_id).unwrap_or(&0);

                    let new_time = get_adjusted_time(max_time, latest_time, current);

                    thread_max_time_ns
                        .entry(thread_id)
                        .and_modify(|max| *max = std::cmp::max(*max, new_time))
                        .or_insert(new_time);

                    thread_latest_sample_time_ns.insert(thread_id, current);

                    // Update the event time
                    if let Some(monotonic) = &mut self.events[i].time.monotonic {
                        if let Some(wall) = &mut monotonic.wall {
                            wall.secs = Some(new_time / 1_000_000_000);
                            wall.nanos = Some(new_time % 1_000_000_000);
                        }
                    }
                }
            }
        } // end fix_samples_time
    }

    fn timestamp_getter(&self) -> Box<dyn Fn(&EventTime) -> u64 + '_> {
        match self.clock {
            Clock::Global => Box::new(|t: &EventTime| {
                let secs: u64 = if let Some(global) = &t.global {
                    global.secs.unwrap_or_default()
                } else {
                    0
                };
                let nanos: u64 = if let Some(global) = &t.global {
                    global.nanos.unwrap_or_default()
                } else {
                    0
                };

                secs * 1_000_000_000 + nanos - self.start_time
                // let nanos: u64 = t.global.unwrap_or(|| -> 0).nanos.unwrap_or_default();
                // secs
            }),
            Clock::Cpu => Box::new(|t: &EventTime| {
                if let Some(monotonic) = &t.monotonic {
                    let secs = if let Some(cpu) = &monotonic.cpu {
                        cpu.secs.unwrap_or_default()
                    } else {
                        0
                    };
                    let nanos = if let Some(cpu) = &monotonic.cpu {
                        cpu.nanos.unwrap_or_default()
                    } else {
                        0
                    };
                    return secs * 1_000_000_000 + nanos;
                }
                0
            }),
            _ => Box::new(|t: &EventTime| {
                if let Some(monotonic) = &t.monotonic {
                    let secs = if let Some(wall) = &monotonic.wall {
                        wall.secs.unwrap_or_default()
                    } else {
                        0
                    };
                    let nanos = if let Some(wall) = &monotonic.wall {
                        wall.nanos.unwrap_or_default()
                    } else {
                        0
                    };
                    return secs * 1_000_000_000 + nanos;
                }
                0
            }),
        }
    }

    fn call_trees(&mut self) -> Result<CallTreesU64, CallTreeError> {
        self.call_trees_with_max_depth(MAX_STACK_DEPTH)
    }

    fn call_trees_with_max_depth(&mut self, max_depth: u64) -> Result<CallTreesU64, CallTreeError> {
        // in case wall-clock.secs is not monotonic, "fix" it
        self.fix_samples_time();

        let active_thread_id = self.active_thread_id();

        let build_timestamp = self.timestamp_getter();
        let mut trees_by_thread_id: HashMap<u64, Vec<Rc<RefCell<Node>>>> = HashMap::new();
        let mut stacks: HashMap<u64, Vec<Rc<RefCell<Node>>>> = HashMap::new();
        let mut stack_depth: HashMap<u64, i64> = HashMap::new();

        let mut methods: HashMap<u64, Cow<AndroidMethod>> = HashMap::new();
        for method in &self.methods {
            methods.insert(method.id, Cow::Borrowed(method));
        }

        let close_frame =
            |thread_id: u64, ts: u64, i: i64, stacks: &HashMap<u64, Vec<Rc<RefCell<Node>>>>| {
                let mut n = stacks
                    .get(&thread_id)
                    .expect("close_frame: no stack found for given thread")
                    .get(i as usize)
                    .expect("close_frame: no node found in current stack")
                    .borrow_mut();
                n.update(ts);
                n.sample_count = (n.duration_ns as f64 / (10 * 1_000_000) as f64).ceil() as u64;
            };

        let mut max_timestamp_ns: u64 = 0;
        let mut enter_per_method: HashMap<u64, i64> = HashMap::new();
        let mut exit_per_method: HashMap<u64, i64> = HashMap::new();

        for event in &self.events {
            if event.thread_id != active_thread_id {
                continue;
            }
            let ts = build_timestamp(&event.time) + self.sdk_start_time.unwrap_or_default();
            if ts > max_timestamp_ns {
                max_timestamp_ns = ts;
            }
            match event.action {
                Action::Enter => {
                    let method = methods.entry(event.method_id).or_insert_with(|| {
                        Cow::Owned(AndroidMethod {
                            class_name: "unknown".to_string(),
                            id: event.method_id,
                            name: "unknown".to_string(),
                            ..Default::default()
                        })
                    });
                    let depth = stack_depth.entry(event.thread_id).or_default();
                    *depth += 1;
                    if *depth > max_depth as i64 {
                        continue;
                    }
                    *enter_per_method.entry(event.method_id).or_default() += 1;
                    let n = nodetree::Node::from_frame(&method.frame(), ts, 0, 0);
                    if stacks.entry(event.thread_id).or_default().is_empty() {
                        trees_by_thread_id
                            .entry(event.thread_id)
                            .or_default()
                            .push(Rc::clone(&n));
                    } else {
                        let stack = stacks.get(&event.thread_id).unwrap();
                        let i = stack.len() - 1;
                        if let Some(node) = stack.get(i) {
                            node.borrow_mut().children.push(Rc::clone(&n));
                        }
                    }
                    let stack = stacks.entry(event.thread_id).or_default();
                    stack.push(Rc::clone(&n));
                    n.borrow_mut().fingerprint = generate_fingerprint(stack);
                } //end Action::Enter
                Action::Exit | Action::Unwind => {
                    let depth = stack_depth.entry(event.thread_id).or_default();
                    *depth -= 1;
                    if *depth >= max_depth as i64 {
                        continue;
                    }
                    if stacks
                        .get(&event.thread_id)
                        .is_none_or(|stack| stack.is_empty())
                    {
                        continue;
                    }
                    let mut i = (stacks.get(&event.thread_id).unwrap().len() as i64) - 1;
                    let mut event_skipped = false;
                    while i >= 0 {
                        let node_method_id = stacks.get(&event.thread_id).unwrap()[i as usize]
                            .borrow()
                            .frame
                            .method_id;
                        if let (Some(method_id), Some(method_enters), Some(method_exits)) = (
                            node_method_id,
                            enter_per_method.get(&event.method_id),
                            exit_per_method.get(&event.method_id),
                        ) {
                            if method_id != event.method_id && method_enters < method_exits {
                                event_skipped = true;
                                break;
                            }
                        }
                        close_frame(event.thread_id, ts, i, &stacks);
                        exit_per_method
                            .entry(event.method_id)
                            .and_modify(|c| *c += 1);
                        if node_method_id.is_some_and(|id| id == event.method_id) {
                            break;
                        }
                        i -= 1;
                    }
                    // If we didn't skip the event, we should cut the stack accordingly.
                    if !event_skipped {
                        stacks
                            .entry(event.thread_id)
                            .and_modify(|stack| stack.truncate(i as usize));
                    }
                } //end Action Exit | Unwind
            } //end match event action
        } //end events loop
          // Close remaining open frames.
        for (thread_id, stack) in stacks.iter() {
            let mut i = (stack.len() as i64) - 1;
            while i >= 0 {
                close_frame(*thread_id, max_timestamp_ns, i, &stacks);
                i -= 1;
            }
        }
        for (_, trees) in trees_by_thread_id.iter() {
            for root in trees {
                root.borrow_mut().close(max_timestamp_ns);
            }
        }

        Ok(trees_by_thread_id)
    }
}

// maxTimeNs: the highest time (in nanoseconds) in the sequence so far
// latestNs: the latest time value in ns (at time t-1) before it was updated
// currentNs: current value in ns (at time t) before it's updated.
fn get_adjusted_time(max_time_ns: u64, latest_ns: u64, current_ns: u64) -> u64 {
    if current_ns < max_time_ns && current_ns < latest_ns {
        max_time_ns + 1_000_000_000
    } else {
        max_time_ns + (current_ns - latest_ns)
    }
}

fn generate_fingerprint(stack: &Vec<Rc<RefCell<Node>>>) -> u64 {
    let mut hasher = Fnv64::default();
    for node in stack {
        node.borrow().write_to_hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::AndroidMethod;
    use crate::{
        android::{
            Action, Android, AndroidEvent, AndroidThread, Clock, Duration, EventMonotonic,
            EventTime,
        },
        frame::Frame,
        nodetree::Node,
        types::CallTreesU64,
    };

    use pretty_assertions::assert_eq;

    fn get_missing_exit_events_trace() -> Android {
        Android {
            clock: Clock::Dual,
            events: vec![
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 1,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 2,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 1,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 1,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(3000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
            ],
            methods: vec![
                AndroidMethod {
                    class_name: "class1".to_string(),
                    id: 1,
                    name: "method1".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class2".to_string(),
                    id: 2,
                    name: "method2".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
            ],
            start_time: 398635355383000,
            threads: vec![AndroidThread {
                id: 1,
                name: "main".to_string(),
            }],
            ..Default::default()
        }
    }

    fn get_missing_enter_events_trace() -> Android {
        Android {
            clock: Clock::Dual,
            events: vec![
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 1,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 3,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1500),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 4,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1750),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 2,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 4,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2250),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 3,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2500),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 1,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(3000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
            ],
            methods: vec![
                AndroidMethod {
                    class_name: "class1".to_string(),
                    id: 1,
                    name: "method1".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class2".to_string(),
                    id: 2,
                    name: "method2".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class3".to_string(),
                    id: 3,
                    name: "method3".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class4".to_string(),
                    id: 4,
                    name: "method4".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
            ],
            start_time: 398635355383000,
            threads: vec![AndroidThread {
                id: 1,
                name: "main".to_string(),
            }],
            ..Default::default()
        }
    }

    fn get_stack_depth_3_events_trace() -> Android {
        Android {
            clock: Clock::Dual,
            events: vec![
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 1,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 3,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 4,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(1000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 2,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Exit,
                    thread_id: 1,
                    method_id: 4,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
                AndroidEvent {
                    action: Action::Enter,
                    thread_id: 1,
                    method_id: 3,
                    time: EventTime {
                        monotonic: Some(EventMonotonic {
                            wall: Some(Duration {
                                nanos: Some(2000),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
            ],
            methods: vec![
                AndroidMethod {
                    class_name: "class1".to_string(),
                    id: 1,
                    name: "method1".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class2".to_string(),
                    id: 2,
                    name: "method2".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class3".to_string(),
                    id: 3,
                    name: "method3".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
                AndroidMethod {
                    class_name: "class4".to_string(),
                    id: 4,
                    name: "method4".to_string(),
                    signature: "()".to_string(),
                    ..Default::default()
                },
            ],
            start_time: 398635355383000,
            threads: vec![AndroidThread {
                id: 1,
                name: "main".to_string(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_fix_samples_time() {
        struct TestStruct<'a> {
            name: String,
            trace: &'a mut Android,
            want: Android,
        }

        let test_cases = [TestStruct {
            name: "Make sample secs monotonic".to_string(),
            trace: &mut Android {
                clock: Clock::Dual,
                events: vec![
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 1,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(1),
                                    nanos: Some(1000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 1,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(2),
                                    nanos: Some(1000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 1,
                        method_id: 3,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(7),
                                    nanos: Some(2000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 1,
                        method_id: 3,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(6),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 1,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(6),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 1,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(9),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 2,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(1),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 2,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(2),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 2,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(2),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 2,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(3),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                ],
                start_time: 398635355383000,
                threads: vec![
                    AndroidThread {
                        id: 1,
                        name: "main".to_string(),
                    },
                    AndroidThread {
                        id: 2,
                        name: "background".to_string(),
                    },
                ],
                ..Default::default()
            },
            want: Android {
                clock: Clock::Dual,
                events: vec![
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 1,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(1),
                                    nanos: Some(1000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 1,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(2),
                                    nanos: Some(1000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 1,
                        method_id: 3,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(7),
                                    nanos: Some(2000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 1,
                        method_id: 3,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(8),
                                    nanos: Some(2000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 1,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(8),
                                    nanos: Some(2000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 1,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(11),
                                    nanos: Some(2000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 2,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(1),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Enter,
                        thread_id: 2,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(2),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 2,
                        method_id: 2,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(2),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                    AndroidEvent {
                        action: Action::Exit,
                        thread_id: 2,
                        method_id: 1,
                        time: EventTime {
                            monotonic: Some(EventMonotonic {
                                wall: Some(Duration {
                                    secs: Some(3),
                                    nanos: Some(3000),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    }, // AndroidEvent
                ],
                start_time: 398635355383000,
                threads: vec![
                    AndroidThread {
                        id: 1,
                        name: "main".to_string(),
                    },
                    AndroidThread {
                        id: 2,
                        name: "background".to_string(),
                    },
                ],
                ..Default::default()
            },
        }]; // end test_cases
        for test_case in test_cases {
            test_case.trace.fix_samples_time();
            assert_eq!(
                *test_case.trace, test_case.want,
                "{} test failed.",
                test_case.name
            )
        }
    }

    #[test]
    fn test_timestamp_getter() {
        struct TestStruct {
            name: String,
            trace: Android,
            event: EventTime,
            want: u64,
        }

        let test_cases = [
            TestStruct {
                name: "global clock".to_string(),
                trace: Android {
                    clock: Clock::Global,
                    start_time: 500,
                    ..Default::default()
                },
                event: EventTime {
                    global: Some(Duration {
                        secs: Some(2),
                        nanos: Some(1000),
                    }),
                    ..Default::default()
                },
                want: 2_000_000_500,
            }, // end test case
            TestStruct {
                name: "cpu clock".to_string(),
                trace: Android {
                    clock: Clock::Cpu,
                    start_time: 0,
                    ..Default::default()
                },
                event: EventTime {
                    monotonic: Some(EventMonotonic {
                        cpu: Some(Duration {
                            secs: Some(1),
                            nanos: Some(1000),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                want: 1_000_001_000,
            }, // end test case
            TestStruct {
                name: "wall clock".to_string(),
                trace: Android {
                    clock: Clock::Wall,
                    start_time: 0,
                    ..Default::default()
                },
                event: EventTime {
                    monotonic: Some(EventMonotonic {
                        wall: Some(Duration {
                            secs: Some(3),
                            nanos: Some(400),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                want: 3_000_000_400,
            }, // end test case
        ];
        for test_case in test_cases {
            let build_timestamp = test_case.trace.timestamp_getter();
            let timestamp = build_timestamp(&test_case.event);
            assert_eq!(timestamp, test_case.want, "{} failed", test_case.name)
        }
    }

    #[test]
    fn test_call_trees() {
        use crate::MAX_STACK_DEPTH;
        struct TestStruct {
            name: String,
            trace: Android,
            want: CallTreesU64,
            max_depth: u64,
        }

        let mut test_cases = [
            TestStruct {
                name: "Build call trees with missing exit events".to_string(),
                trace: get_missing_exit_events_trace(),
                max_depth: MAX_STACK_DEPTH,
                want: [(
                    1,
                    vec![
                        Rc::new(RefCell::new(Node {
                            duration_ns: 1000,
                            is_application: true,
                            end_ns: 2000,
                            start_ns: 1000,
                            name: "class1.method1()".to_string(),
                            package: "class1".to_string(),
                            sample_count: 1,
                            fingerprint: 8189722245693347360,
                            frame: Frame {
                                function: Some("class1.method1()".to_string()),
                                in_app: Some(true),
                                method_id: Some(1),
                                package: Some("class1".to_string()),
                                ..Default::default()
                            },
                            children: vec![Rc::new(RefCell::new(Node {
                                duration_ns: 1000,
                                is_application: true,
                                end_ns: 2000,
                                start_ns: 1000,
                                name: "class2.method2()".to_string(),
                                package: "class2".to_string(),
                                sample_count: 1,
                                fingerprint: 13109094123195830328,
                                frame: Frame {
                                    function: Some("class2.method2()".to_string()),
                                    in_app: Some(true),
                                    method_id: Some(2),
                                    package: Some("class2".to_string()),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }))],
                            ..Default::default()
                        })),
                        Rc::new(RefCell::new(Node {
                            duration_ns: 0,
                            is_application: true,
                            end_ns: 3000,
                            start_ns: 3000,
                            name: "class1.method1()".to_string(),
                            package: "class1".to_string(),
                            fingerprint: 8189722245693347360,
                            frame: Frame {
                                function: Some("class1.method1()".to_string()),
                                in_app: Some(true),
                                method_id: Some(1),
                                package: Some("class1".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                    ], //end trees of thread 1
                )]
                .iter()
                .cloned()
                .collect(),
            },
            TestStruct {
                name: "Build call trees with missing enter events".to_string(),
                trace: get_missing_enter_events_trace(),
                max_depth: MAX_STACK_DEPTH,
                want: [(
                    1,
                    vec![Rc::new(RefCell::new(Node {
                        duration_ns: 2000,
                        is_application: true,
                        start_ns: 1000,
                        end_ns: 3000,
                        sample_count: 1,
                        package: "class1".to_string(),
                        name: "class1.method1()".to_string(),
                        fingerprint: 8189722245693347360,
                        frame: Frame {
                            function: Some("class1.method1()".to_string()),
                            in_app: Some(true),
                            method_id: Some(1),
                            package: Some("class1".to_string()),
                            ..Default::default()
                        },
                        children: vec![Rc::new(RefCell::new(Node {
                            duration_ns: 1000,
                            is_application: true,
                            start_ns: 1500,
                            end_ns: 2500,
                            sample_count: 1,
                            package: "class3".to_string(),
                            name: "class3.method3()".to_string(),
                            fingerprint: 12998937618057698167,
                            frame: Frame {
                                function: Some("class3.method3()".to_string()),
                                in_app: Some(true),
                                method_id: Some(3),
                                package: Some("class3".to_string()),
                                ..Default::default()
                            },
                            children: vec![Rc::new(RefCell::new(Node {
                                duration_ns: 500,
                                is_application: true,
                                start_ns: 1750,
                                end_ns: 2250,
                                sample_count: 1,
                                package: "class4".to_string(),
                                name: "class4.method4()".to_string(),
                                fingerprint: 10444418669734640285,
                                frame: Frame {
                                    function: Some("class4.method4()".to_string()),
                                    in_app: Some(true),
                                    method_id: Some(4),
                                    package: Some("class4".to_string()),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }))],
                            ..Default::default()
                        }))],
                        ..Default::default()
                    }))],
                )]
                .iter()
                .cloned()
                .collect(),
            },
            TestStruct {
                name: "".to_string(),
                trace: get_stack_depth_3_events_trace(),
                max_depth: 1,
                want: [(
                    1,
                    vec![Rc::new(RefCell::new(Node {
                        duration_ns: 1000,
                        is_application: true,
                        start_ns: 1000,
                        end_ns: 2000,
                        sample_count: 1,
                        package: "class1".to_string(),
                        name: "class1.method1()".to_string(),
                        fingerprint: 8189722245693347360,
                        frame: Frame {
                            function: Some("class1.method1()".to_string()),
                            in_app: Some(true),
                            method_id: Some(1),
                            package: Some("class1".to_string()),
                            ..Default::default()
                        },
                        ..Default::default()
                    }))],
                )]
                .iter()
                .cloned()
                .collect(),
            },
        ];

        for test_case in test_cases.as_mut() {
            let call_trees = test_case
                .trace
                .call_trees_with_max_depth(test_case.max_depth)
                .unwrap();
            let call_json = serde_json::to_string(&call_trees).unwrap();
            let want_json = serde_json::to_string(&test_case.want).unwrap();
            //assert_eq!(test_case.want, call_trees, "test: `{}` failed", test_case.name)
            assert_eq!(want_json, call_json, "test: `{}` failed", test_case.name)
        }
    }
}
