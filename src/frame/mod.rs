mod python_std_lib;

use std::{collections::HashSet, hash::Hasher};

use fnv_rs::Fnv64;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static WINDOWS_PATH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^([a-z]:\\|\\\\)").unwrap());
static PACKAGE_EXTENSION_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\.(dylib|so|a|dll|exe)$").unwrap());
static JS_SYSTEM_PACKAGE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"node_modules|^(@moz-extension|chrome-extension)").unwrap());
static COCOA_SYSTEM_PACKAGE: Lazy<HashSet<&'static str>> =
    Lazy::new(|| HashSet::from(["Sentry", "hermes"]));

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct Frame {
    #[serde(rename = "colno", skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,

    pub data: Option<Data>,

    #[serde(rename = "filename", skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,

    #[serde(rename = "function", skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,

    #[serde(rename = "in_app", skip_serializing_if = "Option::is_none")]
    pub in_app: Option<bool>,

    #[serde(rename = "instruction_addr", skip_serializing_if = "Option::is_none")]
    pub instruction_addr: Option<String>,

    #[serde(rename = "lang", skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,

    #[serde(rename = "lineno", skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub method_id: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,

    #[serde(rename = "abs_path", skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sym_addr: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(skip)]
    pub is_react_native: bool,
}

/// Determines whether an android frame's package points at an OS/runtime
/// location. Package paths appear both with and without a leading slash (e.g.
/// "system/lib64/libhwui.so" and "/system/..."), so we strip one before matching.
pub(crate) fn is_android_system_package(package: &str) -> bool {
    let norm = package.strip_prefix('/').unwrap_or(package);
    norm.starts_with("system/")
        || norm.starts_with("vendor/")
        || norm.starts_with("apex/")
        || package == "[vdso]"
        || package == "[stack]"
        || package.starts_with("[anon:dalvik-")
}

/// Platform, runtime and SDK class-name namespaces for android/JVM frames. A
/// frame whose class (module) starts with one of these is a system frame;
/// everything else — the app's own code and its bundled libraries are application code.
/// `io.sentry.` is our own instrumentation, treated as
/// system, like we do for python and cocoa.
const ANDROID_SYSTEM_MODULE_PREFIXES: &[&str] = &[
    "java.",
    "javax.",
    "kotlin.",
    "kotlinx.",
    "android.",
    "com.android.",
    "dalvik.",
    "libcore.",
    "sun.",
    "jdk.",
    "io.sentry.",
];

/// Determines whether an android/JVM class name belongs to a platform, runtime
/// or SDK namespace (i.e. not the application's own code).
pub(crate) fn is_android_system_module(name: &str) -> bool {
    ANDROID_SYSTEM_MODULE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Determines whether the image represents that of the application
/// binary (or a binary embedded in the application binary) by checking its package path.
pub fn is_cocoa_application_package(p: &str) -> bool {
    // These are the path patterns that iOS uses for applications,
    // system libraries are stored elsewhere.
    p.starts_with("/private/var/containers")
        || p.starts_with("/var/containers")
        || p.contains("/Developer/Xcode/DerivedData")
        || p.contains("/data/Containers/Bundle/Application")
        || p.contains(".app")
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Data {
    #[serde(
        rename = "deobfuscation_status",
        skip_serializing_if = "Option::is_none"
    )]
    pub deobfuscation_status: Option<String>,

    #[serde(
        rename = "symbolicator_status",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbolicator_status: Option<String>,

    #[serde(rename = "symbolicated", skip_serializing_if = "Option::is_none")]
    pub js_symbolicated: Option<bool>,
}

