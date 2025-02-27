mod chunk;

use serde::{Deserialize, Serialize};

use crate::types::Platform;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct AndroidThread {
    pub id: u64,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
pub struct AndroidMethod {
    pub class_name: String,
    pub data: Data,
    // method_id is not optional, but in our Vroom service,
    // the field was defined with the json tag `json:"id,omitempty"`
    // which means we (wrongly) skip the serialization of such
    // field if it's 0. By using a default value, we can safely deserialize
    // profiles that were stored previously through the vroom service.
    #[serde(default)]
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_frames: Option<Vec<AndroidMethod>>,
    pub name: String,
    pub signature: String,
    pub source_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_app: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
pub struct Data {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deobfuscation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js_symbolicated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_in_app: Option<i8>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
pub struct Duration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nanos: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
pub struct EventMonotonic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<Duration>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
pub struct EventTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monotonic: Option<EventMonotonic>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Enter,
    Exit,
    Unwind,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum Clock {
    Global,
    Cpu,
    Wall,
    Dual,
    #[default]
    None,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct AndroidEvent {
    pub action: Option<String>,
    pub thread_id: u64,
    // method_id is not optional, but in our Vroom service,
    // the field was defined with the json tag `json:"id,omitempty"`
    // which means we (wrongly) skip the serialization of such
    // field if it's 0. By using a default value, we can safely deserialize
    // profiles that were stored previously through the vroom service.
    #[serde(default)]
    pub method_id: u64,
    pub time: EventTime,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq)]
pub struct Android {
    pub clock: Clock,
    pub events: Vec<AndroidEvent>,
    pub methods: Vec<AndroidMethod>,
    pub start_time: u64,
    pub threads: Vec<AndroidThread>,
}

impl Android {
    /// Wall-clock time is supposed to be monotonic
    /// in a few rare cases we've noticed this was not the case.
    /// Due to some overflow happening client-side in the embedded
    /// profiler, the sequence might be decreasing at certain points.
    ///
    /// This is just a workaround to mitigate this issue, should it
    /// happen.
    pub fn fix_samples_time(&mut self) {
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
        }
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

#[cfg(test)]
mod tests {
    use crate::android::{
        Android, AndroidEvent, AndroidThread, Clock, Duration, EventMonotonic, EventTime,
    };

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
                        action: Some("Enter".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Enter".to_string()),
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
                        action: Some("Exit".to_string()),
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
                        action: Some("Exit".to_string()),
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
}
