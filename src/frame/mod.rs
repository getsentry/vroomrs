mod python_std_lib;

use std::{collections::HashSet, hash::Hasher};

use fnv_rs::Fnv64;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::types::Platform;

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
    pub platform: Option<Platform>,

    #[serde(skip)]
    pub is_react_native: bool,
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

    fn set_in_app(&mut self, p: Platform) {
        // for react-native the in_app field seems to be messed up most of the times,
        // with system libraries and other frames that are clearly system frames
        // labelled as `in_app`.
        // This is likely because RN uses static libraries which are bundled into the app binary.
        // When symbolicated they are marked in_app.
        //
        // For this reason, for react-native app (p.Platform != f.Platform), we skip the f.InApp!=nil
        // check as this field would be highly unreliable, and rely on our rules instead
        if self.in_app.is_some() && self.platform.is_some_and(|fp| p == fp) {
            return;
        }

        let is_application = match self.platform.unwrap() {
            Platform::Node => self.is_node_application_frame(),
            Platform::JavaScript => self.is_javascript_application_frame(),
            Platform::Cocoa => self.is_cocoa_application_frame(),
            Platform::Rust => self.is_rust_application_frame(),
            Platform::Python => self.is_python_application_frame(),
            Platform::Php => self.is_php_application_frame(),
            _ => false,
        };

        self.in_app = Some(is_application);
    }

    #[allow(dead_code)]
    fn is_in_app(&self) -> bool {
        self.in_app.unwrap_or(false)
    }

    fn set_platform(&mut self, p: Platform) {
        if self.platform.is_none() {
            self.platform = Some(p);
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

    pub fn normalize(&mut self, p: Platform) {
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

    pub fn fingerprint(&self) -> u32 {
        let mut hasher = Fnv64::default();
        hasher.write(self.module_or_package().as_bytes());
        hasher.write(":".as_bytes());
        hasher.write(self.function.as_deref().unwrap_or_default().as_bytes());

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
}
