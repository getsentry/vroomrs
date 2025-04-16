use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hasher,
    rc::Rc,
};

use once_cell::sync::Lazy;
use pyo3::{pyclass, pymethods};

use crate::{frame::Frame, types::Platform};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
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

    pub fn set_duration(&mut self, timestamp: u64) {
        self.end_ns = timestamp;
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

    pub fn close(&mut self, mut timestamp: u64) {
        if self.end_ns == 0 {
            self.set_duration(timestamp);
        } else {
            timestamp = self.end_ns;
        }
        for child in &self.children {
            child.borrow_mut().close(timestamp);
        }
    }

    // `collect_functions` walks the node tree, collects any function with a non zero
    // self-time and writes them into the `results` parameter.
    //
    // The meaning of self-time is slightly modified here to adapt better for our use case.
    //
    // For system functions, the self-time is what you would expect, it's the difference
    // between the duration of the function, and the sum of the duration of it's children.
    // e.g. if `foo` is a system function with a duration of 100ms, and it has 3 children
    // with durations 20ms, 30ms and 40ms respectively, the self-time of `foo` will be 10ms
    // because 100ms - 20ms - 30ms - 40ms = 10ms.
    //
    // For application functions, the self-time only looks at the time spent by it's
    // descendents that are also application functions. That is, system functions do not
    // affect the self-time of application functions.
    // e.g. if `bar` is an application function with a duration of 100ms, and it has 3
    // children with durations 20ms, 30ms, and 40ms, and they are system, application, system
    // functions respectively, the self-time of `bar` will be 70ms because
    // 100ms - 30ms = 70ms.
    pub fn collect_functions(
        &self,
        results: &mut HashMap<u32, CallTreeFunction>,
        thread_id: &str,
        node_depth: u16,
        min_depth: u16,
    ) -> (u64, u64) {
        let mut children_application_duration_ns: u64 = 0;
        let mut children_system_duration_ns: u64 = 0;

        // determine the amount of time spent in application vs system functions in the children
        for child in &self.children {
            let (application_duration_ns, system_duration_ns) =
                child
                    .borrow()
                    .collect_functions(results, thread_id, node_depth + 1, min_depth);
            children_application_duration_ns += application_duration_ns;
            children_system_duration_ns += system_duration_ns;
        }

        // calculate the time spent in application functions in this function
        let mut application_duration_ns = children_application_duration_ns;
        // in the event that the time spent in application functions in the descendents exceed
        // the frame duration, we cap it at the frame duration
        if application_duration_ns > self.duration_ns {
            application_duration_ns = self.duration_ns
        }

        let mut self_time_ns: u64 = 0;

        if node_depth >= min_depth && should_aggregate_frame(&self.frame) {
            if self.is_application {
                // cannot use `node.duration_ns - children_application_duration_ns > 0` in case it underflows
                if self.duration_ns > children_application_duration_ns {
                    // application function's self time only looks at the time
                    // spent in application function in its descendents
                    self_time_ns = self.duration_ns - children_application_duration_ns;

                    // credit the self time of this application function
                    // to the total time spent in application functions
                    application_duration_ns += self_time_ns;
                }
            } else {
                // cannot use `node.duration_ns - children_application_duration_ns - children_system_duration_ns` in case it underflows
                if self.duration_ns > children_application_duration_ns + children_system_duration_ns
                {
                    // system function's self time looks at all descendents of its descendents
                    self_time_ns = self.duration_ns
                        - children_application_duration_ns
                        - children_system_duration_ns
                }
            }

            if self_time_ns > 0 {
                // casting to an uint32 here because snuba does not handle uint64 values
                // well as it is converted to a float somewhere
                // not changing to the 32 bit hash function here to preserve backwards
                // compatibility with existing fingerprints that we can cast
                let fingerprint = self.frame.fingerprint();

                results
                    .entry(fingerprint)
                    .and_modify(|function| {
                        function.self_times_ns.push(self_time_ns);
                        function.sum_self_time_ns += self_time_ns;
                        function.sample_count += self.sample_count;
                        if self_time_ns > function.max_duration {
                            function.max_duration = self_time_ns;
                            if thread_id != function.thread_id {
                                function.thread_id = thread_id.to_string();
                            }
                        }
                    })
                    .or_insert(CallTreeFunction {
                        fingerprint,
                        function: self
                            .frame
                            .function
                            .as_ref()
                            .map(|f| f.into())
                            .unwrap_or_default(),
                        package: self.frame.module_or_package(),
                        in_app: self.is_application,
                        self_times_ns: vec![self_time_ns],
                        sum_self_time_ns: self_time_ns,
                        sample_count: self.sample_count,
                        thread_id: thread_id.to_string(),
                        max_duration: self_time_ns,
                    });
            }
        } // end node_depth >= min_depth && should_aggregate_frame

        // this pair represents the time spent in application functions vs
        // time spent in system functions by this function and all of its descendents
        (
            application_duration_ns,
            self.duration_ns - application_duration_ns,
        )
    }
}

