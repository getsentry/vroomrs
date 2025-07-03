use crate::nodetree::Node;
use crate::types::{CallTreesU64, ProfileInterface};
use crate::MAX_STACK_DEPTH;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// Constants
pub const FRAME_DROP: &str = "frame_drop";
const MARGIN_PERCENT: f64 = 0.05;
const MIN_FRAME_DURATION_PERCENT: f64 = 0.5;
const START_LIMIT_PERCENT: f64 = 0.2;
const UNKNOWN_FRAMES_IN_THE_STACK_THRESHOLD: f64 = 0.8;

/// Represents a node in the call stack with its depth and stack trace.
#[derive(Debug, Clone)]
pub struct NodeStack {
    pub depth: i32,
    pub n: Node,
    pub st: Vec<Node>,
}

/// Statistics for frozen frame detection.
#[derive(Debug, Clone, Default)]
pub struct FrozenFrameStats {
    pub duration_ns: u64,
    pub end_ns: u64,
    pub min_duration_ns: u64,
    pub start_limit_ns: u64,
    pub start_ns: u64,
}

impl FrozenFrameStats {
    pub fn new(end_ns: u64, duration_ns: f64) -> Self {
        let ten_millis_ns = Duration::from_millis(10).as_nanos() as f64;
        let margin = (duration_ns * MARGIN_PERCENT).max(ten_millis_ns) as u64;

        let mut stats = FrozenFrameStats {
            end_ns: end_ns + margin,
            duration_ns: duration_ns as u64,
            min_duration_ns: (duration_ns * MIN_FRAME_DURATION_PERCENT) as u64,
            ..Default::default()
        };

        if end_ns >= (stats.duration_ns + margin) {
            stats.start_ns = end_ns - stats.duration_ns - margin;
        }

        stats.start_limit_ns = stats.start_ns + (duration_ns * START_LIMIT_PERCENT) as u64;

        stats
    }

    /// Determines if a node stack is valid for frozen frame detection.
    pub fn is_node_stack_valid(&self, ns: &NodeStack) -> bool {
        // Check if function name exists and is not empty
        let has_function = ns.n.frame.function.as_ref().is_some_and(|f| !f.is_empty());

        has_function
            && ns.n.is_application
            && ns.n.start_ns >= self.start_ns
            && ns.n.end_ns <= self.end_ns
            && ns.n.duration_ns >= self.min_duration_ns
            && ns.n.start_ns <= self.start_limit_ns
    }

    /// Finds the frame drop cause frame by traversing the node tree.
    ///
    /// This function recursively explores the node tree to find the deepest valid node
    /// that could be the cause of a frame drop. It prioritizes nodes with longer duration
    /// and greater depth.
    pub fn find_frame_drop_cause_frame(
        &self,
        n: &Rc<RefCell<Node>>,
        st: &mut Vec<Rc<RefCell<Node>>>,
        depth: i32,
    ) -> Option<NodeStack> {
        // Add current node to stack trace
        st.push(n.clone());

        let mut longest: Option<NodeStack> = None;

        // Explore each branch to find the deepest valid node
        for child in &n.borrow().children {
            if let Some(cause) = self.find_frame_drop_cause_frame(child, st, depth + 1) {
                match &longest {
                    Some(longest_ref) => {
                        // Only keep the longest node
                        if cause.n.duration_ns > longest_ref.n.duration_ns
                            || (cause.n.duration_ns == longest_ref.n.duration_ns
                                && cause.depth > longest_ref.depth)
                        {
                            longest = Some(cause);
                        }
                    }
                    None => {
                        longest = Some(cause);
                    }
                }
            }
        }

        // Create a nodeStack of the current node
        let ns = NodeStack {
            depth,
            n: n.borrow().clone(), // Clone the node data
            st: Vec::new(),        // Will be filled later if needed
        };

        // Check if current node is valid
        let current = if self.is_node_stack_valid(&ns) {
            Some(ns)
        } else {
            None
        };

        let result = match (longest, current) {
            (None, None) => None,
            (None, Some(mut current)) => {
                // If we didn't find any valid node downstream, we return the current
                // Copy the stack trace
                current.st = st.iter().map(|node| node.borrow().clone()).collect();
                Some(current)
            }
            (Some(longest), None) => Some(longest),
            (Some(longest), Some(mut current)) => {
                // If current is not valid or a node downstream is equal or longer, we return it
                // We give priority to the child instead of the current node
                if longest.n.duration_ns >= current.n.duration_ns {
                    Some(longest)
                } else {
                    // Copy the stack trace
                    current.st = st.iter().map(|node| node.borrow().clone()).collect();
                    Some(current)
                }
            }
        };

        st.pop();
        result
    }
}