// Taken from https://github.com/getsentry/sentry/blob/1c9cf8bd92f65e933a407d8ee37fb90997c1c76c/static/app/components/events/interfaces/frame/utils.tsx#L8-L12
// This takes a frame's package and formats it in such a way that is suitable for displaying/aggregation.
fn trim_package(pkg: &str) -> String {
    let separator = if WINDOWS_PATH_REGEX.is_match(pkg) {
        '\\'
    } else {
        '/'
    };

    let pieces: Vec<&str> = pkg.split(separator).collect();

    let mut filename = if !pieces.is_empty() {
        pieces[pieces.len() - 1]
    } else {
        pkg
    };

    if pieces.len() >= 2 && filename.is_empty() {
        filename = pieces[pieces.len() - 2];
    }

    if filename.is_empty() {
        filename = pkg;
    }

    // Replace package extensions with empty string
    PACKAGE_EXTENSION_REGEX
        .replace_all(filename, "")
        .into_owned()
}

impl Frame {
    // is_main returns true if the function is considered the main function.
    // It also returns an offset indicate if we need to keep the previous frame or not.
    // This only works for cocoa profiles.
    fn is_main(&self) -> (bool, i32) {
        if self.status.as_deref() != Some("symbolicated") {
            return (false, 0);
        }

        match self.function.as_deref() {
            Some("main") => (true, 0),
            Some("UIApplicationMain") => (true, -1),
            _ => (false, 0),
        }
    }

    fn is_node_application_frame(&self) -> bool {
        self.path
            .as_ref()
            .is_none_or(|path| !path.starts_with("node:") && !path.contains("node_modules"))
    }

    fn is_javascript_application_frame(&self) -> bool {
        if let Some(function) = &self.function {
            if function.starts_with('[') {
                return false;
            }
        }

        // If filename reveals node_modules, it's a vendor frame regardless
        // of what abs_path looks like. This handles the case where abs_path
        // is an unresolved CDN URL (e.g. https://.../vendors.HASH.js) but
        // the source map resolved the filename to a node_modules path.
        if let Some(file) = &self.file {
            if JS_SYSTEM_PACKAGE_REGEX.is_match(file) {
                return false;
            }
        }

        self.path.is_none()
            || self
                .path
                .as_ref()
                .is_some_and(|path| path.is_empty() || !JS_SYSTEM_PACKAGE_REGEX.is_match(path))
    }

    fn is_cocoa_application_frame(&self) -> bool {
        let (is_main, _) = self.is_main();
        if is_main {
            // the main frame is found in the user package but should be treated
            // as a system frame as it does not contain any user code
            return false;
        }

        // Some packages are known to be system packages.
        // If we detect them, mark them as a system frame immediately.
        if COCOA_SYSTEM_PACKAGE.contains(self.module_or_package().as_str()) {
            return false;
        }

        self.package
            .as_ref()
            .is_some_and(|package| is_cocoa_application_package(package))
    }

    fn is_rust_application_frame(&self) -> bool {
        self.package.as_ref().is_some_and(|package| {
            !package.contains("/library/std/src/")
                && !package.starts_with("/usr/lib/system/")
                && !package.starts_with("/rustc/")
                && !package.starts_with("/usr/local/rustup/")
                && !package.starts_with("/usr/local/cargo/")
        })
    }

    fn is_python_application_frame(&self) -> bool {
        // Check path patterns that indicate system packages
        if let Some(path) = &self.path {
            if path.contains("/site-packages/")
                || path.contains("/dist-packages/")
                || path.contains("\\site-packages\\")
                || path.contains("\\dist-packages\\")
                || path.starts_with("/usr/local/")
            {
                return false;
            }
        }

        // Check if module is from sentry_sdk
        if let Some(module) = &self.module {
            if let Some(module) = module.split('.').next() {
                // Sentry SDK should be considered a system frame
                if module == "sentry_sdk" {
                    return false;
                }

                // Check against Python standard library modules
                return !python_std_lib::PYTHON_STDLIB.contains(module);
            }
        }

        true
    }

    fn is_php_application_frame(&self) -> bool {
        self.path
            .as_ref()
            .is_none_or(|path| !path.contains("/vendor/"))
    }

    fn is_android_application_frame(&self) -> bool {
        if self
            .package
            .as_deref()
            .is_some_and(is_android_system_package)
        {
            return false;
        }

        let symbol = self
            .module
            .as_deref()
            .filter(|m| !m.is_empty())
            .or(self.function.as_deref())
            .unwrap_or_default();
        !is_android_system_module(symbol)
    }

