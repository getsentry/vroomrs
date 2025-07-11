use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use pyo3::{pyclass, pymethods};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    android, frame,
    types::{CallTreesU64, DebugMeta, Platform, ProfileInterface},
};

mod detect_frame;
mod frame_drop;

// Import category constants from detect_frame module
use detect_frame::{
    NodeInfo, BASE64_DECODE, BASE64_ENCODE, COMPRESSION, CORE_DATA_BLOCK, CORE_DATA_MERGE,
    CORE_DATA_READ, CORE_DATA_WRITE, DECOMPRESSION, DETECT_FRAME_JOBS, FILE_READ, FILE_WRITE, HTTP,
    IMAGE_DECODE, IMAGE_ENCODE, JSON_DECODE, JSON_ENCODE, ML_MODEL_INFERENCE, ML_MODEL_LOAD, REGEX,
    SOURCE_CONTEXT, SQL, THREAD_WAIT, VIEW_INFLATION, VIEW_LAYOUT, VIEW_RENDER, VIEW_UPDATE, XPC,
};

// Import frame drop detection function
use frame_drop::find_frame_drop_cause;

// Type constants: DO NOT REMOVE COMMENTED TYPES!
pub const NONE_TYPE: u64 = 0;
pub const CORE_DATA_TYPE: u64 = 2004;
//pub const FILE_IO_TYPE: u64 = 2001;
pub const IMAGE_DECODE_TYPE: u64 = 2002;
pub const JSON_DECODE_TYPE: u64 = 2003;
pub const REGEX_TYPE: u64 = 2007;
pub const VIEW_TYPE: u64 = 2006;
pub const FRAME_DROP_TYPE: u64 = 2009;
//pub const FRAME_REGRESSION_EXP_TYPE: u64 = 2010;
//pub const FRAME_REGRESSION_TYPE: u64 = 2011;

// Evidence name constants
pub const EVIDENCE_NAME_DURATION: &str = "Duration";
pub const EVIDENCE_NAME_FUNCTION: &str = "Suspect function";
pub const EVIDENCE_NAME_PACKAGE: &str = "Package";
//pub const EVIDENCE_FULLY_QUALIFIED_NAME: &str = "Fully qualified name";
//pub const EVIDENCE_BREAKPOINT: &str = "Breakpoint";
//pub const EVIDENCE_REGRESSION: &str = "Regression";

// Other constants
pub const OCCURRENCE_PAYLOAD: &str = "occurrence";

// FRAME_DROP constant (not defined in detect_frame.rs)
const FRAME_DROP: &str = "frame_drop";

#[derive(Debug, Serialize, Clone, Default, PartialEq)]
pub struct StackTrace {
    pub frames: Vec<frame::Frame>,
}

#[pyclass]
#[derive(Debug, Serialize, Clone, Default, PartialEq)]
pub struct Evidence {
    pub name: String,
    pub value: String,
    pub important: bool,
}

#[pymethods]
impl Evidence {
    /// Returns the evidence name.
    ///
    /// Returns:
    ///     str
    ///         The name of the evidence (e.g., "Duration", "Suspect function", "Package").
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Returns the evidence value.
    ///
    /// Returns:
    ///     str
    ///         The value of the evidence.
    pub fn get_value(&self) -> &str {
        &self.value
    }

    /// Returns whether the evidence is important.
    ///
    /// Returns:
    ///     bool
    ///         True if the evidence is marked as important, False otherwise.
    pub fn get_important(&self) -> bool {
        self.important
    }
}

#[pyclass]
#[derive(Debug, Serialize, PartialEq, Default, Clone)]
pub struct EvidenceData {
    frame_duration_ns: u64,
    frame_module: String,
    frame_name: String,
    frame_package: String,
    profile_duration_ns: u64,
    template_name: String,
    transaction_id: String,
    transaction_name: String,
    profile_id: String,
    sample_count: Option<u64>,
}

#[pymethods]
impl EvidenceData {
    /// Returns the frame duration in nanoseconds.
    ///
    /// Returns:
    ///     int
    ///         Duration of the frame in nanoseconds.
    pub fn get_frame_duration_ns(&self) -> u64 {
        self.frame_duration_ns
    }

