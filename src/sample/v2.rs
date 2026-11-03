use fnv_rs::Fnv64;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::rc::Rc;

use super::{SampleError, ThreadMetadata};
use crate::frame::Frame;
use crate::nodetree::Node;
use crate::types::{CallTreeError, CallTreesStr, ChunkInterface};
use crate::types::{ClientSDK, DebugMeta};

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct SampleChunk {
    pub chunk_id: String,

    pub profiler_id: String,

    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    pub debug_meta: DebugMeta,

    pub client_sdk: Option<ClientSDK>,

    pub environment: Option<String>,

    pub platform: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,

    pub version: String,

    pub profile: SampleData,

    pub organization_id: u64,

    pub project_id: u64,

    pub received: f64,

    pub retention_days: i32,

    // `measurements` contains CPU/memory measurements we do during the capture of the chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurements: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct SampleData {
    pub frames: Vec<Frame>,
    pub samples: Vec<Sample>,
    pub stacks: Vec<Vec<i32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_metadata: Option<std::collections::HashMap<String, ThreadMetadata>>,
}

impl SampleData {
    /// Drops synthetic android frames (ART runtime, unsymbolicatable anonymous
    /// frames) and re-indexes the stacks that reference them, compacting both
    /// vectors in place. Samples reference stacks by id, so they're unaffected.
    fn trim_android_stacks(&mut self) {
        let mut next_index: i32 = 0;
        // format: index_map[old_index] -> new_index or None if synthetic
        let index_map: Vec<Option<i32>> = self
            .frames
            .iter()
            .map(|frame| {
                if frame.is_synthetic_android_frame() {
                    None
                } else {
                    let index = next_index;
                    next_index += 1;
                    Some(index)
                }
            })
            .collect();

        if next_index as usize == self.frames.len() {
            return; // no synthetic frames present
        }

        // Drop synthetic frames from stacks
        for stack in &mut self.stacks {
            stack.retain_mut(|id| match index_map.get(*id as usize) {
                Some(Some(new_index)) => {
                    *id = *new_index;
                    true
                }
                Some(None) => false,
                None => true, // out of range; leave for call_trees to reject
            });
        }

        // Drop the synthetic frames themselves in-place.
        let mut frame_indices = index_map.iter();
        self.frames
            .retain(|_| frame_indices.next().unwrap().is_some());
    }

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
                if last_frame as usize == module_frame_index {
                    // Found the module frame so trim it
                    stack.pop();
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Sample {
    #[serde(rename = "stack_id")]
    pub stack_id: i32,
    #[serde(rename = "thread_id")]
    pub thread_id: String,
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
}

impl ChunkInterface for SampleChunk {
    fn call_trees(
        &mut self,
        active_thread_id: Option<&str>,
    ) -> Result<CallTreesStr<'_>, CallTreeError> {
        // Sort samples by timestamp
        self.profile
            .samples
            .sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

        let mut trees_by_thread_id: HashMap<Cow<str>, Vec<Rc<RefCell<Node>>>> = HashMap::new();
        let mut samples_by_thread_id: HashMap<&str, Vec<&Sample>> = HashMap::new();

        for sample in &self.profile.samples {
            samples_by_thread_id
                .entry(sample.thread_id.as_ref())
                .or_default()
                .push(sample);
        }

        let mut hasher = Fnv64::default();

        for (thread_id, samples) in samples_by_thread_id {
            // Skip if we have an active_thread_id and the sample
            // thread id does not match.
            if let Some(active_id) = active_thread_id {
                if thread_id != active_id {
                    continue;
                }
            }

            // Skip last sample as it's only used for timestamp
            for sample_index in 0..samples.len() - 1 {
                let sample = &samples[sample_index];

                // Validate stack ID
                if self.profile.stacks.len() <= (sample.stack_id as usize) {
                    return Err(CallTreeError::Sample(SampleError::InvalidStackId));
                }

                let stack = &self.profile.stacks[sample.stack_id as usize];

                // Validate frame IDs
                for &frame_id in stack.iter().rev() {
                    if self.profile.frames.len() <= (frame_id as usize) {
                        return Err(CallTreeError::Sample(SampleError::InvalidFrameId));
                    }
                }

                // Here while we save the nextTimestamp val, we convert it to nanosecond
                // since the Node struct and utilities use uint64 ns values
                let next_timestamp = (&samples[sample_index + 1].timestamp * 1e9) as u64;
                let sample_timestamp = (sample.timestamp * 1e9) as u64;

                let mut current: Option<Rc<RefCell<Node>>> = None;

                // Process stack frames from bottom to top
                for &frame_id in stack.iter().rev() {
                    let frame = &self.profile.frames[frame_id as usize];

                    // Calculate fingerprint
                    frame.write_to_hash(&mut hasher);
                    let fingerprint = hasher.finish();

                    match current {
                        None => {
                            let trees = trees_by_thread_id
                                .entry(Cow::Borrowed(thread_id))
                                .or_default();

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

    fn normalize(&mut self) {
        if self.platform.as_str() == "android" {
            self.profile.trim_android_stacks();
        }
        for frame in &mut self.profile.frames {
            frame.normalize(&self.platform);
        }
        if self.platform.as_str() == "python" {
            self.profile.trim_python_stacks();
        }
    }

    fn get_environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    fn get_chunk_id(&self) -> &str {
        &self.chunk_id
    }

    fn get_organization_id(&self) -> u64 {
        self.organization_id
    }

    fn get_platform(&self) -> String {
        self.platform.clone()
    }

    fn get_profiler_id(&self) -> &str {
        &self.profiler_id
    }

    fn get_project_id(&self) -> u64 {
        self.project_id
    }

    fn get_received(&self) -> f64 {
        self.received
    }

    fn get_release(&self) -> Option<&str> {
        self.release.as_deref()
    }

    fn get_retention_days(&self) -> i32 {
        self.retention_days
    }

    fn duration_ms(&self) -> u64 {
        ((self.end_timestamp() - self.start_timestamp()).round() * 1e3) as u64
    }

    fn start_timestamp(&self) -> f64 {
        if self.profile.samples.is_empty() {
            0.0
        } else {
            self.profile.samples[0].timestamp
        }
    }

    fn end_timestamp(&self) -> f64 {
        if self.profile.samples.is_empty() {
            0.0
        } else {
            self.profile.samples.last().unwrap().timestamp
        }
    }

    fn sdk_name(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.name.as_str())
    }

    fn sdk_version(&self) -> Option<&str> {
        self.client_sdk.as_deref().map(|sdk| sdk.version.as_str())
    }

    fn storage_path(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.organization_id, self.project_id, self.profiler_id, self.chunk_id
        )
    }

    /// Serialize the given data structure as a JSON byte vector.
    fn to_json_vec(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, cell::RefCell, rc::Rc};

    use serde_path_to_error::Error;

    use super::SampleChunk;
    use crate::{
        frame::Frame,
        sample::v2::{Sample, SampleData},
        types::{CallTreesStr, ChunkInterface},
    };

    use pretty_assertions::assert_eq;

    #[test]
    fn test_sample_format_v2_cocoa() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_cocoa.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{r:#?}")
    }

    #[test]
    fn test_sample_format_v2_android() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_android.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{r:#?}")
    }

    #[test]
    fn test_sample_format_v2_python() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_python.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{r:#?}")
    }

    #[test]
    fn test_call_trees() {
        use crate::nodetree::Node;
        struct TestStruct<'a> {
            name: String,
            chunk: SampleChunk,
            want: CallTreesStr<'a>,
        }

        let mut test_cases = [
            TestStruct {
                name: "call tree with multiple samples per frame".to_string(),
                chunk: SampleChunk {
                    profile: SampleData {
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: "1".to_string(),
                                timestamp: 0.010,
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: "1".to_string(),
                                timestamp: 0.040,
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: "1".to_string(),
                                timestamp: 0.050,
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
                    Cow::Borrowed("1"),
                    vec![Rc::new(RefCell::new(Node {
                        duration_ns: 40_000_000,
                        end_ns: 50_000_000,
                        fingerprint: 6903369137866438128,
                        is_application: true,
                        name: "function0".to_string(),
                        sample_count: 2,
                        start_ns: 10_000_000,
                        frame: Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        children: vec![
                            Rc::new(RefCell::new(Node {
                                duration_ns: 40_000_000,
                                end_ns: 50_000_000,
                                start_ns: 10_000_000,
                                fingerprint: 17095743776245828002,
                                is_application: true,
                                name: "function1".to_string(),
                                sample_count: 2,
                                frame: Frame {
                                    function: Some("function1".to_string()),
                                    ..Default::default()
                                },
                                children: vec![Rc::new(RefCell::new(Node {
                                    duration_ns: 10_000_000,
                                    end_ns: 50_000_000,
                                    fingerprint: 16529420490907277225,
                                    is_application: true,
                                    name: "function2".to_string(),
                                    sample_count: 1,
                                    start_ns: 40_000_000,
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
                chunk: SampleChunk {
                    profile: SampleData {
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: "1".to_string(),
                                timestamp: 0.010,
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: "1".to_string(),
                                timestamp: 0.040,
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
                    Cow::Borrowed("1"),
                    vec![Rc::new(RefCell::new(Node {
                        duration_ns: 30_000_000,
                        end_ns: 40_000_000,
                        fingerprint: 6903369137866438128,
                        is_application: true,
                        name: "function0".to_string(),
                        sample_count: 1,
                        start_ns: 10_000_000,
                        frame: Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        children: vec![Rc::new(RefCell::new(Node {
                            duration_ns: 30_000_000,
                            end_ns: 40_000_000,
                            fingerprint: 17095743776245828002,
                            is_application: true,
                            name: "function1".to_string(),
                            sample_count: 1,
                            start_ns: 10_000_000,
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
                chunk: SampleChunk {
                    profile: SampleData {
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: "1".to_string(),
                                timestamp: 0.010,
                            },
                            Sample {
                                stack_id: 1,
                                thread_id: "1".to_string(),
                                timestamp: 0.020,
                            },
                            Sample {
                                stack_id: 2,
                                thread_id: "1".to_string(),
                                timestamp: 0.030,
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
                    Cow::Borrowed("1"),
                    vec![
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 6903369137866438128,
                            is_application: true,
                            name: "function0".to_string(),
                            sample_count: 1,
                            start_ns: 10_000_000,
                            frame: Frame {
                                function: Some("function0".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10_000_000,
                            end_ns: 30_000_000,
                            fingerprint: 6903370237378066339,
                            is_application: true,
                            name: "function1".to_string(),
                            sample_count: 1,
                            start_ns: 20_000_000,
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
            let call_trees = test_case.chunk.call_trees(None).unwrap();
            assert_eq!(
                call_trees, test_case.want,
                "test: {} failed.",
                test_case.name
            );
        }
    }

    #[test]
    fn test_trim_python_stacks() {
        struct TestStruct {
            name: String,
            chunk: SampleChunk,
            want: SampleChunk,
        }

        let mut test_cases = [
            TestStruct {
                name: "Remove module frame at the end of a stack".to_string(),
                chunk: SampleChunk {
                    platform: "python".to_string(),
                    profile: SampleData {
                        frames: vec![
                            Frame {
                                file: Some("<string>".to_string()),
                                module: Some("__main__".to_string()),
                                in_app: Some(true),
                                line: Some(11),
                                function: Some("<module>".to_string()),
                                path: Some("/usr/src/app/<string>".to_string()),
                                platform: Some("python".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                file: Some("app/util.py".to_string()),
                                module: Some("app.util".to_string()),
                                in_app: Some(true),
                                line: Some(98),
                                function: Some("foobar".to_string()),
                                path: Some("/usr/src/app/util.py".to_string()),
                                platform: Some("python".to_string()),
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![1, 0]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: SampleChunk {
                    platform: "python".to_string(),
                    profile: SampleData {
                        frames: vec![
                            Frame {
                                file: Some("<string>".to_string()),
                                module: Some("__main__".to_string()),
                                in_app: Some(true),
                                line: Some(11),
                                function: Some("<module>".to_string()),
                                path: Some("/usr/src/app/<string>".to_string()),
                                platform: Some("python".to_string()),
                                ..Default::default()
                            },
                            Frame {
                                file: Some("app/util.py".to_string()),
                                module: Some("app.util".to_string()),
                                in_app: Some(true),
                                line: Some(98),
                                function: Some("foobar".to_string()),
                                path: Some("/usr/src/app/util.py".to_string()),
                                platform: Some("python".to_string()),
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![1]],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }, // end first case
        ];

        for test in test_cases.as_mut() {
            test.chunk.normalize();
            assert_eq!(test.chunk, test.want, "test `{}` failed", test.name);
        }
    }

    #[test]
    fn test_trim_android_stacks() {
        let art = |p: &str| Frame {
            package: Some(p.to_string()),
            ..Default::default()
        };
        let named = |f: &str| Frame {
            function: Some(f.to_string()),
            ..Default::default()
        };

        let sample = |stack_id: i32| Sample {
            stack_id,
            thread_id: "1".to_string(),
            timestamp: 0.0,
        };

        let mut data = SampleData {
            frames: vec![
                named("app"),                                 // 0 -> 0
                art("apex/com.android.art/lib64/libart.so"),  // 1 dropped
                art("system/lib64/libhwui.so"),               // 2 -> 1 (not ART)
                art("/apex/com.android.art/lib64/libart.so"), // 3 dropped
                named("app2"),                                // 4 -> 2
            ],
            // Samples reference stacks by id, which trimming must not touch.
            samples: vec![sample(0), sample(1), sample(2)],
            stacks: vec![vec![0, 1, 2, 3, 4], vec![1, 3], vec![2]],
            thread_metadata: None,
        };

        data.trim_android_stacks();

        assert_eq!(
            data.frames
                .iter()
                .map(|f| f.function.clone())
                .collect::<Vec<_>>(),
            vec![Some("app".to_string()), None, Some("app2".to_string())]
        );
        // ART frames vanish from stacks; the all-ART stack becomes empty.
        assert_eq!(data.stacks, vec![vec![0, 1, 2], vec![], vec![1]]);
        // Stack ids on samples are untouched.
        assert_eq!(
            data.samples.iter().map(|s| s.stack_id).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn test_trim_android_stacks_noop() {
        let mut data = SampleData {
            frames: vec![
                Frame {
                    function: Some("app".to_string()),
                    ..Default::default()
                },
                Frame {
                    package: Some("system/lib64/libhwui.so".to_string()),
                    ..Default::default()
                },
            ],
            samples: vec![],
            stacks: vec![vec![0, 1]],
            thread_metadata: None,
        };

        data.trim_android_stacks();

        assert_eq!(data.frames.len(), 2);
        assert_eq!(data.stacks, vec![vec![0, 1]]);
    }

    #[test]
    fn test_trim_android_stacks_out_of_range_id_does_not_panic() {
        // A malformed profile may reference a frame id that is out of range.
        // Trimming must not panic; the stray reference is left in place for
        // `call_trees` to reject as an invalid frame id.
        let mut data = SampleData {
            frames: vec![
                Frame {
                    function: Some("app".to_string()),
                    ..Default::default()
                },
                Frame {
                    package: Some("apex/com.android.art/lib64/libart.so".to_string()),
                    ..Default::default()
                },
            ],
            samples: vec![],
            // id 2 and 99 are out of range; id 1 references the dropped ART frame.
            stacks: vec![vec![0, 1, 2], vec![99]],
            thread_metadata: None,
        };

        data.trim_android_stacks();

        assert_eq!(data.frames.len(), 1);
        // ART reference removed, in-range survivor kept, out-of-range ids preserved.
        assert_eq!(data.stacks, vec![vec![0, 2], vec![99]]);
    }

    #[test]
    fn test_normalize_only_trims_android() {
        // A libart.so frame is synthetic on android, but the trimming is gated
        // on the chunk platform: a non-android chunk must keep every frame.
        let make_chunk = |platform: &str| SampleChunk {
            platform: platform.to_string(),
            profile: SampleData {
                frames: vec![
                    Frame {
                        function: Some("app".to_string()),
                        ..Default::default()
                    },
                    Frame {
                        package: Some("apex/com.android.art/lib64/libart.so".to_string()),
                        ..Default::default()
                    },
                ],
                samples: vec![],
                stacks: vec![vec![0, 1]],
                thread_metadata: None,
            },
            ..Default::default()
        };

        let mut android = make_chunk("android");
        android.normalize();
        assert_eq!(android.profile.frames.len(), 1);
        assert_eq!(android.profile.stacks, vec![vec![0]]);

        let mut cocoa = make_chunk("cocoa");
        cocoa.normalize();
        assert_eq!(cocoa.profile.frames.len(), 2);
        assert_eq!(cocoa.profile.stacks, vec![vec![0, 1]]);
    }
}
