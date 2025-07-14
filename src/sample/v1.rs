use crate::{
    frame::Frame,
    nodetree::Node,
    sample::SampleError,
    types::{
        CallTreeError, CallTreesU64, ClientSDK, DebugMeta, Platform, ProfileInterface, Transaction,
        TransactionMetadata,
    },
};

use super::ThreadMetadata;
use chrono::{DateTime, Utc};
use fnv_rs::Fnv64;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::HashMap, hash::Hasher, rc::Rc};

type FrameTuple<'a> = (usize, &'a Frame);

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct OSMetadata {
    name: String,
    version: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    build_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Measurement {
    pub unit: String,
    pub values: Vec<MeasurementValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MeasurementValue {
    pub elapsed_since_start_ns: u64,
    pub value: f64,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Device {
    architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    classification: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct RuntimeMetadata {
    pub(crate) name: String,
    pub(crate) version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct QueueMetadata {
    label: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Sample {
    pub stack_id: usize,
    pub thread_id: u64,
    pub elapsed_since_start_ns: u64,

    // cocoa only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Profile {
    pub frames: Vec<Frame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_metadata: Option<HashMap<String, QueueMetadata>>,
    pub samples: Vec<Sample>,
    pub stacks: Vec<Vec<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_metadata: Option<HashMap<String, ThreadMetadata>>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct SampleProfile {
    pub client_sdk: Option<ClientSDK>,

    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    pub debug_meta: DebugMeta,

    pub device: Device,

    pub environment: Option<String>,

    pub event_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurements: Option<HashMap<String, Measurement>>,

    pub os: OSMetadata,

    pub organization_id: u64,

    pub platform: Platform,

    pub project_id: u64,

    pub received: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,

    pub retention_days: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeMetadata>,

    pub profile: Profile,

    pub sampled: bool,

    pub timestamp: DateTime<Utc>,

    pub transaction: Transaction,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_metadata: Option<TransactionMetadata>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub transaction_tags: HashMap<String, String>,

    pub version: String,
}

impl SampleProfile {
    fn trim_cocoa_stacks(&mut self) {
        let mut mfi: i32 = -1;
        // Find main frame index in frames
        for (i, frame) in self.profile.frames.iter().enumerate() {
            if frame
                .function
                .as_ref()
                .is_some_and(|function| function.as_str() == "main")
            {
                mfi = i as i32;
            }
        }
        // We do nothing if we don't find it
        if mfi == -1 {
            return;
        }

        for stack in &mut self.profile.stacks {
            // Find main frame index in the stack
            let mut msi = stack.len();
            // Stop searching after 10 frames, it's not there
            let mut until: usize = 0;
            if stack.len() > 10 {
                until = stack.len() - 10;
            }
            for i in (until..stack.len()).rev() {
                let frame_idx = stack[i];
                if frame_idx == mfi as usize {
                    msi = i;
                    break;
                }
            }
            // Skip the stack if we're already at the end or we didn't find it
            if msi >= stack.len().saturating_sub(1) {
                continue;
            }
            // Filter unsymbolicated frames after the main frame index
            let mut ci = msi + 1;
            let loop_start_bound = ci;
            for i in loop_start_bound..stack.len() {
                let frame_index = stack[i];
                let frame = &self.profile.frames[frame_index];
                if let Some(symbolicator_status) = frame
                    .data
                    .as_ref()
                    .and_then(|data| data.symbolicator_status.as_ref())
                {
                    if symbolicator_status == "symbolicated" {
                        stack[ci] = frame_index;
                        ci += 1;
                    }
                }
            }
            stack.truncate(ci);
        }
    }
}

impl Profile {
    fn trim_python_stacks(&mut self) {
        // Find the module frame index in frames
        let module_frame_index = self.frames.iter().position(|f| {
            f.file.as_deref() == Some("<string>") && f.function.as_deref() == Some("<module>")
        });

        // We do nothing if we don't find it
        let module_frame_index = match module_frame_index {
            Some(index) => index,
            None => return,
        };

        // Iterate through stacks and trim module frame if it's the last frame
        for stack in &mut self.stacks {
            if let Some(&last_frame) = stack.last() {
                if last_frame == module_frame_index {
                    // Found the module frame so trim it
                    stack.pop();
                }
            }
        }
    }

    fn find_next_active_stack_id(&self, samples_indices: &[usize], i: usize) -> (i64, i64) {
        for (j, sample_idx) in samples_indices.iter().enumerate().skip(i) {
            let sample = &self.samples[*sample_idx];
            if is_active_stack(&self.stacks[sample.stack_id]) {
                return (j as i64, sample.stack_id as i64);
            }
        }
        (-1, -1)
    }

    fn frames_list(&self, stack_id: usize) -> Vec<FrameTuple> {
        let stack = &self.stacks[stack_id];
        let mut frames: Vec<(usize, &Frame)> = Vec::with_capacity(stack.len());
        for frame_id in stack {
            frames.push((*frame_id, &self.frames[*frame_id]));
        }
        frames
    }

    fn samples_by_thread_id(&self) -> (Vec<u64>, HashMap<u64, Vec<usize>>) {
        let mut samples: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut thread_ids: Vec<u64> = Vec::new();
        for (i, sample) in self.samples.iter().enumerate() {
            if !samples.contains_key(&sample.thread_id) {
                thread_ids.push(sample.thread_id);
            }
            samples.entry(sample.thread_id).or_default().push(i);
        }
        thread_ids.sort_unstable();
        (thread_ids, samples)
    }

    fn replace_idle_stacks(&mut self) {
        let (thread_ids, samples_by_thread_id) = self.samples_by_thread_id();

        for thread in thread_ids {
            let samples_indices = samples_by_thread_id
                .get(&thread)
                .expect("sample for thread id not found");
            let mut previous_active_stack_id: i64 = -1;
            let mut next_active_sample_index: i64 = 0;
            let mut next_active_stack_id: i64 = 0;

            let mut i = 0;
            while i < samples_indices.len() {
                let sample = &mut self.samples[samples_indices[i]];

                // keep track of the previous active sample as we go
                if is_active_stack(&self.stacks[sample.stack_id]) {
                    previous_active_stack_id = sample.stack_id as i64;
                    i += 1;
                    continue;
                }

                // if there's no frame, the thread is considered idle at this time
                sample.state = Some("idle".to_string());

                // if it's an idle stack but we don't have a previous active stack
                // we keep looking
                if previous_active_stack_id == -1 {
                    i += 1;
                    continue;
                }

                if i as i64 >= next_active_sample_index {
                    (next_active_sample_index, next_active_stack_id) =
                        self.find_next_active_stack_id(samples_indices, i);
                    if next_active_sample_index == -1 {
                        // no more active sample on this thread
                        while i < samples_indices.len() {
                            let sample = &mut self.samples[samples_indices[i]];
                            sample.state = Some("idle".to_string());
                            i += 1;
                        }
                        break;
                    }
                } // end if

                let previous_frames = self.frames_list(previous_active_stack_id as usize);
                let next_frames = self.frames_list(next_active_stack_id as usize);
                let common_frames = find_common_frames(&previous_frames, &next_frames);

                // add the common stack to the list of stacks
                let mut common_stack: Vec<usize> = Vec::with_capacity(common_frames.len());
                for frame in common_frames {
                    common_stack.push(frame.0);
                }
                let common_stack_id = self.stacks.len();
                self.stacks.push(common_stack);

                // replace all idle stacks until next active sample
                while i < next_active_sample_index as usize {
                    let sample = &mut self.samples[samples_indices[i]];
                    sample.stack_id = common_stack_id;
                    sample.state = Some("idle".to_string());
                    i += 1;
                }
            } // end while
        }
    }
}

fn is_active_stack(stack: &[usize]) -> bool {
    !stack.is_empty()
}

fn find_common_frames<'a>(
    a: &'a [FrameTuple<'a>],
    b: &'a [FrameTuple<'a>],
) -> Vec<&'a FrameTuple<'a>> {
    let mut common_frames: Vec<&FrameTuple> = Vec::new();

    for (frame_a, frame_b) in a.iter().rev().zip(b.iter().rev()) {
        if frame_a.0 == frame_b.0 {
            common_frames.push(frame_a);
        } else {
            break;
        }
    }
    common_frames.reverse();
    common_frames
}

impl ProfileInterface for SampleProfile {
    fn get_platform(&self) -> Platform {
        self.platform
    }

    /// Serialize the given data structure as a JSON byte vector.
    fn to_json_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    fn get_profile_id(&self) -> &str {
        &self.event_id
    }

    fn get_organization_id(&self) -> u64 {
        self.organization_id
    }

    fn get_project_id(&self) -> u64 {
        self.project_id
    }

    fn get_received(&self) -> i64 {
        self.received
    }

    fn get_release(&self) -> Option<&str> {
        self.release.as_deref()
    }

    fn get_retention_days(&self) -> i32 {
        self.retention_days
    }

    fn get_timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn normalize(&mut self) {
        for frame in &mut self.profile.frames {
            frame.normalize(self.platform);
        }
        if self.platform == Platform::Cocoa {
            self.trim_cocoa_stacks();
        } else if self.platform == Platform::Python {
            self.profile.trim_python_stacks();
        }

        self.profile.replace_idle_stacks();
    }

    fn storage_path(&self) -> String {
        format!(
            "{}/{}/{}",
            self.organization_id, self.project_id, self.event_id
        )
    }

    fn call_trees(&mut self) -> Result<CallTreesU64, CallTreeError> {
        // Sort samples by timestamp
        self.profile
            .samples
            .sort_by(|a, b| a.elapsed_since_start_ns.cmp(&b.elapsed_since_start_ns));

        let active_thread_id = self.transaction.active_thread_id;
        let mut trees_by_thread_id: HashMap<u64, Vec<Rc<RefCell<Node>>>> = HashMap::new();
        let mut samples_by_thread_id: HashMap<u64, Vec<&Sample>> = HashMap::new();

        for sample in &self.profile.samples {
            samples_by_thread_id
                .entry(sample.thread_id)
                .or_default()
                .push(sample);
        }

        let mut hasher = Fnv64::default();

        for (thread_id, samples) in samples_by_thread_id {
            if thread_id != active_thread_id {
                continue;
            }

            // Skip last sample as it's only used for timestamp
            for sample_index in 0..samples.len() - 1 {
                let sample = &samples[sample_index];

                // Validate stack ID
                if self.profile.stacks.len() <= (sample.stack_id) {
                    return Err(CallTreeError::Sample(SampleError::InvalidStackId));
                }

                let stack = &self.profile.stacks[sample.stack_id];

                // Validate frame IDs
                for &frame_id in stack.iter().rev() {
                    if self.profile.frames.len() <= (frame_id) {
                        return Err(CallTreeError::Sample(SampleError::InvalidFrameId));
                    }
                }

                // Here while we save the nextTimestamp val, we convert it to nanosecond
                // since the Node struct and utilities use uint64 ns values
                let next_timestamp = samples[sample_index + 1].elapsed_since_start_ns;
                let sample_timestamp = sample.elapsed_since_start_ns;

                let mut current: Option<Rc<RefCell<Node>>> = None;

                // Process stack frames from bottom to top
                for &frame_id in stack.iter().rev() {
                    let frame = &self.profile.frames[frame_id];

                    // Calculate fingerprint
                    frame.write_to_hash(&mut hasher);
                    let fingerprint = hasher.finish();

                    match current {
                        None => {
                            let trees = trees_by_thread_id.entry(thread_id).or_default();

                            if let Some(last_tree) = trees.last() {
                                if last_tree.borrow().fingerprint == fingerprint
                                    && last_tree.borrow().end_ns == sample_timestamp
                                {
                                    last_tree.borrow_mut().update(next_timestamp);
                                    current = Some(Rc::clone(last_tree));
                                    continue;
                                }
                            }

                            let new_node = Node::from_frame(
                                frame,
                                sample_timestamp,
                                next_timestamp,
                                fingerprint,
                            );
                            trees.push(Rc::clone(&new_node));
                            current = Some(new_node);
                        }
                        Some(node) => {
                            let i = node.borrow().children.len();
                            if !node.borrow().children.is_empty()
                                && node.borrow().children[i - 1].borrow().fingerprint == fingerprint
                                && node.borrow().children[i - 1].borrow().end_ns == sample_timestamp
                            {
                                let last_child = &node.borrow().children[i - 1];
                                last_child.borrow_mut().update(next_timestamp);
                                current = Some(Rc::clone(last_child));
                                continue;
                            } else {
                                let new_node = Node::from_frame(
                                    frame,
                                    sample_timestamp,
                                    next_timestamp,
                                    fingerprint,
                                );
                                node.borrow_mut().children.push(Rc::clone(&new_node));
                                current = Some(new_node);
                            }
                        } // end Some
                    } // end match
                } // end stack loop
                hasher = Fnv64::default();
            }
        }
        Ok(trees_by_thread_id)
    }

    fn sdk_name(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.name.as_str())
    }

    fn sdk_version(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.version.as_str())
    }

    fn duration_ns(&self) -> u64 {
        if self.profile.samples.is_empty() {
            return 0;
        }
        self.profile.samples.last().unwrap().elapsed_since_start_ns
            - self.profile.samples.first().unwrap().elapsed_since_start_ns
    }

    fn get_transaction(&self) -> Cow<Transaction> {
        Cow::Borrowed(&self.transaction)
    }

    fn get_transaction_tags(&self) -> &HashMap<String, String> {
        &self.transaction_tags
    }

    fn get_debug_meta(&self) -> &DebugMeta {
        &self.debug_meta
    }

    fn get_measurements(&self) -> Option<&HashMap<String, Measurement>> {
        self.measurements.as_ref()
    }

    fn is_sampled(&self) -> bool {
        self.sampled
    }

    fn set_profile_id(&mut self, profile_id: String) {
        self.event_id = profile_id
    }

    fn get_metadata(&self) -> crate::types::Metadata {
        crate::types::Metadata {
            android_api_level: None, // Not available in v1 sample profiles
            architecture: self.device.architecture.clone(),
            device_classification: self.device.classification.clone(),
            device_locale: self.device.locale.clone(),
            device_manufacturer: self.device.manufacturer.clone(),
            device_model: self.device.model.as_deref().unwrap_or("").to_string(),
            device_os_build_number: self.os.build_number.clone(),
            device_os_name: self.os.name.clone(),
            device_os_version: self.os.version.clone(),
            id: self.event_id.clone(),
            project_id: self.project_id.to_string(),
            sdk_name: self.client_sdk.as_ref().map(|sdk| sdk.name.clone()),
            sdk_version: self.client_sdk.as_ref().map(|sdk| sdk.version.clone()),
            timestamp: self.timestamp.timestamp(),
            trace_duration_ms: self.duration_ns() as f64 / 1_000_000.0,
            transaction_id: self.transaction.id.clone(),
            transaction_name: self.transaction.name.clone(),
            version_code: None, // Not available in v1 sample profiles
            version_name: self.release.clone(),
        }
    }
}

#[cfg(test)]
mod tests {

    use std::{cell::RefCell, rc::Rc};

    use serde_path_to_error::Error;

    use crate::{
        frame::{self, Data, Frame},
        sample::v1::{Profile, Sample, SampleProfile},
        types::{CallTreesU64, Platform, ProfileInterface, Transaction},
    };

    #[test]
    fn test_sample_format_v1_cocoa() {
        let payload = include_bytes!("../../tests/fixtures/sample/v1/valid_cocoa.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleProfile, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{r:#?}")
    }

    #[test]
    fn test_sample_format_v1_python() {
        let payload = include_bytes!("../../tests/fixtures/sample/v1/valid_python.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleProfile, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{r:#?}")
    }

    #[test]
    fn test_trim_cocoa_stacks() {
        struct TestStruct {
            name: String,
            profile: SampleProfile,
            want: SampleProfile,
        }

        let mut test_cases = [
            TestStruct {
                name: "Remove frames leading to main".to_string(),
                profile: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![vec![1, 0, 2, 3, 3]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![vec![1, 0, 2]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
            TestStruct {
                name: "Remove frames in-between main and a symbolicated frame".to_string(),
                profile: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("start_sim".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![vec![1, 0, 2, 3, 3, 4]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("start_sim".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![vec![1, 0, 2, 4]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
            TestStruct {
                name: "Remove nothing since we couldn't find main".to_string(),
                profile: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("unsymbolicated_main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("start_sim".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![vec![1, 0, 2, 3, 3, 4]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("unsymbolicated_main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("start_sim".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![vec![1, 0, 2, 3, 3, 4]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
            TestStruct {
                name: "Remove frames on many stacks".to_string(),
                profile: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("start_sim".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![
                            vec![0, 2, 3, 4, 3],
                            vec![1, 0, 2, 3, 4, 3],
                            vec![0, 2, 3, 4, 3],
                            vec![1, 0, 2, 3, 4, 3],
                            vec![0, 2, 3, 4, 3],
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: SampleProfile {
                    profile: Profile {
                        frames: vec![
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function1".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("function2".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("main".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("missing".to_string()),
                                    ..Default::default()
                                }),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                            frame::Frame {
                                data: Some(Data {
                                    symbolicator_status: Some("symbolicated".to_string()),
                                    ..Default::default()
                                }),
                                function: Some("start_sim".to_string()),
                                in_app: Some(true),
                                platform: Some(Platform::Cocoa),
                                ..Default::default()
                            },
                        ], // end frames definition
                        stacks: vec![
                            vec![0, 2, 4],
                            vec![1, 0, 2, 4],
                            vec![0, 2, 4],
                            vec![1, 0, 2, 4],
                            vec![0, 2, 4],
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        ];

        for test_case in test_cases.as_mut() {
            test_case.profile.trim_cocoa_stacks();
            assert_eq!(
                test_case.profile, test_case.want,
                "test: {} failed.",
                test_case.name
            );
        }
    }

    #[test]
    fn test_trim_python_stacks() {
        struct TestStruct {
            name: String,
            profile: Profile,
            want: Profile,
        }

        let mut test_cases = [
            TestStruct {
                name: "Remove module frame at the end of a stack".to_string(),
                profile: Profile {
                    frames: vec![
                        Frame {
                            file: Some("<string>".to_string()),
                            module: Some("__main__".to_string()),
                            in_app: Some(true),
                            line: Some(11),
                            function: Some("<module>".to_string()),
                            path: Some("/usr/src/app/<string>".to_string()),
                            platform: Some(Platform::Python),
                            ..Default::default()
                        },
                        Frame {
                            file: Some("app/util.py".to_string()),
                            module: Some("app.util".to_string()),
                            in_app: Some(true),
                            line: Some(98),
                            function: Some("foobar".to_string()),
                            path: Some("/usr/src/app/util.py".to_string()),
                            platform: Some(Platform::Python),
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![1, 0]],
                    ..Default::default()
                },
                want: Profile {
                    frames: vec![
                        Frame {
                            file: Some("<string>".to_string()),
                            module: Some("__main__".to_string()),
                            in_app: Some(true),
                            line: Some(11),
                            function: Some("<module>".to_string()),
                            path: Some("/usr/src/app/<string>".to_string()),
                            platform: Some(Platform::Python),
                            ..Default::default()
                        },
                        Frame {
                            file: Some("app/util.py".to_string()),
                            module: Some("app.util".to_string()),
                            in_app: Some(true),
                            line: Some(98),
                            function: Some("foobar".to_string()),
                            path: Some("/usr/src/app/util.py".to_string()),
                            platform: Some(Platform::Python),
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![1]],
                    ..Default::default()
                },
            }, // end first case
        ];

        for test in test_cases.as_mut() {
            test.profile.trim_python_stacks();
            assert_eq!(test.profile, test.want, "test `{}` failed", test.name);
        }
    }

    #[test]
    fn test_replace_dile_stacks() {
        use pretty_assertions::assert_eq;
        struct TestStruct {
            name: String,
            profile: Profile,
            want: Profile,
        }

        let mut test_cases = [
            TestStruct {
                name: "replace idle stacks between 2 actives".to_string(),
                profile: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 10,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 20,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 30,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 40,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 50,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0], vec![4, 2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                want: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 10,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 20,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 30,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 40,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 50,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0], vec![4, 2, 1, 0], vec![2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            },
            TestStruct {
                name: "replace idle stacks between 2 actives with idle around".to_string(),
                profile: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 10,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 20,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 30,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 40,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 50,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0], vec![4, 2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                want: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 10,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 20,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 30,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 40,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 50,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0], vec![4, 2, 1, 0], vec![2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            },
            TestStruct {
                name: "do nothing since only one active stack".to_string(),
                profile: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 10,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 20,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 30,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 40,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 50,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                want: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 10,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 20,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 30,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 40,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 50,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            },
            TestStruct {
                name: "replace idle stacks between 2 actives on different threads".to_string(),
                profile: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 10,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 10,
                            thread_id: 2,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 20,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 20,
                            thread_id: 2,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 30,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 30,
                            thread_id: 2,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 40,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 40,
                            thread_id: 2,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 50,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 50,
                            thread_id: 2,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0], vec![4, 2, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                want: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 10,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 10,
                            thread_id: 2,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 20,
                            thread_id: 1,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 4,
                            elapsed_since_start_ns: 20,
                            thread_id: 2,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 30,
                            thread_id: 1,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 4,
                            elapsed_since_start_ns: 30,
                            thread_id: 2,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 40,
                            thread_id: 1,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 4,
                            elapsed_since_start_ns: 40,
                            thread_id: 2,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 50,
                            thread_id: 1,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 50,
                            thread_id: 2,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![
                        vec![],
                        vec![4, 3, 2, 1, 0],
                        vec![4, 2, 1, 0],
                        vec![2, 1, 0],
                        vec![2, 1, 0],
                    ],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            },
            TestStruct {
                name: "replace multiple idle stacks between 2 actives with idle stacks around"
                    .to_string(),
                profile: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 10,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 20,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 30,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 40,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 50,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 60,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 70,
                            ..Default::default()
                        },
                    ],
                    stacks: vec![vec![], vec![4, 3, 2, 1, 0], vec![4, 2, 1, 0], vec![4, 1, 0]],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                want: Profile {
                    samples: vec![
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 10,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 1,
                            elapsed_since_start_ns: 20,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 4,
                            elapsed_since_start_ns: 30,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 2,
                            elapsed_since_start_ns: 40,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 5,
                            elapsed_since_start_ns: 50,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 3,
                            elapsed_since_start_ns: 60,
                            ..Default::default()
                        },
                        Sample {
                            stack_id: 0,
                            elapsed_since_start_ns: 70,
                            state: Some("idle".to_string()),
                            ..Default::default()
                        },
                    ],
                    stacks: vec![
                        vec![],
                        vec![4, 3, 2, 1, 0],
                        vec![4, 2, 1, 0],
                        vec![4, 1, 0],
                        vec![2, 1, 0],
                        vec![1, 0],
                    ],
                    frames: vec![
                        Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function1".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function2".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function3".to_string()),
                            ..Default::default()
                        },
                        Frame {
                            function: Some("function4".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            },
        ];
        for test in test_cases.as_mut() {
            test.profile.replace_idle_stacks();
            assert_eq!(test.profile, test.want, "test `{}` failed", test.name);
        }
    }

    #[test]
    fn test_call_trees() {
        use crate::nodetree::Node;
        use pretty_assertions::assert_eq;
        struct TestStruct {
            name: String,
            profile: SampleProfile,
            want: CallTreesU64,
        }

        let mut test_cases = [
            TestStruct {
                name: "call tree with multiple samples per frame".to_string(),
                profile: SampleProfile {
                    profile: Profile {
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 10,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: 1,
                                elapsed_since_start_ns: 40,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: 1,
                                elapsed_since_start_ns: 50,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![1, 0], vec![2, 1, 0]],
                        frames: vec![
                            Frame {
                                function: Some("function0".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                function: Some("function2".to_string()),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    },
                    transaction: Transaction {
                        active_thread_id: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                }, //end chucnk
                want: [(
                    1,
                    vec![Rc::new(RefCell::new(Node {
                        duration_ns: 40,
                        end_ns: 50,
                        fingerprint: 6903369137866438128,
                        is_application: true,
                        name: "function0".to_string(),
                        sample_count: 2,
                        start_ns: 10,
                        frame: Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        children: vec![
                            Rc::new(RefCell::new(Node {
                                duration_ns: 40,
                                end_ns: 50,
                                start_ns: 10,
                                fingerprint: 17095743776245828002,
                                is_application: true,
                                name: "function1".to_string(),
                                sample_count: 2,
                                frame: Frame {
                                    function: Some("function1".to_string()),
                                    ..Default::default()
                                },
                                children: vec![Rc::new(RefCell::new(Node {
                                    duration_ns: 10,
                                    end_ns: 50,
                                    fingerprint: 16529420490907277225,
                                    is_application: true,
                                    name: "function2".to_string(),
                                    sample_count: 1,
                                    start_ns: 40,
                                    frame: Frame {
                                        function: Some("function2".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }))],
                                ..Default::default()
                            })), // TODO finish
                        ],
                        ..Default::default()
                    }))],
                )]
                .iter()
                .cloned()
                .collect(),
            }, //end first test case
            TestStruct {
                name: "call tree with single sample frames".to_string(),
                profile: SampleProfile {
                    transaction: Transaction {
                        active_thread_id: 1,
                        ..Default::default()
                    },
                    profile: Profile {
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 10,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: 1,
                                elapsed_since_start_ns: 40,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![1, 0], vec![2, 1, 0]],
                        frames: vec![
                            Frame {
                                function: Some("function0".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                function: Some("function2".to_string()),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                }, //end chucnk
                want: [(
                    1,
                    vec![Rc::new(RefCell::new(Node {
                        duration_ns: 30,
                        end_ns: 40,
                        fingerprint: 6903369137866438128,
                        is_application: true,
                        name: "function0".to_string(),
                        sample_count: 1,
                        start_ns: 10,
                        frame: Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        children: vec![Rc::new(RefCell::new(Node {
                            duration_ns: 30,
                            end_ns: 40,
                            fingerprint: 17095743776245828002,
                            is_application: true,
                            name: "function1".to_string(),
                            sample_count: 1,
                            start_ns: 10,
                            frame: Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        }))],
                        ..Default::default()
                    }))],
                )]
                .iter()
                .cloned()
                .collect(),
            }, //end second test case
            TestStruct {
                name: "call tree with single samples".to_string(),
                profile: SampleProfile {
                    transaction: Transaction {
                        active_thread_id: 1,
                        ..Default::default()
                    },
                    profile: Profile {
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 10,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: 1,
                                elapsed_since_start_ns: 20,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 2,
                                thread_id: 1,
                                elapsed_since_start_ns: 30,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![0], vec![1], vec![2]],
                        frames: vec![
                            Frame {
                                function: Some("function0".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                function: Some("function2".to_string()),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    },
                    ..Default::default()
                }, //end chucnk
                want: [(
                    1,
                    vec![
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            end_ns: 20,
                            fingerprint: 6903369137866438128,
                            is_application: true,
                            name: "function0".to_string(),
                            sample_count: 1,
                            start_ns: 10,
                            frame: Frame {
                                function: Some("function0".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            end_ns: 30,
                            fingerprint: 6903370237378066339,
                            is_application: true,
                            name: "function1".to_string(),
                            sample_count: 1,
                            start_ns: 20,
                            frame: Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                    ],
                )]
                .iter()
                .cloned()
                .collect(),
            }, //end third test case
        ];

        for test_case in test_cases.as_mut() {
            let call_trees = test_case.profile.call_trees().unwrap();
            assert_eq!(
                call_trees, test_case.want,
                "test: {} failed.",
                test_case.name
            );
        }
    }
}
