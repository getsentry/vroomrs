use fnv_rs::Fnv64;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hasher;

use super::SampleError;
use crate::frame::Frame;
use crate::nodetree::Node;
use crate::types::{ClientSDK, DebugMeta, Platform};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SampleChunk {
    #[serde(rename = "chunk_id")]
    pub id: String,

    pub profiler_id: String,

    #[serde(default, skip_serializing_if = "DebugMeta::is_empty")]
    pub debug_meta: DebugMeta,

    pub client_sdk: Option<ClientSDK>,

    pub environment: Option<String>,

    pub platform: Platform,

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

#[derive(Serialize, Deserialize, Debug)]
pub struct ThreadMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SampleData {
    pub frames: Vec<Frame>,
    pub samples: Vec<Sample>,
    pub stacks: Vec<Vec<i32>>,
    pub thread_metadata: std::collections::HashMap<String, ThreadMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sample {
    #[serde(rename = "stack_id")]
    pub stack_id: i32,
    #[serde(rename = "thread_id")]
    pub thread_id: String,
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
}

impl SampleChunk {
    pub fn call_trees(
        &mut self,
        active_thread_id: Option<&str>,
    ) -> Result<HashMap<&str, Vec<Node>>, SampleError> {
        // Sort samples by timestamp
        self.profile
            .samples
            .sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

        let mut trees_by_thread_id: HashMap<&str, Vec<Node>> = HashMap::new();
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
                    return Err(SampleError::InvalidStackId);
                }

                let stack = &self.profile.stacks[sample.stack_id as usize];

                // Validate frame IDs
                for &frame_id in stack.iter().rev() {
                    if self.profile.frames.len() <= (frame_id as usize) {
                        return Err(SampleError::InvalidFrameId);
                    }
                }

                // Here while we save the nextTimestamp val, we convert it to nanosecond
                // since the Node struct and utilities use uint64 ns values
                let next_timestamp = (&samples[sample_index + 1].timestamp * 1e9) as u64;
                let sample_timestamp = (sample.timestamp * 1e9) as u64;

                let mut current: Option<&mut Node> = None;

                // Process stack frames from bottom to top
                for &frame_id in stack.iter().rev() {
                    let frame = &self.profile.frames[frame_id as usize];

                    // Calculate fingerprint
                    frame.write_to_hash(&mut hasher);
                    let fingerprint = hasher.finish();

                    match current {
                        None => {
                            let trees = trees_by_thread_id.entry(thread_id).or_default();

                            if let Some(last_tree) = trees.last_mut() {
                                if last_tree.fingerprint == fingerprint
                                    && last_tree.end_ns == sample_timestamp
                                {
                                    last_tree.update(next_timestamp);
                                    current = Some(last_tree);
                                    continue;
                                }
                            }

                            let new_node = Node::from_frame(
                                frame,
                                sample_timestamp,
                                next_timestamp,
                                fingerprint,
                            );
                            trees.push(new_node);
                            current = trees.last_mut();
                        }
                        Some(node) => {
                            let i = node.children.len();
                            if !node.children.is_empty()
                                && node.children[i - 1].fingerprint == fingerprint
                                && node.children[i - 1].end_ns == sample_timestamp
                            {
                                let last_child = &mut node.children[i - 1];
                                last_child.update(next_timestamp);
                                current = Some(last_child);
                                continue;
                            } else {
                                let new_node = Node::from_frame(
                                    frame,
                                    sample_timestamp,
                                    next_timestamp,
                                    fingerprint,
                                );
                                node.children.push(new_node);
                                current = node.children.last_mut();
                            }
                        } // end Some
                    } // end match
                } // end stack loop
                hasher = Fnv64::default();
            }
        }
        Ok(trees_by_thread_id)
    }
}

#[cfg(test)]
mod tests {
    use serde_path_to_error::Error;

    use crate::{
        frame::Frame,
        sample::v2::{Sample, SampleData},
    };

    use super::SampleChunk;

    #[test]
    fn test_sample_format_v2_cocoa() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_cocoa.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }

    #[test]
    fn test_sample_format_v2_python() {
        let payload = include_bytes!("../../tests/fixtures/sample/v2/valid_python.json");
        let d = &mut serde_json::Deserializer::from_slice(payload);
        let r: Result<SampleChunk, Error<_>> = serde_path_to_error::deserialize(d);
        assert!(r.is_ok(), "{:#?}", r)
    }

    #[test]
    fn test_call_trees() {
        use crate::nodetree::Node;
        use std::collections::HashMap;
        struct TestStruct<'a> {
            name: String,
            chunk: SampleChunk,
            want: HashMap<&'a str, Vec<Node>>,
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
                    "1",
                    vec![Node {
                        duration_ns: 40000000,
                        end_ns: 50000000,
                        fingerprint: 6903369137866438128,
                        is_application: true,
                        name: "function0".to_string(),
                        sample_count: 2,
                        start_ns: 10000000,
                        frame: Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        children: vec![
                            Node {
                                duration_ns: 40000000,
                                end_ns: 50000000,
                                start_ns: 10000000,
                                fingerprint: 17095743776245828002,
                                is_application: true,
                                name: "function1".to_string(),
                                sample_count: 2,
                                frame: Frame {
                                    function: Some("function1".to_string()),
                                    ..Default::default()
                                },
                                children: vec![Node {
                                    duration_ns: 10000000,
                                    end_ns: 50000000,
                                    fingerprint: 16529420490907277225,
                                    is_application: true,
                                    name: "function2".to_string(),
                                    sample_count: 1,
                                    start_ns: 40000000,
                                    frame: Frame {
                                        function: Some("function2".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }, // TODO finish
                        ],
                        ..Default::default()
                    }],
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
                    "1",
                    vec![Node {
                        duration_ns: 30000000,
                        end_ns: 40000000,
                        fingerprint: 6903369137866438128,
                        is_application: true,
                        name: "function0".to_string(),
                        sample_count: 1,
                        start_ns: 10000000,
                        frame: Frame {
                            function: Some("function0".to_string()),
                            ..Default::default()
                        },
                        children: vec![Node {
                            duration_ns: 30000000,
                            end_ns: 40000000,
                            fingerprint: 17095743776245828002,
                            is_application: true,
                            name: "function1".to_string(),
                            sample_count: 1,
                            start_ns: 10000000,
                            frame: Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
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
                    "1",
                    vec![
                        Node {
                            duration_ns: 10000000,
                            end_ns: 20000000,
                            fingerprint: 6903369137866438128,
                            is_application: true,
                            name: "function0".to_string(),
                            sample_count: 1,
                            start_ns: 10000000,
                            frame: Frame {
                                function: Some("function0".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        Node {
                            duration_ns: 10000000,
                            end_ns: 30000000,
                            fingerprint: 6903370237378066339,
                            is_application: true,
                            name: "function1".to_string(),
                            sample_count: 1,
                            start_ns: 20000000,
                            frame: Frame {
                                function: Some("function1".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
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
}