    /// Whether the frame is synthetic noise that's dropped from android profiles,
    /// mirroring sentry-java's TombstoneParser:
    /// - ART runtime/interpreter frames (libart.so), not actionable for developers.
    /// - anonymous VMA frames with no function name, which can't be symbolicated
    ///   and have no value in themselves.
    pub(crate) fn is_synthetic_android_frame(&self) -> bool {
        let Some(package) = self.package.as_deref() else {
            return false;
        };
        package.ends_with("libart.so")
            || (package.starts_with("<anonymous")
                && self.function.as_deref().is_none_or(str::is_empty))
    }

    fn set_in_app(&mut self, p: &str) {
        // for react-native the in_app field seems to be messed up most of the times,
        // with system libraries and other frames that are clearly system frames
        // labelled as `in_app`.
        // This is likely because RN uses static libraries which are bundled into the app binary.
        // When symbolicated they are marked in_app.
        //
        // For this reason, for react-native app (p.Platform != f.Platform), we skip the f.InApp!=nil
        // check as this field would be highly unreliable, and rely on our rules instead
        if self.in_app.is_some() && self.platform.as_ref().is_some_and(|fp| p == fp) {
            return;
        }

        let is_application = match self.platform.as_ref().unwrap().as_str() {
            "node" => self.is_node_application_frame(),
            "javascript" => self.is_javascript_application_frame(),
            "cocoa" => self.is_cocoa_application_frame(),
            "rust" => self.is_rust_application_frame(),
            "python" => self.is_python_application_frame(),
            "php" => self.is_php_application_frame(),
            "java" | "native" | "android" if p == "android" => self
                .in_app
                .unwrap_or_else(|| self.is_android_application_frame()),
            _ => false,
        };

        self.in_app = Some(is_application);
    }

    #[allow(dead_code)]
    fn is_in_app(&self) -> bool {
        self.in_app.unwrap_or(false)
    }

    fn set_platform(&mut self, p: &str) {
        if self.platform.is_none() {
            self.platform = Some(p.to_string());
        }
    }

    fn set_status(&mut self) {
        if let Some(data) = &self.data {
            if let Some(symbolicator_status) = &data.symbolicator_status {
                if !symbolicator_status.is_empty() {
                    self.status = Some(symbolicator_status.clone());
                }
            }
        }
    }

    pub fn normalize(&mut self, p: &str) {
        // Call order is important since set_in_app uses status and platform
        self.set_status();
        self.set_platform(p);
        self.set_in_app(p);
    }

    /// Returns the module name if present, otherwise returns the trimmed package name.
    /// If neither is present, returns an empty string.
    pub fn module_or_package(&self) -> String {
        if let Some(module) = &self.module {
            if !module.is_empty() {
                return module.clone();
            }
        }

        if let Some(package) = &self.package {
            if !package.is_empty() {
                return trim_package(package);
            }
        }

        String::new()
    }

    /// Writes frame data to the provided hash implementation.
    /// This is used to create a unique identifier for the frame.
    pub fn write_to_hash<H: std::hash::Hasher>(&self, h: &mut H) {
        let s = if let Some(module) = &self.module {
            module
        } else if let Some(package) = &self.package {
            &trim_package(package)
        } else if let Some(file) = &self.file {
            file
        } else {
            "-"
        };

        h.write(s.as_bytes());

        let s = self.function.as_deref().unwrap_or("-");
        h.write(s.as_bytes());

        // Important for native platforms to distinguish unknown frames
        if let Some(addr) = &self.instruction_addr {
            h.write(addr.as_bytes());
        }
    }