/// Finds frame drop causes in the profile based on frozen frame measurements.
///
/// This function looks for "frozen_frame_renders" measurements in the profile
/// and analyzes call trees to identify potential causes of frame drops.
pub fn find_frame_drop_cause(
    profile: &dyn ProfileInterface,
    call_trees_per_thread_id: &CallTreesU64,
    occurrences: &mut Vec<super::Occurrence>,
) {
    // Get frozen frame measurements
    let Some(measurements) = profile.get_measurements() else {
        return;
    };

    let Some(frame_drops) = measurements.get("frozen_frame_renders") else {
        return;
    };

    // Get call trees for the active thread
    let active_thread_id = profile.get_transaction().active_thread_id;
    let Some(call_trees) = call_trees_per_thread_id.get(&active_thread_id) else {
        return;
    };

    // Process each measurement value
    for mv in &frame_drops.values {
        let stats = FrozenFrameStats::new(mv.elapsed_since_start_ns, mv.value);

        // Check each root in call trees
        for root in call_trees {
            let mut st = Vec::with_capacity(MAX_STACK_DEPTH as usize);
            if let Some(cause) = stats.find_frame_drop_cause_frame(root, &mut st, 0) {
                // We found a potential stacktrace responsible for this frozen frame
                let mut stack_trace = Vec::with_capacity(cause.st.len());
                let mut unknown_frames_count = 0.0;

                for frame_node in &cause.st {
                    if frame_node
                        .frame
                        .function
                        .as_ref()
                        .is_none_or(|f| f.is_empty())
                    {
                        unknown_frames_count += 1.0;
                    }
                    stack_trace.push(frame_node.to_frame());
                }

                // If there are too many unknown frames in the stack,
                // we do not create an occurrence.
                let unknown_threshold =
                    stack_trace.len() as f64 * UNKNOWN_FRAMES_IN_THE_STACK_THRESHOLD;
                if unknown_frames_count >= unknown_threshold {
                    continue;
                }

                // Create NodeInfo for the found cause
                let node_info = super::NodeInfo {
                    category: FRAME_DROP.to_string(),
                    node: cause.n,
                    stack_trace,
                };

                // Create new occurrence and add it to the occurrences vector
                let occurrence = super::new_occurrence(profile, node_info);
                occurrences.push(occurrence);
                break; // Found a cause for this measurement, move to next one
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        frame::Frame,
        occurrence::{Event, Evidence, EvidenceData, Occurrence, StackTrace, ISSUE_TITLES},
    };

    use chrono::DateTime;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_is_node_stack_valid() {
        use super::*;

        struct TestCase {
            name: String,
            stats: FrozenFrameStats,
            nodestack: NodeStack,
            valid: bool,
        }

        let tests = vec![TestCase {
            name: "frame too short".to_string(),
            stats: FrozenFrameStats {
                start_limit_ns: 0,
                duration_ns: 1000,
                min_duration_ns: 500,
                start_ns: 0,
                end_ns: 1000,
            },
            nodestack: NodeStack {
                depth: 0,
                n: Node {
                    start_ns: 100,
                    end_ns: 200,
                    is_application: true,
                    duration_ns: 100,
                    frame: Frame {
                        function: Some("test_function".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                st: Vec::new(),
            },
            valid: false,
        }];

        for tt in tests {
            let output = tt.stats.is_node_stack_valid(&tt.nodestack);
            assert_eq!(
                output, tt.valid,
                "Test '{}': expected {}, got {}",
                tt.name, tt.valid, output
            );
        }
    }

    #[test]
    fn test_find_frame_drop_cause() {
        use super::*;
        use crate::sample::v1::Sample;
        use crate::sample::v1::{
            Measurement, MeasurementValue, Profile as SampleProfileData, SampleProfile,
        };
        use crate::types::{Platform, ProfileInterface, Transaction};

        struct TestCase {
            name: String,
            call_trees: CallTreesU64,
            profile: Box<dyn ProfileInterface>,
            want: Vec<Occurrence>,
        }

        let tests = vec![
            TestCase {
                name: "Find a basic cause of a frame drop".to_string(),
                profile: Box::new(SampleProfile {
                    event_id: "1234567890".to_string(),
                    measurements: Some(HashMap::from_iter([(
                        "frozen_frame_renders".to_string(),
                        Measurement {
                            unit: "nanosecond".to_string(),
                            values: vec![MeasurementValue {
                                elapsed_since_start_ns: 400_000_000, // 400ms
                                value: 200_000_000.0,                // 200ms
                            }],
                        },
                    )])),
                    transaction: Transaction {
                        active_thread_id: 1,
                        id: "1234".to_string(),
                        name: "some".to_string(),
                        ..Default::default()
                    },
                    platform: Platform::Cocoa,
                    profile: SampleProfileData {
                        frames: vec![],
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 0,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 500_000_000,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![]],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                call_trees: HashMap::from_iter([
                    (
                        1_u64,
                        Vec::from_iter([Rc::new(RefCell::new(Node {
                            duration_ns: 500_000_000, // 500ms
                            end_ns: 500_000_000,
                            is_application: false,
                            name: "root".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            frame: Frame {
                                function: Some("root".to_string()),
                                in_app: Some(false),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 200_000_000, // 200ms
                                    end_ns: 200_000_000,
                                    is_application: true,
                                    name: "child1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child1".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                })),
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 100_000_000, // 100ms
                                    end_ns: 300_000_000,
                                    start_ns: 200_000_000,
                                    is_application: true,
                                    name: "child2".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child2".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    children: vec![Rc::new(RefCell::new(Node {
                                        duration_ns: 50_000_000, // 50ms
                                        end_ns: 250_000_000,
                                        start_ns: 200_000_000,
                                        is_application: true,
                                        name: "child2-1".to_string(),
                                        package: "package".to_string(),
                                        path: Some("path".to_string()),
                                        frame: Frame {
                                            function: Some("child2-1".to_string()),
                                            in_app: Some(false),
                                            package: Some("package".to_string()),
                                            path: Some("path".to_string()),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }))],
                                    ..Default::default()
                                })),
                            ],
                            ..Default::default()
                        }))]),
                    ), // end call_tree for tid 1
                ]),
                want: vec![Occurrence {
                    culprit: "some".to_string(),
                    issue_title: ISSUE_TITLES[FRAME_DROP].issue_title.to_string(),
                    level: "info".to_string(),
                    payload_type: "occurrence".to_string(),
                    subtitle: "child2".to_string(),
                    r#type: ISSUE_TITLES[FRAME_DROP].r#type,
                    category: FRAME_DROP.to_string(),
                    duration_ns: 100000000,
                    event: Event {
                        platform: "cocoa".to_string(),
                        stacktrace: StackTrace {
                            frames: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2".to_string()),
                                    in_app: Some(true),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        },
                        ..Default::default()
                    },
                    evidence_data: EvidenceData {
                        frame_duration_ns: 100000000,
                        frame_module: "".to_string(),
                        frame_name: "child2".to_string(),
                        frame_package: "package".to_string(),
                        profile_id: "1234567890".to_string(),
                        template_name: "profile".to_string(),
                        transaction_id: "1234".to_string(),
                        transaction_name: "some".to_string(),
                        profile_duration_ns: 500000000,
                        ..Default::default()
                    },
                    evidence_display: vec![
                        Evidence {
                            name: "Suspect function".to_string(),
                            value: "child2".to_string(),
                            important: true,
                        },
                        Evidence {
                            name: "Package".to_string(),
                            value: "package".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }], // Expect one occurrence
            },
            TestCase {
                name: "Find a deeper frame than expected".to_string(),
                profile: Box::new(SampleProfile {
                    event_id: "1234567890".to_string(),
                    measurements: Some(HashMap::from_iter([(
                        "frozen_frame_renders".to_string(),
                        Measurement {
                            unit: "nanosecond".to_string(),
                            values: vec![MeasurementValue {
                                elapsed_since_start_ns: 400_000_000, // 400ms
                                value: 200_000_000.0,                // 200ms
                            }],
                        },
                    )])),
                    transaction: Transaction {
                        active_thread_id: 1,
                        id: "1234".to_string(),
                        name: "some".to_string(),
                        ..Default::default()
                    },
                    platform: Platform::Cocoa,
                    profile: SampleProfileData {
                        frames: vec![],
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 0,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 500_000_000,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![]],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                call_trees: HashMap::from_iter([
                    (
                        1_u64,
                        Vec::from_iter([Rc::new(RefCell::new(Node {
                            duration_ns: 500_000_000, // 500ms
                            end_ns: 500_000_000,
                            is_application: false,
                            name: "root".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            frame: Frame {
                                function: Some("root".to_string()),
                                in_app: Some(false),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 20_000_000, // 20ms
                                    end_ns: 20_000_000,
                                    is_application: true,
                                    name: "child1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child1-1".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                })),
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 100_000_000, // 100ms
                                    end_ns: 300_000_000,
                                    start_ns: 200_000_000,
                                    is_application: true,
                                    name: "child2".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child2".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    children: vec![Rc::new(RefCell::new(Node {
                                        duration_ns: 100_000_000, // 100ms
                                        end_ns: 300_000_000,
                                        start_ns: 200_000_000,
                                        is_application: false,
                                        name: "child2-1".to_string(),
                                        package: "package".to_string(),
                                        path: Some("path".to_string()),
                                        frame: Frame {
                                            function: Some("child2-1".to_string()),
                                            in_app: Some(false),
                                            package: Some("package".to_string()),
                                            path: Some("path".to_string()),
                                            ..Default::default()
                                        },
                                        children: vec![Rc::new(RefCell::new(Node {
                                            duration_ns: 100_000_000, // 100ms
                                            end_ns: 300_000_000,
                                            start_ns: 200_000_000,
                                            is_application: true,
                                            name: "child2-1-1".to_string(),
                                            package: "package".to_string(),
                                            path: Some("path".to_string()),
                                            frame: Frame {
                                                function: Some("child2-1-1".to_string()),
                                                in_app: Some(true),
                                                package: Some("package".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                            children: vec![Rc::new(RefCell::new(Node {
                                                duration_ns: 100_000_000, // 100ms
                                                end_ns: 300_000_000,
                                                start_ns: 200_000_000,
                                                is_application: false,
                                                name: "child2-1-1-1".to_string(),
                                                package: "package".to_string(),
                                                path: Some("path".to_string()),
                                                frame: Frame {
                                                    function: Some("child2-1-1-1".to_string()),
                                                    in_app: Some(false),
                                                    package: Some("package".to_string()),
                                                    path: Some("path".to_string()),
                                                    ..Default::default()
                                                },
                                                children: vec![Rc::new(RefCell::new(Node {
                                                    duration_ns: 100_000_000, // 100ms
                                                    end_ns: 300_000_000,
                                                    start_ns: 200_000_000,
                                                    is_application: false,
                                                    name: "child2-1-1-1-1".to_string(),
                                                    package: "package".to_string(),
                                                    path: Some("path".to_string()),
                                                    frame: Frame {
                                                        function: Some(
                                                            "child2-1-1-1-1".to_string(),
                                                        ),
                                                        in_app: Some(false),
                                                        package: Some("package".to_string()),
                                                        path: Some("path".to_string()),
                                                        ..Default::default()
                                                    },
                                                    ..Default::default()
                                                }))],
                                                ..Default::default()
                                            }))],
                                            ..Default::default()
                                        }))],
                                        ..Default::default()
                                    }))],
                                    ..Default::default()
                                })),
                            ],
                            ..Default::default()
                        }))]),
                    ), // end call_tree for tid 1
                ]),
                want: vec![Occurrence {
                    culprit: "some".to_string(),
                    issue_title: ISSUE_TITLES[FRAME_DROP].issue_title.to_string(),
                    level: "info".to_string(),
                    payload_type: "occurrence".to_string(),
                    subtitle: "child2-1-1".to_string(),
                    r#type: ISSUE_TITLES[FRAME_DROP].r#type,
                    category: FRAME_DROP.to_string(),
                    duration_ns: 100000000,
                    event: Event {
                        platform: "cocoa".to_string(),
                        stacktrace: StackTrace {
                            frames: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2".to_string()),
                                    in_app: Some(true),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2-1".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2-1-1".to_string()),
                                    in_app: Some(true),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        },
                        ..Default::default()
                    },
                    evidence_data: EvidenceData {
                        frame_duration_ns: 100000000,
                        frame_module: "".to_string(),
                        frame_name: "child2-1-1".to_string(),
                        frame_package: "package".to_string(),
                        profile_id: "1234567890".to_string(),
                        template_name: "profile".to_string(),
                        transaction_id: "1234".to_string(),
                        transaction_name: "some".to_string(),
                        profile_duration_ns: 500000000,
                        ..Default::default()
                    },
                    evidence_display: vec![
                        Evidence {
                            name: "Suspect function".to_string(),
                            value: "child2-1-1".to_string(),
                            important: true,
                        },
                        Evidence {
                            name: "Package".to_string(),
                            value: "package".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }], // Expect one occurrence
            },
            TestCase {
                name: "Find a deeper and longer frame in shorter parent system frame".to_string(),
                profile: Box::new(SampleProfile {
                    event_id: "1234567890".to_string(),
                    measurements: Some(HashMap::from_iter([(
                        "frozen_frame_renders".to_string(),
                        Measurement {
                            unit: "nanosecond".to_string(),
                            values: vec![MeasurementValue {
                                elapsed_since_start_ns: 400_000_000, // 400ms
                                value: 300_000_000.0,                // 300ms
                            }],
                        },
                    )])),
                    transaction: Transaction {
                        active_thread_id: 1,
                        id: "1234".to_string(),
                        name: "some".to_string(),
                        ..Default::default()
                    },
                    platform: Platform::Cocoa,
                    profile: SampleProfileData {
                        frames: vec![],
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 0,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 500_000_000,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![]],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                call_trees: HashMap::from_iter([
                    (
                        1_u64,
                        Vec::from_iter([Rc::new(RefCell::new(Node {
                            duration_ns: 500_000_000, // 500ms
                            end_ns: 500_000_000,
                            is_application: false,
                            name: "root".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            frame: Frame {
                                function: Some("root".to_string()),
                                in_app: Some(false),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 50_000_000, // 50ms
                                    end_ns: 50_000_000,
                                    is_application: true,
                                    name: "child1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child1".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    children: vec![Rc::new(RefCell::new(Node {
                                        duration_ns: 50_000_000, // 50ms
                                        end_ns: 50_000_000,
                                        is_application: true,
                                        name: "child1-1".to_string(),
                                        package: "package".to_string(),
                                        path: Some("path".to_string()),
                                        frame: Frame {
                                            function: Some("child1-1".to_string()),
                                            in_app: Some(true),
                                            package: Some("package".to_string()),
                                            path: Some("path".to_string()),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }))],
                                    ..Default::default()
                                })),
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 200_000_000, // 200ms
                                    end_ns: 250_000_000,
                                    start_ns: 50_000_000,
                                    is_application: false,
                                    name: "child2".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child2".to_string()),
                                        in_app: Some(false),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    children: vec![Rc::new(RefCell::new(Node {
                                        duration_ns: 150_000_000, // 150ms
                                        end_ns: 250_000_000,
                                        start_ns: 100_000_000,
                                        is_application: true,
                                        name: "child2-1".to_string(),
                                        package: "package".to_string(),
                                        path: Some("path".to_string()),
                                        frame: Frame {
                                            function: Some("child2-1".to_string()),
                                            in_app: Some(true),
                                            package: Some("package".to_string()),
                                            path: Some("path".to_string()),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }))],
                                    ..Default::default()
                                })),
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 250_000_000, // 250ms
                                    end_ns: 500_000_000,
                                    start_ns: 250_000_000,
                                    is_application: false,
                                    name: "child3".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child3".to_string()),
                                        in_app: Some(false),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    children: vec![Rc::new(RefCell::new(Node {
                                        duration_ns: 50_000_000, // 50ms
                                        end_ns: 300_000_000,
                                        start_ns: 250_000_000,
                                        is_application: true,
                                        name: "child3-1".to_string(),
                                        package: "package".to_string(),
                                        path: Some("path".to_string()),
                                        frame: Frame {
                                            function: Some("child3-1".to_string()),
                                            in_app: Some(true),
                                            package: Some("package".to_string()),
                                            path: Some("path".to_string()),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }))],
                                    ..Default::default()
                                })),
                            ],
                            ..Default::default()
                        }))]),
                    ), // end call_tree for tid 1
                ]),
                want: vec![Occurrence {
                    culprit: "some".to_string(),
                    issue_title: ISSUE_TITLES[FRAME_DROP].issue_title.to_string(),
                    level: "info".to_string(),
                    payload_type: "occurrence".to_string(),
                    subtitle: "child2-1".to_string(),
                    r#type: ISSUE_TITLES[FRAME_DROP].r#type,
                    category: FRAME_DROP.to_string(),
                    duration_ns: 150000000,
                    event: Event {
                        platform: "cocoa".to_string(),
                        stacktrace: StackTrace {
                            frames: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2-1".to_string()),
                                    in_app: Some(true),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        },
                        ..Default::default()
                    },
                    evidence_data: EvidenceData {
                        frame_duration_ns: 150000000,
                        frame_module: "".to_string(),
                        frame_name: "child2-1".to_string(),
                        frame_package: "package".to_string(),
                        profile_id: "1234567890".to_string(),
                        template_name: "profile".to_string(),
                        transaction_id: "1234".to_string(),
                        transaction_name: "some".to_string(),
                        profile_duration_ns: 500000000,
                        ..Default::default()
                    },
                    evidence_display: vec![
                        Evidence {
                            name: "Suspect function".to_string(),
                            value: "child2-1".to_string(),
                            important: true,
                        },
                        Evidence {
                            name: "Package".to_string(),
                            value: "package".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }], // Expect one occurrence
            },
            TestCase {
                name: "Make sure we're biased towards earlier frames".to_string(),
                profile: Box::new(SampleProfile {
                    event_id: "1234567890".to_string(),
                    measurements: Some(HashMap::from_iter([(
                        "frozen_frame_renders".to_string(),
                        Measurement {
                            unit: "nanosecond".to_string(),
                            values: vec![MeasurementValue {
                                elapsed_since_start_ns: 500_000_000, // 500ms
                                value: 500_000_000.0,                // 500ms
                            }],
                        },
                    )])),
                    transaction: Transaction {
                        active_thread_id: 1,
                        id: "1234".to_string(),
                        name: "some".to_string(),
                        ..Default::default()
                    },
                    platform: Platform::Cocoa,
                    profile: SampleProfileData {
                        frames: vec![],
                        samples: vec![
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 0,
                                ..Default::default()
                            },
                            Sample {
                                stack_id: 0,
                                thread_id: 1,
                                elapsed_since_start_ns: 500_000_000,
                                ..Default::default()
                            },
                        ],
                        stacks: vec![vec![]],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                call_trees: HashMap::from_iter([
                    (
                        1_u64,
                        Vec::from_iter([Rc::new(RefCell::new(Node {
                            duration_ns: 1_000_000_000, // 1000ms
                            end_ns: 1_000_000_000,
                            is_application: false,
                            name: "root".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            frame: Frame {
                                function: Some("root".to_string()),
                                in_app: Some(false),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 50_000_000, // 50ms
                                    end_ns: 50_000_000,
                                    is_application: true,
                                    name: "child1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child1".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                })),
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 200_000_000, // 200ms
                                    end_ns: 300_000_000,
                                    start_ns: 100_000_000,
                                    is_application: false,
                                    name: "child2".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child2".to_string()),
                                        in_app: Some(false),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    children: vec![Rc::new(RefCell::new(Node {
                                        duration_ns: 250_000_000, // 250ms
                                        end_ns: 350_000_000,
                                        start_ns: 100_000_000,
                                        is_application: true,
                                        name: "child2-1".to_string(),
                                        package: "package".to_string(),
                                        path: Some("path".to_string()),
                                        frame: Frame {
                                            function: Some("child2-1".to_string()),
                                            in_app: Some(true),
                                            package: Some("package".to_string()),
                                            path: Some("path".to_string()),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }))],
                                    ..Default::default()
                                })),
                                Rc::new(RefCell::new(Node {
                                    duration_ns: 250_000_000, // 250ms
                                    end_ns: 500_000_000,
                                    start_ns: 250_000_000,
                                    is_application: true,
                                    name: "child3".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    frame: Frame {
                                        function: Some("child3".to_string()),
                                        in_app: Some(true),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                })),
                            ],
                            ..Default::default()
                        }))]),
                    ), // end call_tree for tid 1
                ]),
                want: vec![Occurrence {
                    culprit: "some".to_string(),
                    issue_title: ISSUE_TITLES[FRAME_DROP].issue_title.to_string(),
                    level: "info".to_string(),
                    payload_type: "occurrence".to_string(),
                    subtitle: "child2-1".to_string(),
                    r#type: ISSUE_TITLES[FRAME_DROP].r#type,
                    category: FRAME_DROP.to_string(),
                    duration_ns: 250000000,
                    event: Event {
                        platform: "cocoa".to_string(),
                        stacktrace: StackTrace {
                            frames: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2".to_string()),
                                    in_app: Some(false),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2-1".to_string()),
                                    in_app: Some(true),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        },
                        ..Default::default()
                    },
                    evidence_data: EvidenceData {
                        frame_duration_ns: 250000000,
                        frame_module: "".to_string(),
                        frame_name: "child2-1".to_string(),
                        frame_package: "package".to_string(),
                        profile_id: "1234567890".to_string(),
                        template_name: "profile".to_string(),
                        transaction_id: "1234".to_string(),
                        transaction_name: "some".to_string(),
                        profile_duration_ns: 500000000,
                        ..Default::default()
                    },
                    evidence_display: vec![
                        Evidence {
                            name: "Suspect function".to_string(),
                            value: "child2-1".to_string(),
                            important: true,
                        },
                        Evidence {
                            name: "Package".to_string(),
                            value: "package".to_string(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }], // Expect one occurrence
            },
        ];

        for tt in tests {
            let mut occurrences = Vec::new();
            find_frame_drop_cause(tt.profile.as_ref(), &tt.call_trees, &mut occurrences);
            for occurrence in &mut occurrences {
                occurrence.event.event_id = "".to_string();
                occurrence.detection_time = DateTime::default();
                occurrence.id = "".to_string();
                occurrence.fingerprint = Vec::new();
            }
            assert_eq!(
                occurrences, tt.want,
                "Test '{:?}': expected {:?} occurrences, got {:?}",
                tt.name, tt.want, occurrences
            );
        }
    }
}