    /// Returns the frame module.
    ///
    /// Returns:
    ///     str
    ///         Module name where the frame is located.
    pub fn get_frame_module(&self) -> &str {
        &self.frame_module
    }

    /// Returns the frame name.
    ///
    /// Returns:
    ///     str
    ///         Name of the frame/function.
    pub fn get_frame_name(&self) -> &str {
        &self.frame_name
    }

    /// Returns the frame package.
    ///
    /// Returns:
    ///     str
    ///         Package name where the frame is located.
    pub fn get_frame_package(&self) -> &str {
        &self.frame_package
    }

    /// Returns the profile duration in nanoseconds.
    ///
    /// Returns:
    ///     int
    ///         Total duration of the profile in nanoseconds.
    pub fn get_profile_duration_ns(&self) -> u64 {
        self.profile_duration_ns
    }

    /// Returns the template name.
    ///
    /// Returns:
    ///     str
    ///         Name of the template used.
    pub fn get_template_name(&self) -> &str {
        &self.template_name
    }

    /// Returns the transaction ID.
    ///
    /// Returns:
    ///     str
    ///         ID of the transaction.
    pub fn get_transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the transaction name.
    ///
    /// Returns:
    ///     str
    ///         Name of the transaction.
    pub fn get_transaction_name(&self) -> &str {
        &self.transaction_name
    }

    /// Returns the profile ID.
    ///
    /// Returns:
    ///     str
    ///         ID of the profile.
    pub fn get_profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Returns the sample count.
    ///
    /// Returns:
    ///     int
    ///         Number of samples, or None if not available.
    pub fn get_sample_count(&self) -> Option<u64> {
        self.sample_count
    }
}

#[pyclass]
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Occurrence {
    pub culprit: String,
    pub detection_time: DateTime<Utc>,
    pub event: Event,
    pub evidence_data: EvidenceData,
    pub evidence_display: Vec<Evidence>,
    pub fingerprint: Vec<String>,
    pub id: String,
    pub issue_title: String,
    pub level: String,
    pub payload_type: String,
    pub project_id: u64,
    pub resource_id: Option<String>,
    pub subtitle: String,
    pub r#type: u64,

    // Only use for stats.
    pub category: String,
    pub duration_ns: u64,
    pub sample_count: u64,
}

#[pymethods]
impl Occurrence {
    /// Returns the culprit (transaction name) where the issue occurred.
    ///
    /// Returns:
    ///     str
    ///         The name of the transaction or main operation where the issue occurred.
    pub fn get_culprit(&self) -> &str {
        &self.culprit
    }

    /// Returns the detection time as an RFC 3339 formatted string.
    ///
    /// Returns:
    ///     str
    ///         The detection time in RFC 3339 format.
    pub fn get_detection_time(&self) -> String {
        self.detection_time.to_rfc3339()
    }

    /// Returns the event data.
    ///
    /// Returns:
    ///     Event
    ///         Event data including platform, stack trace, and debug information.
    pub fn get_event(&self) -> Event {
        self.event.clone()
    }

    /// Returns the evidence data.
    ///
    /// Returns:
    ///     EvidenceData
    ///         Structured data about the performance issue.
    pub fn get_evidence_data(&self) -> EvidenceData {
        self.evidence_data.clone()
    }

    /// Returns the evidence display list.
    ///
    /// Returns:
    ///     list[Evidence]
    ///         Human-readable evidence for displaying the issue.
    pub fn get_evidence_display(&self) -> Vec<Evidence> {
        self.evidence_display.clone()
    }

    /// Returns the fingerprint list.
    ///
    /// Returns:
    ///     list[str]
    ///         Unique identifiers for grouping similar issues.
    pub fn get_fingerprint(&self) -> &Vec<String> {
        &self.fingerprint
    }

    /// Returns the occurrence ID.
    ///
    /// Returns:
    ///     str
    ///         Unique identifier for this specific occurrence.
    pub fn get_id(&self) -> &str {
        &self.id
    }

    /// Returns the issue title.
    ///
    /// Returns:
    ///     str
    ///         Human-readable title describing the type of issue.
    pub fn get_issue_title(&self) -> &str {
        &self.issue_title
    }