#[pyclass]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallTreeFunction {
    pub fingerprint: u32,
    pub function: String,
    pub package: String,
    pub in_app: bool,
    pub self_times_ns: Vec<u64>,
    pub sum_self_time_ns: u64,
    pub sample_count: u64,
    pub thread_id: String,
    pub max_duration: u64,
}

#[pymethods]
impl CallTreeFunction {
    /// Returns the function fingerprint.
    ///
    /// Returns:
    ///     int
    ///         The function fingerprint.
    pub fn get_fingerprint(&self) -> u32 {
        self.fingerprint
    }

    /// Returns the function name.
    ///
    /// Returns:
    ///     str
    ///         The function name.
    pub fn get_function(&self) -> &str {
        &self.function
    }

    /// Returns the package name.
    ///
    /// Returns:
    ///     str
    ///         The package name.
    pub fn get_package(&self) -> &str {
        &self.package
    }

    /// Returns whether the function is in an app or system one.
    ///
    /// Returns:
    ///     bool
    ///         True if the function is an app one, False otherwise.
    pub fn get_in_app(&self) -> bool {
        self.in_app
    }

    /// Returns the self times in nanoseconds.
    ///
    /// Returns:
    ///     list[int]
    ///         The self times in nanoseconds.
    pub fn get_self_times_ns(&self) -> Vec<u64> {
        self.self_times_ns.clone()
    }

    /// Returns the sum of self times in nanoseconds.
    ///
    /// Returns:
    ///     int
    ///         The sum of self times in nanoseconds.
    pub fn get_sum_self_time_ns(&self) -> u64 {
        self.sum_self_time_ns
    }

    /// Returns the sample count.
    ///
    /// Returns:
    ///     int
    ///         The sample count.
    pub fn get_sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Returns the thread ID.
    ///
    /// Returns:
    ///     str
    ///         The thread ID.
    pub fn get_thread_id(&self) -> &str {
        &self.thread_id
    }

    /// Returns the maximum duration in nanoseconds.
    ///
    /// Returns:
    ///     int
    ///         The maximum duration in nanoseconds.
    pub fn get_max_duration(&self) -> u64 {
        self.max_duration
    }
}