    pub fn fingerprint(&self, parent_fingerprint: Option<u32>) -> u32 {
        let mut hasher = Fnv64::default();
        hasher.write(self.module_or_package().as_bytes());
        hasher.write(":".as_bytes());
        hasher.write(self.function.as_deref().unwrap_or_default().as_bytes());
        if let Some(parent_fingerprint) = parent_fingerprint {
            hasher.write_u32(parent_fingerprint);
        }

        // casting to an uint32 here because snuba does not handle uint64 values well
        // as it is converted to a float somewhere not changing to the 32 bit hash
        // function here to preserve backwards compatibility with existing fingerprints
        // that we can cast
        hasher.finish() as u32
    }
}

#[cfg(test)]
mod tests {
    use std::hash::Hasher;

    use super::Frame;

    #[test]
    fn test_is_cocoa_application_frame() {
        const OCK_UUID: &str = "00000000-0000-0000-0000-000000000000";
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        let test_cases = vec![
            TestStruct {
                name: "main".to_string(),
                frame: Frame {
                    function: Some("main".to_string()),
                    status: Some("symbolicated".to_string()),
                    package: Some(format!("/Users/runner/Library/Developer/CoreSimulator/Devices/{OCK_UUID}/data/Containers/Bundle/Application/{OCK_UUID}/iOS-Swift.app/Frameworks/libclang_rt.asan_iossim_dynamic.dylib",
                        )
                    ),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "main must be symbolicated".to_string(),
                frame: Frame {
                    function: Some("main".to_string()),
                    package: Some(format!("/Users/runner/Library/Developer/CoreSimulator/Devices/{OCK_UUID}/data/Containers/Bundle/Application/{OCK_UUID}/iOS-Swift.app/Frameworks/libclang_rt.asan_iossim_dynamic.dylib",
                        )
                    ),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "__sanitizer::StackDepotNode::store(unsigned int, __sanitizer::StackTrace const&, unsigned long long)".to_string(),
                frame: Frame {
                    function: Some("__sanitizer::StackDepotNode::store(unsigned int, __sanitizer::StackTrace const&, unsigned long long)".to_string()),
                    package: Some(format!("/Users/runner/Library/Developer/CoreSimulator/Devices/{OCK_UUID}/data/Containers/Bundle/Application/{OCK_UUID}/iOS-Swift.app/Frameworks/libclang_rt.asan_iossim_dynamic.dylib",
                        )
                    ),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "symbolicate_internal".to_string(),
                frame: Frame {
                    function: Some("symbolicate_internal".to_string()),
                    package: Some("/private/var/containers/Bundle/Application/00000000-0000-0000-0000-000000000000/App.app/Frameworks/Sentry.framework/Sentry".to_string()),
                    ..Default::default()
                },
                is_application: false,
            }
        ];

        for test_case in test_cases {
            let is_app = test_case.frame.is_cocoa_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_is_python_application_frame() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        let test_cases = vec![
            TestStruct {
                name: "empty".to_string(),
                frame: Frame {
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "empty".to_string(),
                frame: Frame {
                    module: Some("app".to_string()),
                    file: Some("app.py".to_string()),
                    path: Some("/home/user/app/app.py".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "app.utils".to_string(),
                frame: Frame {
                    module: Some("app.utils".to_string()),
                    file: Some("app/utils.py".to_string()),
                    path: Some("/home/user/app/app/utils.py".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "site-packges unix".to_string(),
                frame: Frame {
                    path: Some("/usr/local/lib/python3.10/site-packages/urllib3/request.py".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "site-packges dos".to_string(),
                frame: Frame {
                    path: Some("C:\\Users\\user\\AppData\\Local\\Programs\\Python\\Python310\\lib\\site-packages\\urllib3\\request.py".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "dist-packges unix".to_string(),
                frame: Frame {
                    path: Some("/usr/local/lib/python3.10/dist-packages/urllib3/request.py".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "dist-packges dos".to_string(),
                frame: Frame {
                    path: Some("C:\\Users\\user\\AppData\\Local\\Programs\\Python\\Python310\\lib\\dist-packages\\urllib3\\request.py".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "stdlib".to_string(),
                frame: Frame {
                    module: Some("multiprocessing.pool".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "sentry_sdk".to_string(),
                frame: Frame {
                    module: Some("sentry_sdk.profiler".to_string()),
                    ..Default::default()
                },
                is_application: false,
            }
        ];

        for test_case in test_cases {
            let is_app = test_case.frame.is_python_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_is_node_application_frame() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        let test_cases = vec![
            TestStruct {
                name: "empty".to_string(),
                frame: Frame {
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "empty".to_string(),
                frame: Frame {
                    path: Some("/home/user/app/app.js".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "node_modules".to_string(),
                frame: Frame {
                    path: Some("/home/user/app/node_modules/express/lib/express.js".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "internal".to_string(),
                frame: Frame {
                    path: Some("node:internal/process/task_queues".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
        ];
        for test_case in test_cases {
            let is_app = test_case.frame.is_node_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_is_javascript_application_frame() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        let test_cases = vec![
            TestStruct {
                name: "empty".to_string(),
                frame: Frame {
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "app".to_string(),
                frame: Frame {
                    path: Some("/home/user/app/app.js".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "node_modules".to_string(),
                frame: Frame {
                    path: Some("/home/user/app/node_modules/express/lib/express.js".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "app".to_string(),
                frame: Frame {
                    path: Some(
                        "@moz-extension://00000000-0000-0000-0000-000000000000/app.js".to_string(),
                    ),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "app".to_string(),
                frame: Frame {
                    path: Some(
                        "chrome-extension://00000000-0000-0000-0000-000000000000/app.js"
                            .to_string(),
                    ),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "native".to_string(),
                frame: Frame {
                    function: Some("[Native] functionPrototypeApply".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "host_function".to_string(),
                frame: Frame {
                    function: Some("[HostFunction] nativeCallSyncHook".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "gc".to_string(),
                frame: Frame {
                    function: Some("[GC Young Gen]".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "cdn_url_with_node_modules_filename".to_string(),
                frame: Frame {
                    path: Some(
                        "https://business.example.com/assets/vendors.b1d183fd6fb0da242e21.js"
                            .to_string(),
                    ),
                    file: Some(
                        "./node_modules/react-dom/cjs/react-dom.production.min.js".to_string(),
                    ),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "cdn_url_no_filename_unresolved".to_string(),
                frame: Frame {
                    path: Some(
                        "https://business.example.com/assets/vendors.b1d183fd6fb0da242e21.js"
                            .to_string(),
                    ),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "cdn_url_with_app_filename".to_string(),
                frame: Frame {
                    path: Some("https://business.example.com/assets/main.abc123.js".to_string()),
                    file: Some("./src/pages/Dashboard.tsx".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
        ];
        for test_case in test_cases {
            let is_app = test_case.frame.is_javascript_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_is_php_application_frame() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        let test_cases = vec![
            TestStruct {
                name: "empty".to_string(),
                frame: Frame {
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "file".to_string(),
                frame: Frame {
                    function: Some("/var/www/http/webroot/index.php".to_string()),
                    file: Some("/var/www/http/webroot/index.php".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "src".to_string(),
                frame: Frame {
                    function: Some("App\\Middleware\\SentryMiddleware::process".to_string()),
                    file: Some("/var/www/http/src/Middleware/SentryMiddleware.php".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "vendor".to_string(),
                frame: Frame {
                    function: Some("Cake\\Http\\Client::send".to_string()),
                    path: Some(
                        "/var/www/http/vendor/cakephp/cakephp/src/Http/Client.php".to_string(),
                    ),
                    ..Default::default()
                },
                is_application: false,
            },
        ];
        for test_case in test_cases {
            let is_app = test_case.frame.is_php_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_trim_package() {
        use super::trim_package;
        struct TestStruct {
            pkg: String,
            expected: String,
        }
        let test_cases = [
            TestStruct {
                pkg: "/System/Library/PrivateFrameworks/UIKitCore.framework/UIKitCore".to_string(),
                expected: "UIKitCore".to_string(),
            },
            TestStruct {
                // // strips the .dylib
                pkg: "/usr/lib/system/libsystem_pthread.dylib".to_string(),
                expected: "libsystem_pthread".to_string(),
            },
            TestStruct {
                pkg: "/lib/x86_64-linux-gnu/libc.so.6".to_string(),
                expected: "libc.so.6".to_string(),
            },
            TestStruct {
                pkg: "/foo".to_string(),
                expected: "foo".to_string(),
            },
            TestStruct {
                pkg: "/foo/".to_string(),
                expected: "foo".to_string(),
            },
            TestStruct {
                pkg: "/foo//".to_string(),
                expected: "/foo//".to_string(),
            },
            TestStruct {
                pkg: "C:\\WINDOWS\\SYSTEM32\\ntdll.dll".to_string(),
                expected: "ntdll".to_string(),
            },
            TestStruct {
                pkg: "C:\\Program Files\\Foo 2023.3\\bin\\foo.exe".to_string(),
                expected: "foo".to_string(),
            },
        ];
        for test_case in test_cases {
            let result = trim_package(test_case.pkg.as_ref());
            assert_eq!(
                result, test_case.expected,
                "expected: {} - got: {}",
                test_case.expected, result
            );
        }
    }

    #[test]
    fn test_write_to_hash() {
        use fnv_rs::Fnv64;

        struct TestStruct<'a> {
            name: String,
            bytes: &'a [u8],
            frame: Frame,
        }

        let test_cases = [
            TestStruct {
                name: "unknown frame".to_string(),
                bytes: "--".as_bytes(),
                frame: Frame::default(),
            },
            TestStruct {
                name: "prefers function module over package".to_string(),
                bytes: "foo-".as_bytes(),
                frame: Frame {
                    module: Some("foo".to_string()),
                    package: Some("/bar/bar".to_string()),
                    file: Some("baz".to_string()),
                    ..Default::default()
                },
            },
            TestStruct {
                name: "prefers package over file".to_string(),
                bytes: "bar-".as_bytes(),
                frame: Frame {
                    package: Some("/bar/bar".to_string()),
                    file: Some("baz".to_string()),
                    ..Default::default()
                },
            },
            TestStruct {
                name: "prefers file over nothing".to_string(),
                bytes: "baz-".as_bytes(),
                frame: Frame {
                    file: Some("baz".to_string()),
                    ..Default::default()
                },
            },
            TestStruct {
                name: "uses function name".to_string(),
                bytes: "-qux".as_bytes(),
                frame: Frame {
                    function: Some("qux".to_string()),
                    ..Default::default()
                },
            },
            TestStruct {
                name: "native unknown frame".to_string(),
                bytes: "--0x123456789".as_bytes(),
                frame: Frame {
                    instruction_addr: Some("0x123456789".to_string()),
                    ..Default::default()
                },
            },
        ];

        for test_case in test_cases {
            let mut h1 = Fnv64::default();
            h1.write(test_case.bytes);

            let mut h2 = Fnv64::default();
            test_case.frame.write_to_hash(&mut h2);

            let s1 = h1.finish();
            let s2 = h2.finish();

            assert_eq!(
                s1, s2,
                "test: {}. \nexpected: {} - got: {}",
                test_case.name, s1, s2
            );
        }
    }

    #[test]
    fn test_is_android_application_frame_by_package() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        // Native/library frames are classified by their package location.
        let test_cases = vec![
            TestStruct {
                name: "no package -> unresolved, application".to_string(),
                frame: Frame {
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "empty package -> application".to_string(),
                frame: Frame {
                    package: Some("".to_string()),
                    ..Default::default()
                },
                is_application: true,
            },
            TestStruct {
                name: "system lib".to_string(),
                frame: Frame {
                    package: Some("system/lib64/libhwui.so".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "system framework jar with leading slash".to_string(),
                frame: Frame {
                    package: Some("/system/framework/framework.jar".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "vendor lib".to_string(),
                frame: Frame {
                    package: Some("vendor/lib64/libGLESv2_enc.so".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "apex runtime lib".to_string(),
                frame: Frame {
                    package: Some("apex/com.android.runtime/lib64/bionic/libc.so".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "vdso".to_string(),
                frame: Frame {
                    package: Some("[vdso]".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
            TestStruct {
                name: "dalvik heap region".to_string(),
                frame: Frame {
                    package: Some("[anon:dalvik-LinearAlloc]".to_string()),
                    ..Default::default()
                },
                is_application: false,
            },
        ];
        for test_case in test_cases {
            let is_app = test_case.frame.is_android_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_is_android_application_frame_by_module() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_application: bool,
        }

        // JVM frames are classified by their class namespace, taken from `module`
        // (or `function` when the module is absent, e.g. deobfuscated frames). The
        // JIT code cache package must not sway the result.
        let jit = "[anon_shmem:dalvik-jit-code-cache]";
        let by_module = |module: &str| Frame {
            package: Some(jit.to_string()),
            module: Some(module.to_string()),
            ..Default::default()
        };
        let by_function = |function: &str| Frame {
            function: Some(function.to_string()),
            ..Default::default()
        };

        let test_cases = vec![
            // platform / runtime / stdlib -> system
            TestStruct {
                name: "java stdlib".to_string(),
                frame: by_module("java.util.HashMap"),
                is_application: false,
            },
            TestStruct {
                name: "android platform".to_string(),
                frame: by_module("android.view.View"),
                is_application: false,
            },
            TestStruct {
                name: "com.android internal".to_string(),
                frame: by_module("com.android.internal.os.ZygoteInit"),
                is_application: false,
            },
            TestStruct {
                name: "libcore".to_string(),
                frame: by_module("libcore.io.Linux"),
                is_application: false,
            },
            TestStruct {
                name: "sun".to_string(),
                frame: by_module("sun.nio.ch.FileChannelImpl"),
                is_application: false,
            },
            TestStruct {
                name: "kotlin stdlib".to_string(),
                frame: by_module("kotlin.collections.ArraysKt"),
                is_application: false,
            },
            // the Sentry SDK is our own instrumentation -> system
            TestStruct {
                name: "sentry sdk".to_string(),
                frame: by_module("io.sentry.transport.AsyncHttpTransport"),
                is_application: false,
            },
            // the app and its bundled libraries (androidx, third-party) -> application
            TestStruct {
                name: "app code".to_string(),
                frame: by_module("com.example.MainActivity"),
                is_application: true,
            },
            TestStruct {
                name: "androidx ships with the app".to_string(),
                frame: by_module("androidx.recyclerview.widget.RecyclerView"),
                is_application: true,
            },
            TestStruct {
                name: "bundled gson".to_string(),
                frame: by_module("com.google.gson.Gson"),
                is_application: true,
            },
            TestStruct {
                name: "bundled rxjava".to_string(),
                frame: by_module("rx.internal.operators.OperatorMap"),
                is_application: true,
            },
            // FQN in `function` when `module` is absent (deobfuscated frames)
            TestStruct {
                name: "system fqn in function".to_string(),
                frame: by_function("android.os.Handler.dispatchMessage"),
                is_application: false,
            },
            TestStruct {
                name: "app fqn in function".to_string(),
                frame: by_function("com.example.HomeActivity.onCreate"),
                is_application: true,
            },
            // A bare method name (no namespace, e.g. an unattributed native
            // symbol) matches no system prefix and defaults to application.
            TestStruct {
                name: "bare method name in function".to_string(),
                frame: by_function("malloc"),
                is_application: true,
            },
        ];
        for test_case in test_cases {
            let is_app = test_case.frame.is_android_application_frame();
            assert_eq!(
                is_app, test_case.is_application,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_application, is_app
            );
        }
    }

    #[test]
    fn test_is_synthetic_android_frame() {
        struct TestStruct {
            name: String,
            frame: Frame,
            is_synthetic: bool,
        }

        let test_cases = vec![
            TestStruct {
                name: "libart runtime".to_string(),
                frame: Frame {
                    package: Some("apex/com.android.art/lib64/libart.so".to_string()),
                    ..Default::default()
                },
                is_synthetic: true,
            },
            TestStruct {
                name: "libart with leading slash".to_string(),
                frame: Frame {
                    package: Some("/apex/com.android.art/lib64/libart.so".to_string()),
                    ..Default::default()
                },
                is_synthetic: true,
            },
            TestStruct {
                name: "other lib in the art apex".to_string(),
                frame: Frame {
                    package: Some("apex/com.android.art/lib64/libjavacore.so".to_string()),
                    ..Default::default()
                },
                is_synthetic: false,
            },
            TestStruct {
                name: "unrelated system lib".to_string(),
                frame: Frame {
                    package: Some("system/lib64/libhwui.so".to_string()),
                    ..Default::default()
                },
                is_synthetic: false,
            },
            TestStruct {
                name: "anonymous VMA without a function name".to_string(),
                frame: Frame {
                    package: Some("<anonymous:7f8a0000>".to_string()),
                    ..Default::default()
                },
                is_synthetic: true,
            },
            TestStruct {
                name: "anonymous VMA with an empty function name".to_string(),
                frame: Frame {
                    package: Some("<anonymous:7f8a0000>".to_string()),
                    function: Some("".to_string()),
                    ..Default::default()
                },
                is_synthetic: true,
            },
            TestStruct {
                name: "anonymous VMA with a resolved function name".to_string(),
                frame: Frame {
                    package: Some("<anonymous:7f8a0000>".to_string()),
                    function: Some("doWork".to_string()),
                    ..Default::default()
                },
                is_synthetic: false,
            },
            TestStruct {
                name: "no package".to_string(),
                frame: Frame {
                    ..Default::default()
                },
                is_synthetic: false,
            },
        ];
        for test_case in test_cases {
            let is_synthetic = test_case.frame.is_synthetic_android_frame();
            assert_eq!(
                is_synthetic, test_case.is_synthetic,
                "test: {}\nexpected: {} - got: {}",
                test_case.name, test_case.is_synthetic, is_synthetic
            );
        }
    }

    #[test]
    fn test_set_in_app_android_computes_when_absent() {
        // When in_app is absent, set_in_app populates it from the package/module.
        // Native frames are classified by their package.
        let mut sys = Frame {
            platform: Some("native".to_string()),
            package: Some("system/lib64/libhwui.so".to_string()),
            ..Default::default()
        };
        sys.normalize("android");
        assert_eq!(sys.in_app, Some(false));

        // Untyped frames (platform absent) default to the "android" platform and
        // are still classified — this is how relay delivers JVM frames.
        let mut untyped_app = Frame {
            function: Some("com.example.MainActivity.onCreate".to_string()),
            ..Default::default()
        };
        untyped_app.normalize("android");
        assert_eq!(untyped_app.in_app, Some(true));

        let mut untyped_sys = Frame {
            module: Some("android.os.Handler".to_string()),
            ..Default::default()
        };
        untyped_sys.normalize("android");
        assert_eq!(untyped_sys.in_app, Some(false));

        // JVM frames ("java") are classified by their class module.
        let mut framework = Frame {
            platform: Some("java".to_string()),
            module: Some("android.os.Handler".to_string()),
            ..Default::default()
        };
        framework.normalize("android");
        assert_eq!(framework.in_app, Some(false));
    }

    #[test]
    fn test_set_in_app_android_preserves_existing_in_app() {
        // Relay already computes in_app; an existing value is trusted and never
        // overridden, mirroring the legacy android format (in_app.or_else(compute)).
        // This holds even when our own rules would classify the frame differently.
        let mut app = Frame {
            platform: Some("native".to_string()),
            function: Some("com.example.MainActivity.onCreate".to_string()),
            in_app: Some(false), // our rules would say true, but relay wins
            ..Default::default()
        };
        app.normalize("android");
        assert_eq!(app.in_app, Some(false));

        let mut sys = Frame {
            module: Some("android.os.Handler".to_string()),
            in_app: Some(true), // our rules would say false, but relay wins
            ..Default::default()
        };
        sys.normalize("android");
        assert_eq!(sys.in_app, Some(true));
    }
}