    /// Returns the severity level.
    ///
    /// Returns:
    ///     str
    ///         Severity level of the issue (e.g., "info", "warning", "error").
    pub fn get_level(&self) -> &str {
        &self.level
    }

    /// Returns the payload type.
    ///
    /// Returns:
    ///     str
    ///         Type of payload, typically "occurrence".
    pub fn get_payload_type(&self) -> &str {
        &self.payload_type
    }

    /// Returns the project ID.
    ///
    /// Returns:
    ///     int
    ///         ID of the project where the issue was detected.
    pub fn get_project_id(&self) -> u64 {
        self.project_id
    }

    /// Returns the resource ID.
    ///
    /// Returns:
    ///     str
    ///         Optional resource identifier, or None if not available.
    pub fn get_resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    /// Returns the subtitle.
    ///
    /// Returns:
    ///     str
    ///         Brief description, usually the function name where the issue occurred.
    pub fn get_subtitle(&self) -> &str {
        &self.subtitle
    }

    /// Returns the issue type.
    ///
    /// Returns:
    ///     int
    ///         Numeric type identifier for the issue category.
    pub fn get_type(&self) -> u64 {
        self.r#type
    }

    /// Returns the category name.
    ///
    /// Returns:
    ///     str
    ///         Category name for the performance issue.
    pub fn get_category(&self) -> &str {
        &self.category
    }

    /// Returns the duration in nanoseconds.
    ///
    /// Returns:
    ///     int
    ///         Duration of the problematic operation in nanoseconds.
    pub fn get_duration_ns(&self) -> u64 {
        self.duration_ns
    }

    /// Returns the sample count.
    ///
    /// Returns:
    ///     int
    ///         Number of samples where this issue was detected.
    pub fn get_sample_count(&self) -> u64 {
        self.sample_count
    }
}

pub struct CategoryMetadata {
    issue_title: &'static str,
    r#type: u64,
}

/// Options for detecting exact frames in profiling data.
#[pyclass]
#[derive(Debug, Serialize, Clone, Default, PartialEq)]
pub struct Event {
    pub debug_meta: DebugMeta,
    pub environment: String,
    pub event_id: String,
    pub platform: String,
    pub project_id: u64,
    pub received: DateTime<Utc>,
    pub release: Option<String>,
    pub stacktrace: StackTrace,
    pub tags: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
}