fn should_aggregate_frame(frame: &Frame) -> bool {
    let frame_function = frame.function.as_deref().unwrap_or_default();

    // frames with no name are not valuable for aggregation
    if frame_function.is_empty() {
        return false;
    }

    // hard coded list of functions that we should not aggregate by
    if let Some(platform) = frame.platform {
        if let Some(function_deny_list) = FUNCTION_DENY_LIST_BY_PLATFORM.get(&platform) {
            if function_deny_list.contains(frame_function) {
                return false;
            }
        }
    }

    if let Some(platform) = frame.platform {
        if OBFUSCATION_SUPPORTED_PLATFORMS.contains(&platform) {
            /*
                There are 4 possible deobfuscation statuses
                1. deobfuscated	- The frame was successfully deobfuscated.
                2. partial			- The frame was only partially deobfuscated.
                                                    (likely just the class name and not the method name)
                3. missing			- The frame could not be deobfuscated, not found in the mapping file.
                                                    (likely to be a system library that should not be obfuscated)
                4. <no status>	- The frame did not go through deobfuscation. No mapping file specified.

                Only the `partial` status should not be aggregated because only having a deobfuscated
                class names makes grouping ineffective.
            */
            if frame
                .data
                .as_ref()
                .and_then(|data| data.deobfuscation_status.as_deref())
                == Some("partial")
            {
                return false;
            }

            // obfuscated package names often don't contain a dot (`.`)
            let frame_package = frame.module_or_package();
            if !frame_package.contains('.') {
                return false;
            }
        }

        if SYMBOLICATION_SUPPORTED_PLATFORMS.contains(&platform) {
            return is_symbolicated_frame(frame);
        }
    }

    // all other frames are safe to aggregate
    true
}

fn is_symbolicated_frame(frame: &Frame) -> bool {
    if let Some(platform) = frame.platform {
        if platform == Platform::JavaScript && frame.is_react_native {
            return frame.data.as_ref().is_some_and(|data| {
                data.js_symbolicated
                    .is_some_and(|js_symbolicated| js_symbolicated)
            });
        } else if platform == Platform::JavaScript || platform == Platform::Node {
            // else, if it's not a react-native but simply a js frame from either
            // browser js or node, for now we'll simply consider everything as symbolicated
            // and just ingest into metrics
            return true;
        }
    }
    frame.data.as_ref().is_some_and(|data| {
        data.symbolicator_status
            .as_ref()
            .is_some_and(|status| status == "symbolicated")
    })
}

pub(crate) static FUNCTION_DENY_LIST_BY_PLATFORM: Lazy<HashMap<Platform, HashSet<&'static str>>> =
    Lazy::new(|| HashMap::from([(Platform::Cocoa, HashSet::from(["main"]))]));

pub(crate) static OBFUSCATION_SUPPORTED_PLATFORMS: Lazy<HashSet<Platform>> =
    Lazy::new(|| HashSet::from([Platform::Android, Platform::Java]));

pub(crate) static SYMBOLICATION_SUPPORTED_PLATFORMS: Lazy<HashSet<Platform>> =
    Lazy::new(|| HashSet::from([Platform::JavaScript, Platform::Node, Platform::Cocoa]));

#[cfg(test)]
mod tests {

    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    use crate::{
        frame::{Data, Frame},
        nodetree::{is_symbolicated_frame, CallTreeFunction, Node},
        types::Platform,
    };

    use pretty_assertions::assert_eq;

