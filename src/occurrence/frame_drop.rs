use crate::nodetree::Node;
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