// Static lazy HashMap for issue titles
pub static ISSUE_TITLES: Lazy<HashMap<&'static str, CategoryMetadata>> = Lazy::new(|| {
    HashMap::from([
        (
            BASE64_DECODE,
            CategoryMetadata {
                issue_title: "Base64 Decode on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            BASE64_ENCODE,
            CategoryMetadata {
                issue_title: "Base64 Encode on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            COMPRESSION,
            CategoryMetadata {
                issue_title: "Compression on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            CORE_DATA_BLOCK,
            CategoryMetadata {
                issue_title: "Object Context operation on Main Thread",
                r#type: CORE_DATA_TYPE,
            },
        ),
        (
            CORE_DATA_MERGE,
            CategoryMetadata {
                issue_title: "Object Context operation on Main Thread",
                r#type: CORE_DATA_TYPE,
            },
        ),
        (
            CORE_DATA_READ,
            CategoryMetadata {
                issue_title: "Object Context operation on Main Thread",
                r#type: CORE_DATA_TYPE,
            },
        ),
        (
            CORE_DATA_WRITE,
            CategoryMetadata {
                issue_title: "Object Context operation on Main Thread",
                r#type: CORE_DATA_TYPE,
            },
        ),
        (
            DECOMPRESSION,
            CategoryMetadata {
                issue_title: "Decompression on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            FILE_READ,
            CategoryMetadata {
                issue_title: "File I/O on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            FILE_WRITE,
            CategoryMetadata {
                issue_title: "File I/O on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            FRAME_DROP,
            CategoryMetadata {
                issue_title: "Frame Drop",
                r#type: FRAME_DROP_TYPE,
            },
        ),
        (
            HTTP,
            CategoryMetadata {
                issue_title: "Network I/O on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            IMAGE_DECODE,
            CategoryMetadata {
                issue_title: "Image Decoding on Main Thread",
                r#type: IMAGE_DECODE_TYPE,
            },
        ),
        (
            IMAGE_ENCODE,
            CategoryMetadata {
                issue_title: "Image Encoding on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            JSON_DECODE,
            CategoryMetadata {
                issue_title: "JSON Decoding on Main Thread",
                r#type: JSON_DECODE_TYPE,
            },
        ),
        (
            JSON_ENCODE,
            CategoryMetadata {
                issue_title: "JSON Encoding on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            ML_MODEL_INFERENCE,
            CategoryMetadata {
                issue_title: "Machine Learning inference on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            ML_MODEL_LOAD,
            CategoryMetadata {
                issue_title: "Machine Learning model load on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            REGEX,
            CategoryMetadata {
                issue_title: "Regex on Main Thread",
                r#type: REGEX_TYPE,
            },
        ),
        (
            SQL,
            CategoryMetadata {
                issue_title: "SQL operation on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            SOURCE_CONTEXT,
            CategoryMetadata {
                issue_title: "Adding Source Context is slow",
                r#type: NONE_TYPE,
            },
        ),
        (
            THREAD_WAIT,
            CategoryMetadata {
                issue_title: "Thread Wait on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
        (
            VIEW_INFLATION,
            CategoryMetadata {
                issue_title: "SwiftUI View Inflation is slow",
                r#type: NONE_TYPE,
            },
        ),
        (
            VIEW_LAYOUT,
            CategoryMetadata {
                issue_title: "SwiftUI View Layout is slow",
                r#type: VIEW_TYPE,
            },
        ),
        (
            VIEW_RENDER,
            CategoryMetadata {
                issue_title: "SwiftUI View Render is slow",
                r#type: VIEW_TYPE,
            },
        ),
        (
            VIEW_UPDATE,
            CategoryMetadata {
                issue_title: "SwiftUI View Update is slow",
                r#type: VIEW_TYPE,
            },
        ),
        (
            XPC,
            CategoryMetadata {
                issue_title: "XPC operation on Main Thread",
                r#type: NONE_TYPE,
            },
        ),
    ])
});

/// Generates evidence data for an occurrence.
/// Converts the Go generateEvidenceData function to Rust equivalent.
pub fn generate_evidence_data(
    profile: &dyn ProfileInterface,
    node_info: &NodeInfo,
) -> EvidenceData {
    let transaction = profile.get_transaction();

    let mut evidence_data = EvidenceData {
        frame_duration_ns: node_info.node.duration_ns,
        frame_module: node_info
            .node
            .frame
            .module
            .as_deref()
            .unwrap_or("")
            .to_string(),
        frame_name: node_info.node.name.clone(),
        frame_package: node_info
            .node
            .frame
            .package
            .as_deref()
            .unwrap_or("")
            .to_string(),
        profile_duration_ns: profile.duration_ns(),
        template_name: "profile".to_string(),
        transaction_id: transaction.id.clone(),
        transaction_name: transaction.name.clone(),
        profile_id: profile.get_profile_id().to_string(),
        sample_count: None,
    };

    // Special handling based on category and platform
    match node_info.category.as_str() {
        FRAME_DROP => {}
        _ => {
            if profile.get_platform() == Platform::Android {
                evidence_data.sample_count = Some(node_info.node.sample_count);
            }
        }
    }
    evidence_data
}

/// Rounds a given Duration to the nearest multiple of another Duration.
///
/// # Arguments
///
/// * `duration` - The Duration to be rounded.
/// * `multiple` - The Duration multiple to round to.
///
/// # Returns
///
/// A new Duration rounded to the nearest multiple of `multiple`.
pub fn round_duration_to_nearest_multiple(duration: Duration, multiple: Duration) -> Duration {
    if multiple.as_nanos() == 0 {
        return duration;
    }

    // Convert to a common unit (nanoseconds)
    let duration_nanos = duration.as_nanos();
    let multiple_nanos = multiple.as_nanos();

    // Calculate the number of multiples
    // We cast to f64 for accurate floating-point division and rounding
    let num_multiples = duration_nanos as f64 / multiple_nanos as f64;

    // Round the number of multiples
    let rounded_num_multiples = num_multiples.round();

    // Convert back to Duration
    let rounded_nanos = (rounded_num_multiples * multiple_nanos as f64) as u128;

    Duration::from_nanos(rounded_nanos as u64)
}

/// Generates evidence display for an occurrence.
pub fn generate_evidence_display(
    profile: &dyn ProfileInterface,
    node_info: &NodeInfo,
) -> Vec<Evidence> {
    let mut evidence_display = vec![
        Evidence {
            important: true,
            name: EVIDENCE_NAME_FUNCTION.to_string(),
            value: node_info.node.name.clone(),
        },
        Evidence {
            important: false,
            name: EVIDENCE_NAME_PACKAGE.to_string(),
            value: node_info.node.package.clone(),
        },
    ];

    match node_info.category.as_str() {
        FRAME_DROP => {}
        _ => {
            // Calculate duration and percentage
            let profile_percentage =
                (node_info.node.duration_ns as f64 * 100.0) / profile.duration_ns() as f64;
            let duration = round_duration_to_nearest_multiple(
                Duration::from_nanos(node_info.node.duration_ns),
                Duration::from_micros(10),
            );

            let duration_str = match profile.get_platform() {
                Platform::Android => {
                    format!("{duration:?} ({profile_percentage:.2}% of the profile)")
                }
                _ => {
                    format!(
                        "{:?} ({:.2}% of the profile, found in {} samples)",
                        duration, profile_percentage, node_info.node.sample_count
                    )
                }
            };

            evidence_display.push(Evidence {
                important: false,
                name: EVIDENCE_NAME_DURATION.to_string(),
                value: duration_str,
            });
        }
    }

    evidence_display
}

/// Normalizes an Android stack trace by stripping package names from full method names.
pub fn normalize_android_stack_trace(st: &mut Vec<frame::Frame>) {
    for frame in st {
        if let (Some(function), Some(package)) = (&frame.function, &frame.package) {
            frame.function = Some(android::strip_package_name_from_full_method_name(
                function, package,
            ));
        }
    }
}

/// Generates a unique event ID.
fn event_id() -> String {
    Uuid::new_v4().to_string().replace("-", "")
}

/// Creates a new occurrence from profile data and node information.
/// This is the Rust equivalent of the Go NewOccurrence function.
pub fn new_occurrence(profile: &dyn ProfileInterface, mut ni: NodeInfo) -> Occurrence {
    let transaction = profile.get_transaction();

    // Look up issue title and type
    let (issue_type, title) = if let Some(cm) = ISSUE_TITLES.get(ni.category.as_str()) {
        (cm.r#type, cm.issue_title.to_string())
    } else {
        (NONE_TYPE, format!("{} issue detected", ni.category))
    };

    let mut platform = profile.get_platform();

    // Handle Android platform special case
    if platform == Platform::Android {
        platform = Platform::Java;
        normalize_android_stack_trace(&mut ni.stack_trace);
        ni.node.name =
            android::strip_package_name_from_full_method_name(&ni.node.name, &ni.node.package);
    }

    // Create fingerprint using MD5 hash
    let mut hasher = md5::Context::new();
    hasher.consume(profile.get_project_id().to_string().as_bytes());
    hasher.consume(title.as_bytes());
    hasher.consume(issue_type.to_string().as_bytes());
    hasher.consume(ni.node.frame.module_or_package().as_bytes());
    hasher.consume(ni.node.name.as_bytes());
    let fingerprint = format!("{:x}", hasher.compute());

    // Get transaction tags or create empty map
    let tags = profile.get_transaction_tags().clone();

    let event = Event {
        debug_meta: profile.get_debug_meta().clone(),
        environment: profile.get_environment().unwrap_or("").to_string(),
        event_id: event_id(),
        platform: platform.to_string(),
        project_id: profile.get_project_id(),
        received: DateTime::from_timestamp(profile.get_received(), 0)
            .expect("timestamp out of range"),
        release: profile.get_release().map(|s| s.to_string()),
        stacktrace: StackTrace {
            frames: ni.stack_trace.clone(),
        },
        tags,
        timestamp: profile.get_timestamp(),
    };

    Occurrence {
        culprit: transaction.name.clone(),
        detection_time: chrono::Utc::now(),
        event,
        evidence_data: generate_evidence_data(profile, &ni),
        evidence_display: generate_evidence_display(profile, &ni),
        fingerprint: vec![fingerprint],
        id: event_id(),
        issue_title: title,
        level: "info".to_string(),
        payload_type: OCCURRENCE_PAYLOAD.to_string(),
        project_id: profile.get_project_id(),
        resource_id: None,
        subtitle: ni.node.name,
        r#type: issue_type,

        // Stats fields
        category: ni.category,
        duration_ns: ni.node.duration_ns,
        sample_count: ni.node.sample_count,
    }
}

/// Finds occurrences in a profile by detecting frames and frame drop causes.
pub fn find_occurences(
    profile: &dyn ProfileInterface,
    call_trees: &CallTreesU64,
) -> Vec<Occurrence> {
    let mut occurrences = Vec::new();

    // Check if there are detection jobs for this platform
    if let Some(jobs) = DETECT_FRAME_JOBS.get(&profile.get_platform()) {
        for options in jobs {
            detect_frame::detect_frame(profile, call_trees, options.as_ref(), &mut occurrences);
        }
    }

    // Find frame drop causes
    find_frame_drop_cause(profile, call_trees, &mut occurrences);

    occurrences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_duration_to_nearest_multiple() {
        struct TestCase {
            name: String,
            duration: Duration,
            multiple: Duration,
            expected: Duration,
        }

        let test_cases = [
            TestCase {
                name: "round_up".to_string(),
                duration: Duration::from_secs_f64(1.7),
                multiple: Duration::from_secs(1),
                expected: Duration::from_secs(2),
            },
            TestCase {
                name: "round_down".to_string(),
                duration: Duration::from_secs_f64(1.3),
                multiple: Duration::from_secs(1),
                expected: Duration::from_secs(1),
            },
            TestCase {
                name: "exact_multiple".to_string(),
                duration: Duration::from_secs(5),
                multiple: Duration::from_secs(1),
                expected: Duration::from_secs(5),
            },
            TestCase {
                name: "halfway_round_up".to_string(),
                duration: Duration::from_millis(150),
                multiple: Duration::from_millis(100),
                expected: Duration::from_millis(200),
            },
            TestCase {
                name: "smaller_duration_than_multiple".to_string(),
                duration: Duration::from_millis(30),
                multiple: Duration::from_millis(100),
                expected: Duration::from_millis(0),
            },
            TestCase {
                name: "larger_duration_than_multiple".to_string(),
                duration: Duration::from_secs(10),
                multiple: Duration::from_secs(3),
                expected: Duration::from_secs(9),
            },
            TestCase {
                name: "with_zero_duration".to_string(),
                duration: Duration::from_secs(0),
                multiple: Duration::from_secs(10),
                expected: Duration::from_secs(0),
            },
            TestCase {
                name: "with_zero_multiple".to_string(),
                duration: Duration::from_secs(5),
                multiple: Duration::from_secs(0),
                expected: Duration::from_secs(5),
            },
        ];

        for test_case in test_cases {
            let result = round_duration_to_nearest_multiple(test_case.duration, test_case.multiple);
            assert_eq!(
                result, test_case.expected,
                "Test case '{}' failed: expected {:?}, got {:?}",
                test_case.name, test_case.expected, result
            );
        }
    }

    #[test]
    fn test_normalize_android_stack_trace() {
        struct TestCase {
            name: String,
            input: Vec<frame::Frame>,
            output: Vec<frame::Frame>,
        }

        let tests = vec![TestCase {
            name: "Normalize Android stack trace".to_string(),
            input: vec![frame::Frame {
                package: Some("com.google.gson".to_string()),
                function: Some("com.google.gson.JSONDecode.decode()".to_string()),
                ..Default::default()
            }],
            output: vec![frame::Frame {
                package: Some("com.google.gson".to_string()),
                function: Some("JSONDecode.decode()".to_string()),
                ..Default::default()
            }],
        }];

        for mut tt in tests {
            normalize_android_stack_trace(&mut tt.input);
            assert_eq!(tt.input, tt.output, "Test '{}' failed", tt.name);
        }
    }
}