    #[test]
    fn test_is_symbolicated() {
        struct TestStruct {
            name: String,
            frame: Frame,
            want: bool,
        }

        let test_cases = [
            TestStruct {
                name: "react-native-symbolicated".to_string(),
                frame: Frame {
                    is_react_native: true,
                    platform: Some(Platform::JavaScript),
                    data: Some(Data {
                        js_symbolicated: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                want: true,
            }, // end first test
            TestStruct {
                name: "react-native-not-symbolicated".to_string(),
                frame: Frame {
                    is_react_native: true,
                    platform: Some(Platform::JavaScript),
                    data: Some(Data {
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                want: false,
            }, // end second test
            TestStruct {
                name: "browser-js".to_string(),
                frame: Frame {
                    platform: Some(Platform::JavaScript),
                    data: Some(Data {
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                want: true,
            }, // end third test
            TestStruct {
                name: "nodejs".to_string(),
                frame: Frame {
                    platform: Some(Platform::Node),
                    data: Some(Data {
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                want: true,
            }, // end fourth test
        ];

        for test in &test_cases {
            assert_eq!(
                is_symbolicated_frame(&test.frame),
                test.want,
                "test `{}` failed",
                test.name
            );
        }
    }

    #[test]
    fn test_node_collect_functions() {
        struct TestStruct {
            name: String,
            node: Node,
            want: HashMap<u32, CallTreeFunction>,
        }

        const FINGERPRINT_FOO: u32 = 2655321105;
        const FINGERPRINT_BAR: u32 = 1766712469;
        const FINGERPRINT_BAZ: u32 = 509004053;
        const FINGERPRINT_QUX: u32 = 1897269005;
        const FINGERPRINT_MAIN: u32 = 1169397183;

        let test_cases: Vec<TestStruct> = vec![
            TestStruct {
                name: "single application node".to_string(),
                node: Node {
                    duration_ns: 10,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Python),
                        function: Some("foo".to_string()),
                        package: Some("foo".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: [(
                    FINGERPRINT_FOO,
                    CallTreeFunction {
                        fingerprint: FINGERPRINT_FOO,
                        in_app: true,
                        function: "foo".to_string(),
                        package: "foo".to_string(),
                        self_times_ns: vec![10],
                        sum_self_time_ns: 10,
                        max_duration: 10,
                        ..Default::default()
                    },
                )]
                .iter()
                .cloned()
                .collect(),
            }, // end first test case
            TestStruct {
                name: "single system node".to_string(),
                node: Node {
                    duration_ns: 10,
                    is_application: false,
                    frame: Frame {
                        platform: Some(Platform::Python),
                        function: Some("foo".to_string()),
                        package: Some("foo".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: [(
                    FINGERPRINT_FOO,
                    CallTreeFunction {
                        fingerprint: FINGERPRINT_FOO,
                        in_app: false,
                        function: "foo".to_string(),
                        package: "foo".to_string(),
                        self_times_ns: vec![10],
                        sum_self_time_ns: 10,
                        max_duration: 10,
                        ..Default::default()
                    },
                )]
                .iter()
                .cloned()
                .collect(),
            }, // end second test case
            TestStruct {
                name: "non leaf node with non zero self time".to_string(),
                node: Node {
                    duration_ns: 20,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Python),
                        function: Some("foo".to_string()),
                        package: Some("foo".to_string()),
                        ..Default::default()
                    },
                    children: vec![Rc::new(RefCell::new(Node {
                        duration_ns: 10,
                        is_application: true,
                        frame: Frame {
                            platform: Some(Platform::Python),
                            function: Some("bar".to_string()),
                            package: Some("bar".to_string()),
                            ..Default::default()
                        },
                        ..Default::default()
                    }))],
                    ..Default::default()
                },
                want: [
                    (
                        FINGERPRINT_FOO,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_FOO,
                            in_app: true,
                            function: "foo".to_string(),
                            package: "foo".to_string(),
                            self_times_ns: vec![10],
                            sum_self_time_ns: 10,
                            max_duration: 10,
                            ..Default::default()
                        },
                    ),
                    (
                        FINGERPRINT_BAR,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_BAR,
                            in_app: true,
                            function: "bar".to_string(),
                            package: "bar".to_string(),
                            self_times_ns: vec![10],
                            sum_self_time_ns: 10,
                            max_duration: 10,
                            ..Default::default()
                        },
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }, // end third test case
            TestStruct {
                name: "application node wrapping system nodes of same duration".to_string(),
                node: Node {
                    duration_ns: 10,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Python),
                        function: Some("main".to_string()),
                        package: Some("main".to_string()),
                        ..Default::default()
                    },
                    children: vec![Rc::new(RefCell::new(Node {
                        duration_ns: 10,
                        is_application: true,
                        frame: Frame {
                            platform: Some(Platform::Python),
                            function: Some("foo".to_string()),
                            package: Some("foo".to_string()),
                            ..Default::default()
                        },
                        children: vec![Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            is_application: false,
                            frame: Frame {
                                platform: Some(Platform::Python),
                                function: Some("bar".to_string()),
                                package: Some("bar".to_string()),
                                ..Default::default()
                            },
                            children: vec![Rc::new(RefCell::new(Node {
                                duration_ns: 10,
                                is_application: false,
                                frame: Frame {
                                    platform: Some(Platform::Python),
                                    function: Some("baz".to_string()),
                                    package: Some("baz".to_string()),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }))],
                            ..Default::default()
                        }))],
                        ..Default::default()
                    }))],
                    ..Default::default()
                },
                want: [
                    (
                        FINGERPRINT_FOO,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_FOO,
                            in_app: true,
                            function: "foo".to_string(),
                            package: "foo".to_string(),
                            self_times_ns: vec![10],
                            sum_self_time_ns: 10,
                            max_duration: 10,
                            ..Default::default()
                        },
                    ),
                    (
                        FINGERPRINT_BAZ,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_BAZ,
                            in_app: false,
                            function: "baz".to_string(),
                            package: "baz".to_string(),
                            self_times_ns: vec![10],
                            sum_self_time_ns: 10,
                            max_duration: 10,
                            ..Default::default()
                        },
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }, // end fourth test case
            TestStruct {
                name: "multitple occurrences of same functions".to_string(),
                node: Node {
                    duration_ns: 40,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Python),
                        function: Some("main".to_string()),
                        package: Some("main".to_string()),
                        ..Default::default()
                    },
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            is_application: true,
                            frame: Frame {
                                platform: Some(Platform::Python),
                                function: Some("foo".to_string()),
                                package: Some("foo".to_string()),
                                ..Default::default()
                            },
                            children: vec![Rc::new(RefCell::new(Node {
                                duration_ns: 10,
                                is_application: false,
                                frame: Frame {
                                    platform: Some(Platform::Python),
                                    function: Some("bar".to_string()),
                                    package: Some("bar".to_string()),
                                    ..Default::default()
                                },
                                children: vec![Rc::new(RefCell::new(Node {
                                    duration_ns: 10,
                                    is_application: false,
                                    frame: Frame {
                                        platform: Some(Platform::Python),
                                        function: Some("baz".to_string()),
                                        package: Some("baz".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }))],
                                ..Default::default()
                            }))],
                            ..Default::default()
                        })),
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            is_application: false,
                            frame: Frame {
                                platform: Some(Platform::Python),
                                function: Some("qux".to_string()),
                                package: Some("qux".to_string()),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                        Rc::new(RefCell::new(Node {
                            duration_ns: 20,
                            is_application: true,
                            frame: Frame {
                                platform: Some(Platform::Python),
                                function: Some("foo".to_string()),
                                package: Some("foo".to_string()),
                                ..Default::default()
                            },
                            children: vec![Rc::new(RefCell::new(Node {
                                duration_ns: 20,
                                is_application: false,
                                frame: Frame {
                                    platform: Some(Platform::Python),
                                    function: Some("bar".to_string()),
                                    package: Some("bar".to_string()),
                                    ..Default::default()
                                },
                                children: vec![Rc::new(RefCell::new(Node {
                                    duration_ns: 20,
                                    is_application: false,
                                    frame: Frame {
                                        platform: Some(Platform::Python),
                                        function: Some("baz".to_string()),
                                        package: Some("baz".to_string()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }))],
                                ..Default::default()
                            }))],
                            ..Default::default()
                        })),
                    ],
                    ..Default::default()
                },
                want: [
                    (
                        FINGERPRINT_FOO,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_FOO,
                            in_app: true,
                            function: "foo".to_string(),
                            package: "foo".to_string(),
                            self_times_ns: vec![10, 20],
                            sum_self_time_ns: 30,
                            max_duration: 20,
                            ..Default::default()
                        },
                    ),
                    (
                        FINGERPRINT_BAZ,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_BAZ,
                            in_app: false,
                            function: "baz".to_string(),
                            package: "baz".to_string(),
                            self_times_ns: vec![10, 20],
                            sum_self_time_ns: 30,
                            max_duration: 20,
                            ..Default::default()
                        },
                    ),
                    (
                        FINGERPRINT_QUX,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_QUX,
                            in_app: false,
                            function: "qux".to_string(),
                            package: "qux".to_string(),
                            self_times_ns: vec![10],
                            sum_self_time_ns: 10,
                            max_duration: 10,
                            ..Default::default()
                        },
                    ),
                    (
                        FINGERPRINT_MAIN,
                        CallTreeFunction {
                            fingerprint: FINGERPRINT_MAIN,
                            in_app: true,
                            function: "main".to_string(),
                            package: "main".to_string(),
                            self_times_ns: vec![10],
                            sum_self_time_ns: 10,
                            max_duration: 10,
                            ..Default::default()
                        },
                    ),
                ]
                .iter()
                .cloned()
                .collect(),
            }, // end fifth test case
            TestStruct {
                name: "obfuscated android frames".to_string(),
                node: Node {
                    duration_ns: 20,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Android),
                        function: Some("a.B()".to_string()),
                        package: Some("a".to_string()),
                        data: Some(Data {
                            deobfuscation_status: Some("missing".to_string()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            is_application: true,
                            frame: Frame {
                                platform: Some(Platform::Android),
                                function: Some("com.example.Thing.doStuff()".to_string()),
                                package: Some("com.example".to_string()),
                                data: Some(Data {
                                    deobfuscation_status: Some("deobfuscated".to_string()),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                        Rc::new(RefCell::new(Node {
                            duration_ns: 10,
                            is_application: true,
                            frame: Frame {
                                platform: Some(Platform::Android),
                                function: Some("com.example.Thing.a()".to_string()),
                                package: Some("com.example".to_string()),
                                data: Some(Data {
                                    deobfuscation_status: Some("partial".to_string()),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            ..Default::default()
                        })),
                    ],
                    ..Default::default()
                },
                want: [(
                    1902388659,
                    CallTreeFunction {
                        fingerprint: 1902388659,
                        in_app: true,
                        function: "com.example.Thing.doStuff()".to_string(),
                        package: "com.example".to_string(),
                        self_times_ns: vec![10],
                        sum_self_time_ns: 10,
                        max_duration: 10,
                        ..Default::default()
                    },
                )]
                .iter()
                .cloned()
                .collect(),
            }, // end sixth test case
            TestStruct {
                name: "obfuscated java frames".to_string(),
                node: Node {
                    duration_ns: 20,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Java),
                        function: Some("a.B()".to_string()),
                        package: Some("a".to_string()),
                        ..Default::default()
                    },
                    children: vec![Rc::new(RefCell::new(Node {
                        duration_ns: 10,
                        is_application: true,
                        frame: Frame {
                            platform: Some(Platform::Java),
                            function: Some("com.example.Thing.doStuff()".to_string()),
                            package: Some("com.example".to_string()),
                            ..Default::default()
                        },
                        ..Default::default()
                    }))],
                    ..Default::default()
                },
                want: [(
                    1902388659,
                    CallTreeFunction {
                        fingerprint: 1902388659,
                        in_app: true,
                        function: "com.example.Thing.doStuff()".to_string(),
                        package: "com.example".to_string(),
                        self_times_ns: vec![10],
                        sum_self_time_ns: 10,
                        max_duration: 10,
                        ..Default::default()
                    },
                )]
                .iter()
                .cloned()
                .collect(),
            }, // end seventh test case
            TestStruct {
                name: "cocoa main frame".to_string(),
                node: Node {
                    duration_ns: 10,
                    is_application: true,
                    frame: Frame {
                        platform: Some(Platform::Cocoa),
                        function: Some("main".to_string()),
                        package: Some("iOS-Swift".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                want: HashMap::new(),
            }, // end eighth test case
        ];

        for test in &test_cases {
            let mut results: HashMap<u32, CallTreeFunction> = HashMap::new();
            test.node.collect_functions(&mut results, "", 0, 0);

            assert_eq!(results, test.want, "test `{}` failed", test.name);
        }
    }
}
