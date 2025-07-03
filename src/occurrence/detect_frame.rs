use once_cell::sync::Lazy;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use crate::{
    frame::Frame,
    nodetree::Node,
    types::{CallTreesU64, Platform, ProfileInterface},
    MAX_STACK_DEPTH,
};

pub(crate) const BASE64_DECODE: &str = "base64_decode";
pub(crate) const BASE64_ENCODE: &str = "base64_encode";
pub(crate) const COMPRESSION: &str = "compression";
pub(crate) const CORE_DATA_BLOCK: &str = "core_data_block";
pub(crate) const CORE_DATA_MERGE: &str = "core_data_merge";
pub(crate) const CORE_DATA_READ: &str = "core_data_read";
pub(crate) const CORE_DATA_WRITE: &str = "core_data_write";
pub(crate) const DECOMPRESSION: &str = "decompression";
pub(crate) const FILE_READ: &str = "file_read";
pub(crate) const FILE_WRITE: &str = "file_write";
pub(crate) const HTTP: &str = "http";
pub(crate) const IMAGE_DECODE: &str = "image_decode";
pub(crate) const IMAGE_ENCODE: &str = "image_encode";
pub(crate) const JSON_DECODE: &str = "json_decode";
pub(crate) const JSON_ENCODE: &str = "json_encode";
pub(crate) const ML_MODEL_INFERENCE: &str = "ml_model_inference";
pub(crate) const ML_MODEL_LOAD: &str = "ml_model_load";
pub(crate) const REGEX: &str = "regex";
pub(crate) const SQL: &str = "sql";
pub(crate) const SOURCE_CONTEXT: &str = "source_context";
pub(crate) const THREAD_WAIT: &str = "thread_wait";
pub(crate) const VIEW_INFLATION: &str = "view_inflation";
pub(crate) const VIEW_LAYOUT: &str = "view_layout";
pub(crate) const VIEW_RENDER: &str = "view_render";
pub(crate) const VIEW_UPDATE: &str = "view_update";
pub(crate) const XPC: &str = "xpc";

/// Trait for frame detection options with configurable behavior.
pub trait DetectFrameOptions {
    /// Returns whether to only check the active thread.
    fn only_check_active_thread(&self) -> bool;

    /// Checks a node and returns information about it if it matches detection criteria.
    /// Returns None if the node doesn't match the criteria.
    fn check_node(&self, node: &Node) -> Option<NodeInfo>;
}

/// Options for detecting exact frames in profiling data.
#[derive(Debug, Clone, Default)]
pub struct DetectExactFrameOptions {
    /// Whether to only consider the active thread
    pub active_thread_only: bool,

    /// Minimum duration threshold for frame detection
    pub duration_threshold: Duration,

    /// Map of package names to functions and their categories
    pub functions_by_package: HashMap<&'static str, HashMap<&'static str, &'static str>>,

    /// Minimum number of samples in which we need to detect the frame
    /// in order to create an occurrence
    pub sample_threshold: u32,
}

/// Options for detecting Android frames in profiling data.
#[derive(Debug, Clone)]
pub struct DetectAndroidFrameOptions {
    /// Whether to only consider the active thread
    pub active_thread_only: bool,

    /// Minimum duration threshold for frame detection
    pub duration_threshold: Duration,

    /// Map of package names to functions and their categories
    pub functions_by_package: HashMap<&'static str, HashMap<&'static str, &'static str>>,

    /// Minimum number of samples in which we need to detect the frame
    /// in order to create an occurrence
    pub sample_threshold: u32,
}

/// Key for identifying a specific node in the call tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeKey {
    /// Package name
    pub package: String,

    /// Function name
    pub function: String,
}

/// Information about a detected node and its context.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
    /// Category classification of the node
    pub category: String,

    /// The actual node from the call tree
    pub node: Node,

    /// Stack trace leading to this node
    pub stack_trace: Vec<Frame>,
}

impl DetectFrameOptions for DetectExactFrameOptions {
    fn only_check_active_thread(&self) -> bool {
        self.active_thread_only
    }

    fn check_node(&self, node: &Node) -> Option<NodeInfo> {
        // Check if we have a list of functions associated to the package.
        let functions = self.functions_by_package.get(node.package.as_str())?;

        // Check if we need to detect that function.
        let category = functions.get(node.name.as_str())?;

        // Check if it's above the duration threshold.
        let duration_threshold_ns = self.duration_threshold.as_nanos() as u64;
        if node.duration_ns < duration_threshold_ns {
            return None;
        }

        // Check if it's above the sample threshold.
        if node.sample_count < self.sample_threshold as u64 {
            return None;
        }

        // Create NodeInfo with the category and a copy of the node (without children).
        let mut node_copy = node.clone();
        node_copy.children.clear();

        Some(NodeInfo {
            category: category.to_string(),
            node: node_copy,
            stack_trace: Vec::new(), // Initialize empty stack trace
        })
    }
}

impl DetectFrameOptions for DetectAndroidFrameOptions {
    fn only_check_active_thread(&self) -> bool {
        self.active_thread_only
    }

    fn check_node(&self, node: &Node) -> Option<NodeInfo> {
        // Check if we have a list of functions associated to the package.
        let functions = self.functions_by_package.get(node.package.as_str())?;

        // Android frame names contain the deobfuscated signature.
        // Here we strip away the argument and return types to only
        // match on the package + function name.
        let name = if let Some(paren_pos) = node.name.find('(') {
            &node.name[..paren_pos]
        } else {
            &node.name
        };

        // Check if we need to detect that function.
        let category = functions.get(name)?;

        // Check if it's above the duration threshold.
        let duration_threshold_ns = self.duration_threshold.as_nanos() as u64;
        if node.duration_ns < duration_threshold_ns {
            return None;
        }

        // Check if it's above the sample threshold.
        if node.sample_count < self.sample_threshold as u64 {
            return None;
        }

        // Create NodeInfo with the category and a copy of the node (without children).
        let mut node_copy = node.clone();
        node_copy.children.clear();

        Some(NodeInfo {
            category: category.to_string(),
            node: node_copy,
            stack_trace: Vec::new(), // Initialize empty stack trace
        })
    }
}

/// Platform-specific frame detection job configurations.
pub static DETECT_FRAME_JOBS: Lazy<
    HashMap<Platform, Vec<Box<dyn DetectFrameOptions + Send + Sync>>>,
> = Lazy::new(|| {
    HashMap::from([
        // Node.js platform
        (Platform::Node, vec![
            Box::new(DetectExactFrameOptions {
                active_thread_only: true,
                duration_threshold: Duration::from_millis(0),
                sample_threshold: 1,
                functions_by_package: HashMap::from([
                    ("node:fs", HashMap::from([
                        ("accessSync", FILE_READ),
                        ("appendFileSync", FILE_READ),
                        ("chmodSync", FILE_READ),
                        ("chownSync", FILE_READ),
                        ("closeSync", FILE_READ),
                        ("copyFileSync", FILE_READ),
                        ("cpSync", FILE_READ),
                        ("existsSync", FILE_READ),
                        ("fchmodSync", FILE_READ),
                        ("fchownSync", FILE_READ),
                        ("fdatasyncSync", FILE_READ),
                        ("fstatSync", FILE_READ),
                        ("fsyncSync", FILE_READ),
                        ("ftruncateSync", FILE_READ),
                        ("futimesSync", FILE_READ),
                        ("lchmodSync", FILE_READ),
                        ("lchownSync", FILE_READ),
                        ("linkSync", FILE_READ),
                        ("lstatSync", FILE_READ),
                        ("lutimesSync", FILE_READ),
                        ("mkdirSync", FILE_READ),
                        ("mkdtempSync", FILE_READ),
                        ("openSync", FILE_READ),
                        ("opendirSync", FILE_READ),
                        ("readFileSync", FILE_READ),
                        ("readSync", FILE_READ),
                        ("readdirSync", FILE_READ),
                        ("readlinkSync", FILE_READ),
                        ("readvSync", FILE_READ),
                        ("realpathSync", FILE_READ),
                        ("realpathSync.native", FILE_READ),
                        ("renameSync", FILE_READ),
                        ("rmSync", FILE_READ),
                        ("rmdirSync", FILE_READ),
                        ("statSync", FILE_READ),
                        ("symlinkSync", FILE_READ),
                        ("truncateSync", FILE_READ),
                        ("unlinkSync", FILE_READ),
                        ("utimesSync", FILE_READ),
                        ("writeFileSync", FILE_READ),
                        ("writeSync", FILE_READ),
                        ("writevSync", FILE_READ),
                    ]))
                ]),
            }) as Box<dyn DetectFrameOptions + Send + Sync>,
            Box::new(DetectExactFrameOptions {
                active_thread_only: false,
                duration_threshold: Duration::from_millis(100),
                sample_threshold: 1,
                functions_by_package: HashMap::from([
                    ("", HashMap::from([
                        ("addSourceContext", SOURCE_CONTEXT),
                        ("addSourceContextToFrames", SOURCE_CONTEXT),
                    ]))
                ]),
            }) as Box<dyn DetectFrameOptions + Send + Sync>,
        ]),
        // Cocoa platform
        (Platform::Cocoa, vec![
            Box::new(DetectExactFrameOptions {
                active_thread_only: true,
                duration_threshold: Duration::from_millis(16),
                sample_threshold: 4,
                functions_by_package: HashMap::from([
                    ("AppleJPEG", HashMap::from([
                        ("applejpeg_decode_image_all", IMAGE_DECODE),
                    ])),
                    ("AttributeGraph", HashMap::from([
                        ("AG::LayoutDescriptor::make_layout(AG::swift::metadata const*, AGComparisonMode, AG::LayoutDescriptor::HeapMode)", VIEW_LAYOUT),
                    ])),
                    ("CoreData", HashMap::from([
                        ("-[NSManagedObjectContext countForFetchRequest:error:]", CORE_DATA_READ),
                        ("-[NSManagedObjectContext executeFetchRequest:error:]", CORE_DATA_READ),
                        ("-[NSManagedObjectContext executeRequest:error:]", CORE_DATA_READ),
                        ("-[NSManagedObjectContext mergeChangesFromContextDidSaveNotification:]", CORE_DATA_MERGE),
                        ("-[NSManagedObjectContext obtainPermanentIDsForObjects:error:]", CORE_DATA_WRITE),
                        ("-[NSManagedObjectContext performBlockAndWait:]", CORE_DATA_BLOCK),
                        ("-[NSManagedObjectContext save:]", CORE_DATA_WRITE),
                        ("NSManagedObjectContext.fetch<A>(NSFetchRequest<A>)", CORE_DATA_READ),
                    ])),
                    ("CoreFoundation", HashMap::from([
                        ("CFReadStreamRead", FILE_READ),
                        ("CFURLConnectionSendSynchronousRequest", HTTP),
                        ("CFURLCreateData", FILE_READ),
                        ("CFURLCreateDataAndPropertiesFromResource", FILE_READ),
                        ("CFURLWriteDataAndPropertiesToResource", FILE_WRITE),
                        ("CFWriteStreamWrite", FILE_WRITE),
                    ])),
                    ("CoreML", HashMap::from([
                        ("+[MLModel modelWithContentsOfURL:configuration:error:]", ML_MODEL_LOAD),
                        ("-[MLNeuralNetworkEngine predictionFromFeatures:options:error:]", ML_MODEL_INFERENCE),
                    ])),
                    ("Foundation", HashMap::from([
                        ("+[NSJSONSerialization JSONObjectWithStream:options:error:]", JSON_DECODE),
                        ("+[NSJSONSerialization writeJSONObject:toStream:options:error:]", JSON_ENCODE),
                        ("+[NSRegularExpression regularExpressionWithPattern:options:error:]", REGEX),
                        ("-[NSRegularExpression initWithPattern:options:error:]", REGEX),
                        ("-[NSRegularExpression(NSMatching) enumerateMatchesInString:options:range:usingBlock:]", REGEX),
                        ("Regex.firstMatch(in: String)", REGEX),
                        ("Regex.wholeMatch(in: String)", REGEX),
                        ("Regex.prefixMatch(in: String)", REGEX),
                        ("+[NSURLConnection sendSynchronousRequest:returningResponse:error:]", HTTP),
                        ("-[NSData(NSData) initWithContentsOfMappedFile:]", FILE_READ),
                        ("-[NSData(NSData) initWithContentsOfURL:]", FILE_READ),
                        ("-[NSData(NSData) initWithContentsOfURL:options:maxLength:error:]", FILE_READ),
                        ("-[NSData(NSData) writeToFile:atomically:]", FILE_WRITE),
                        ("-[NSData(NSData) writeToFile:atomically:error:]", FILE_WRITE),
                        ("-[NSData(NSData) writeToFile:options:error:]", FILE_WRITE),
                        ("-[NSData(NSData) writeToURL:atomically:]", FILE_WRITE),
                        ("-[NSData(NSData) writeToURL:options:error:]", FILE_WRITE),
                        ("-[NSFileManager contentsAtPath:]", FILE_READ),
                        ("-[NSFileManager createFileAtPath:contents:attributes:]", FILE_WRITE),
                        ("-[NSISEngine performModifications:withUnsatisfiableConstraintsHandler:]", VIEW_LAYOUT),
                        ("@nonobjc NSData.init(contentsOf: URL, options: NSDataReadingOptions)", FILE_READ),
                        ("Data.init(contentsOf: __shared URL, options: NSDataReadingOptions)", FILE_READ),
                        ("JSONDecoder.decode<A>(_: A.Type, from: Any)", JSON_DECODE),
                        ("JSONDecoder.decode<A>(_: A.Type, from: Data)", JSON_DECODE),
                        ("JSONDecoder.decode<A>(_: A.Type, jsonData: Data, logErrors: Bool)", JSON_DECODE),
                        ("-[_NSJSONReader parseData:options:error:]", JSON_ENCODE),
                        ("JSONEncoder.encode<A>(A)", JSON_ENCODE),
                        ("NSFileManager.contents(atURL: URL)", FILE_READ),
                    ])),
                    ("ImageIO", HashMap::from([
                        ("DecodeImageData", IMAGE_DECODE),
                        ("DecodeImageStream", IMAGE_DECODE),
                        ("GIFReadPlugin::DoDecodeImageData(IIOImageReadSession*, GlobalGIFInfo*, ReadPluginData const&, GIFPluginData const&, unsigned char*, unsigned long, std::__1::shared_ptr<GIFBufferInfo>, long*)", IMAGE_DECODE),
                        ("IIOImageProviderInfo::CopyImageBlockSetWithOptions(void*, CGImageProvider*, CGRect, CGSize, __CFDictionary const*)", IMAGE_DECODE),
                        ("LZWDecode", IMAGE_DECODE),
                        ("NeXTDecode", IMAGE_DECODE),
                        ("PNGReadPlugin::DecodeFrameStandard(IIOImageReadSession*, ReadPluginData const&, PNGPluginData const&, IIODecodeFrameParams&)", IMAGE_DECODE),
                        ("VP8Decode", IMAGE_DECODE),
                        ("VP8DecodeMB", IMAGE_DECODE),
                        ("WebPDecode", IMAGE_DECODE),
                        ("jpeg_huff_decode", IMAGE_DECODE),
                    ])),
                    ("libcompression.dylib", HashMap::from([
                        ("BrotliDecoderDecompress", COMPRESSION),
                        ("brotli_encode_buffer", COMPRESSION),
                        ("lz4_decode", COMPRESSION),
                        ("lz4_decode_asm", COMPRESSION),
                        ("lzfseDecode", COMPRESSION),
                        ("lzfseEncode", COMPRESSION),
                        ("lzfseStreamDecode", COMPRESSION),
                        ("lzfseStreamEncode", COMPRESSION),
                        ("lzvnDecode", COMPRESSION),
                        ("lzvnEncode", COMPRESSION),
                        ("lzvnStreamDecode", COMPRESSION),
                        ("lzvnStreamEncode", COMPRESSION),
                        ("zlibDecodeBuffer", COMPRESSION),
                        ("zlib_decode_buffer", COMPRESSION),
                        ("zlib_encode_buffer", COMPRESSION),
                    ])),
                    ("libsqlite3.dylib", HashMap::from([
                        ("sqlite3_blob_read", SQL),
                        ("sqlite3_column_blob", SQL),
                        ("sqlite3_column_bytes", SQL),
                        ("sqlite3_column_double", SQL),
                        ("sqlite3_column_int", SQL),
                        ("sqlite3_column_int64", SQL),
                        ("sqlite3_column_text", SQL),
                        ("sqlite3_column_text16", SQL),
                        ("sqlite3_column_value", SQL),
                        ("sqlite3_step", SQL),
                        ("sqlite3_value_blob", SQL),
                        ("sqlite3_value_double", SQL),
                        ("sqlite3_value_int", SQL),
                        ("sqlite3_value_int64", SQL),
                        ("sqlite3_value_pointer", SQL),
                        ("sqlite3_value_text", SQL),
                        ("sqlite3_value_text16", SQL),
                        ("sqlite3_value_text16be", SQL),
                        ("sqlite3_value_text16le", SQL),
                    ])),
                    ("libswiftCoreData.dylib", HashMap::from([
                        ("NSManagedObjectContext.count<A>(for: NSFetchRequest<A>)", CORE_DATA_READ),
                        ("NSManagedObjectContext.fetch<A>(NSFetchRequest<A>)", CORE_DATA_READ),
                        ("NSManagedObjectContext.perform<A>(schedule: NSManagedObjectContext.ScheduledTaskType, _: ())", CORE_DATA_BLOCK),
                    ])),
                    ("libswiftFoundation.dylib", HashMap::from([
                        ("__JSONDecoder.decode<A>(A.Type)", JSON_DECODE),
                        ("__JSONEncoder.encode<A>(A)", JSON_ENCODE),
                    ])),
                    ("libsystem_c.dylib", HashMap::from([
                        ("__fread", FILE_READ),
                        ("fread", FILE_READ),
                    ])),
                    ("libxpc.dylib", HashMap::from([
                        ("xpc_connection_send_message_with_reply_sync", XPC),
                    ])),
                    ("SwiftUI", HashMap::from([
                        ("UnaryLayoutEngine.sizeThatFits(_ProposedSize)", VIEW_LAYOUT),
                        ("ViewRendererHost.render(interval: Double, updateDisplayList: Bool)", VIEW_RENDER),
                        ("ViewRendererHost.updateViewGraph<A>(body: (ViewGraph))", VIEW_UPDATE),
                    ])),
                    ("UIKit", HashMap::from([
                        ("-[_UIPathLazyImageAsset imageWithConfiguration:]", IMAGE_DECODE),
                        ("-[UINib instantiateWithOwner:options:]", VIEW_INFLATION),
                    ])),
                ]),
            }) as Box<dyn DetectFrameOptions + Send + Sync>,
        ]),
        // Android platform
        (Platform::Android, vec![
            Box::new(DetectAndroidFrameOptions {
                active_thread_only: true,
                duration_threshold: Duration::from_millis(40),
                sample_threshold: 1,
                functions_by_package: HashMap::from([
                    ("com.google.gson", HashMap::from([
                        ("com.google.gson.Gson.fromJson", JSON_DECODE),
                        ("com.google.gson.Gson.toJson", JSON_ENCODE),
                        ("com.google.gson.Gson.toJsonTree", JSON_ENCODE),
                    ])),
                    ("org.json", HashMap::from([
                        ("org.json.JSONArray.get", JSON_DECODE),
                        ("org.json.JSONArray.opt", JSON_DECODE),
                        ("org.json.JSONArray.writeTo", JSON_ENCODE),
                        ("org.json.JSONObject.checkName", JSON_DECODE),
                        ("org.json.JSONObject.get", JSON_DECODE),
                        ("org.json.JSONObject.opt", JSON_DECODE),
                        ("org.json.JSONObject.put", JSON_ENCODE),
                        ("org.json.JSONObject.putOpt", JSON_ENCODE),
                        ("org.json.JSONObject.remove", JSON_ENCODE),
                        ("org.json.JSONObject.writeTo", JSON_ENCODE),
                    ])),
                    ("android.content.res", HashMap::from([
                        ("android.content.res.AssetManager.open", FILE_READ),
                        ("android.content.res.AssetManager.openFd", FILE_READ),
                    ])),
                    ("java.io", HashMap::from([
                        ("java.io.File.canExecute", FILE_READ),
                        ("java.io.File.canRead", FILE_READ),
                        ("java.io.File.canWrite", FILE_READ),
                        ("java.io.File.createNewFile", FILE_WRITE),
                        ("java.io.File.createTempFile", FILE_WRITE),
                        ("java.io.File.delete", FILE_WRITE),
                        ("java.io.File.exists", FILE_READ),
                        ("java.io.File.length", FILE_READ),
                        ("java.io.File.mkdir", FILE_WRITE),
                        ("java.io.File.mkdirs", FILE_WRITE),
                        ("java.io.File.renameTo", FILE_WRITE),
                        ("java.io.FileInputStream.open", FILE_READ),
                        ("java.io.FileInputStream.read", FILE_READ),
                        ("java.io.FileOutputStream.open", FILE_READ),
                        ("java.io.FileOutputStream.write", FILE_WRITE),
                        ("java.io.RandomAccessFile.readBytes", FILE_READ),
                        ("java.io.RandomAccessFile.writeBytes", FILE_WRITE),
                    ])),
                    ("okio", HashMap::from([
                        ("okio.Buffer.read", FILE_READ),
                        ("okio.Buffer.readByte", FILE_READ),
                        ("okio.Buffer.write", FILE_WRITE),
                        ("okio.Buffer.writeAll", FILE_WRITE),
                    ])),
                    ("android.graphics", HashMap::from([
                        ("android.graphics.BitmapFactory.decodeByteArray", IMAGE_DECODE),
                        ("android.graphics.BitmapFactory.decodeFile", IMAGE_DECODE),
                        ("android.graphics.BitmapFactory.decodeFileDescriptor", IMAGE_DECODE),
                        ("android.graphics.BitmapFactory.decodeStream", IMAGE_DECODE),
                    ])),
                    ("android.database.sqlite", HashMap::from([
                        ("android.database.sqlite.SQLiteDatabase.insertWithOnConflict", SQL),
                        ("android.database.sqlite.SQLiteDatabase.open", SQL),
                        ("android.database.sqlite.SQLiteDatabase.query", SQL),
                        ("android.database.sqlite.SQLiteDatabase.rawQueryWithFactory", SQL),
                        ("android.database.sqlite.SQLiteStatement.execute", SQL),
                        ("android.database.sqlite.SQLiteStatement.executeInsert", SQL),
                        ("android.database.sqlite.SQLiteStatement.executeUpdateDelete", SQL),
                        ("android.database.sqlite.SQLiteStatement.simpleQueryForLong", SQL),
                    ])),
                    ("androidx.room", HashMap::from([
                        ("androidx.room.RoomDatabase.query", SQL),
                    ])),
                    ("java.util.zip", HashMap::from([
                        ("java.util.zip.Deflater.deflate", COMPRESSION),
                        ("java.util.zip.Deflater.deflateBytes", COMPRESSION),
                        ("java.util.zip.DeflaterOutputStream.write", COMPRESSION),
                        ("java.util.zip.GZIPInputStream.read", COMPRESSION),
                        ("java.util.zip.GZIPOutputStream.write", COMPRESSION),
                        ("java.util.zip.Inflater.inflate", COMPRESSION),
                        ("java.util.zip.Inflater.inflateBytes", COMPRESSION),
                    ])),
                    ("java.util", HashMap::from([
                        ("java.util.Base64$Decoder.decode", BASE64_DECODE),
                        ("java.util.Base64$Decoder.decode0", BASE64_DECODE),
                    ])),
                    ("java.util.regex", HashMap::from([
                        ("java.util.regex.Matcher.matches", REGEX),
                        ("java.util.regex.Matcher.find", REGEX),
                        ("java.util.regex.Matcher.lookingAt", REGEX),
                    ])),
                    ("kotlinx.coroutines", HashMap::from([
                        ("kotlinx.coroutines.AwaitAll.await", THREAD_WAIT),
                        ("kotlinx.coroutines.AwaitKt.awaitAll", THREAD_WAIT),
                        ("kotlinx.coroutines.BlockingCoroutine.joinBlocking", THREAD_WAIT),
                        ("kotlinx.coroutines.JobSupport.join", THREAD_WAIT),
                        ("kotlinx.coroutines.JobSupport.joinSuspend", THREAD_WAIT),
                    ])),
                ]),
            }) as Box<dyn DetectFrameOptions + Send + Sync>,
        ])
    ])
});

/// Detects frames in a call tree starting from the root node.
pub(crate) fn detect_frame_in_call_tree(
    node: &Rc<RefCell<Node>>,
    options: &dyn DetectFrameOptions,
    nodes: &mut HashMap<NodeKey, NodeInfo>,
) {
    let mut stack_trace: Vec<Frame> = Vec::with_capacity(MAX_STACK_DEPTH as usize);
    detect_frame_in_node(node, options, nodes, &mut stack_trace);
}

/// Recursively detects frames in a node and its children, building up a stack trace.
/// Returns Some(NodeInfo) if a matching node is found, None otherwise.
fn detect_frame_in_node(
    node: &Rc<RefCell<Node>>,
    options: &dyn DetectFrameOptions,
    nodes: &mut HashMap<NodeKey, NodeInfo>,
    stack_trace: &mut Vec<Frame>,
) -> Option<NodeInfo> {
    let borrowed_node = node.borrow();

    // Add current node's frame to stack trace
    stack_trace.push(borrowed_node.to_frame());

    // Recursively check all children first
    for child in &borrowed_node.children {
        if let Some(node_info) = detect_frame_in_node(child, options, nodes, stack_trace) {
            // Pop the current frame before returning (mimicking defer)
            stack_trace.pop();
            return Some(node_info);
        }
    }

    // Check if current node matches criteria after children
    let result = if let Some(mut node_info) = options.check_node(&borrowed_node) {
        let key = NodeKey {
            package: node_info.node.package.clone(),
            function: node_info.node.name.clone(),
        };

        // Only add if this key doesn't already exist
        if let std::collections::hash_map::Entry::Vacant(e) = nodes.entry(key) {
            node_info.stack_trace = stack_trace.clone();
            e.insert(node_info.clone());
        }

        Some(node_info)
    } else {
        None
    };

    stack_trace.pop();

    result
}

/// Detects occurrence of an issue based by matching frames of the profile on a list of frames.
/// This is the Rust equivalent of the Go detectFrame function.
pub fn detect_frame(
    profile: &dyn ProfileInterface,
    call_trees_per_thread_id: &CallTreesU64,
    options: &dyn DetectFrameOptions,
    occurrences: &mut Vec<super::Occurrence>,
) {
    // List nodes matching criteria
    let mut nodes: HashMap<NodeKey, NodeInfo> = HashMap::new();

    if options.only_check_active_thread() {
        let active_thread_id = profile.get_transaction().active_thread_id;
        if let Some(call_trees) = call_trees_per_thread_id.get(&active_thread_id) {
            for root in call_trees {
                detect_frame_in_call_tree(root, options, &mut nodes);
            }
        } else {
            return;
        }
    } else {
        for call_trees in call_trees_per_thread_id.values() {
            for root in call_trees {
                detect_frame_in_call_tree(root, options, &mut nodes);
            }
        }
    }

    // Create occurrences
    for node_info in nodes.into_values() {
        occurrences.push(super::new_occurrence(profile, node_info));
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

    use crate::{
        frame::Frame,
        nodetree::Node,
        occurrence::detect_frame::{
            detect_frame_in_call_tree, DetectAndroidFrameOptions, DetectExactFrameOptions,
            DetectFrameOptions, NodeInfo, NodeKey, FILE_READ, IMAGE_DECODE,
        },
    };

    use pretty_assertions::assert_eq;

    #[test]
    fn test_detect_frame_in_call_tree() {
        struct TestStruct {
            name: String,
            job: Box<dyn DetectFrameOptions>,
            node: Rc<RefCell<Node>>,
            want: HashMap<NodeKey, NodeInfo>,
        }

        let test_cases = [
            TestStruct {
                name: "Detect frame in call tree".to_string(),
                job: Box::new(DetectExactFrameOptions {
                    duration_threshold: Duration::from_millis(16),
                    functions_by_package: HashMap::from([
                        ("CoreFoundation", HashMap::from([
                            ("CFReadStreamRead", FILE_READ),
                        ]))
                    ]),
                    ..Default::default()
                }),
                node: Rc::new(RefCell::new(Node {
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    children: vec![
                                        Rc::new(RefCell::new(Node {
                                            children: vec![],
                                            duration_ns: 20_000_000, // 20 * time.Millisecond
                                            end_ns: 20_000_000,
                                            fingerprint: 0,
                                            is_application: false,
                                            line: Some(0),
                                            name: "CFReadStreamRead".to_string(),
                                            package: "CoreFoundation".to_string(),
                                            path: Some("path".to_string()),
                                            sample_count: 4,
                                            start_ns: 0,
                                            frame: Frame {
                                                function: Some("CFReadStreamRead".to_string()),
                                                in_app: Some(false),
                                                line: Some(0),
                                                package: Some("CoreFoundation".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                        }))
                                    ],
                                    duration_ns: 20_000_000,
                                    end_ns: 20_000_000,
                                    fingerprint: 0,
                                    is_application: true,
                                    line: Some(0),
                                    name: "child2-1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    sample_count: 1,
                                    start_ns: 0,
                                    frame: Frame {
                                        function: Some("child2-1".to_string()),
                                        in_app: Some(true),
                                        line: Some(0),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                }))
                            ],
                            duration_ns: 20_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-1".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 0,
                            frame: Frame {
                                function: Some("child1-1".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        })),
                        Rc::new(RefCell::new(Node {
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    children: vec![
                                        Rc::new(RefCell::new(Node {
                                            children: vec![],
                                            duration_ns: 5,
                                            end_ns: 10,
                                            fingerprint: 0,
                                            is_application: false,
                                            line: Some(0),
                                            name: "child3-1".to_string(),
                                            package: "package".to_string(),
                                            path: Some("path".to_string()),
                                            sample_count: 1,
                                            start_ns: 5,
                                            frame: Frame {
                                                function: Some("child3-1".to_string()),
                                                in_app: Some(false),
                                                line: Some(0),
                                                package: Some("package".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                        }))
                                    ],
                                    duration_ns: 5,
                                    end_ns: 10,
                                    fingerprint: 0,
                                    is_application: true,
                                    line: Some(0),
                                    name: "child2-1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    sample_count: 1,
                                    start_ns: 5,
                                    frame: Frame {
                                        function: Some("child2-1".to_string()),
                                        in_app: Some(true),
                                        line: Some(0),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                }))
                            ],
                            duration_ns: 5,
                            end_ns: 10,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-2".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 5,
                            frame: Frame {
                                function: Some("child1-2".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        }))
                    ],
                    duration_ns: 30_000_000, // 30 * time.Millisecond
                    end_ns: 30_000_000,
                    fingerprint: 0,
                    is_application: true,
                    line: Some(0),
                    name: "root".to_string(),
                    package: "package".to_string(),
                    path: Some("path".to_string()),
                    sample_count: 1,
                    start_ns: 0,
                    frame: Frame {
                        function: Some("root".to_string()),
                        in_app: Some(true),
                        line: Some(0),
                        package: Some("package".to_string()),
                        path: Some("path".to_string()),
                        ..Default::default()
                    },
                })),
                want: HashMap::from([
                    (
                        NodeKey {
                            package: "CoreFoundation".to_string(),
                            function: "CFReadStreamRead".to_string(),
                        },
                        NodeInfo {
                            category: FILE_READ.to_string(),
                            node: Node {
                                children: vec![], // children cleared as per the logic
                                duration_ns: 20_000_000,
                                end_ns: 20_000_000,
                                fingerprint: 0,
                                is_application: false,
                                line: Some(0),
                                name: "CFReadStreamRead".to_string(),
                                package: "CoreFoundation".to_string(),
                                path: Some("path".to_string()),
                                sample_count: 4,
                                start_ns: 0,
                                frame: Frame {
                                    function: Some("CFReadStreamRead".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            },
                            stack_trace: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child1-1".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2-1".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("CFReadStreamRead".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        }
                    )
                ]),
            },
            TestStruct {
                name: "Do not detect frame in call tree under duration threshold".to_string(),
                job: Box::new(DetectExactFrameOptions {
                    duration_threshold: Duration::from_millis(16),
                    functions_by_package: HashMap::from([
                        ("vroom", HashMap::from([
                            ("SuperShortFunction", FILE_READ),
                        ]))
                    ]),
                    ..Default::default()
                }),
                node: Rc::new(RefCell::new(Node {
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    children: vec![
                                        Rc::new(RefCell::new(Node {
                                            children: vec![],
                                            duration_ns: 10_000_000, // 10 * time.Millisecond - below threshold
                                            end_ns: 10_000_000,
                                            fingerprint: 0,
                                            is_application: false,
                                            line: Some(0),
                                            name: "SuperShortFunction".to_string(),
                                            package: "vroom".to_string(),
                                            path: Some("path".to_string()),
                                            sample_count: 1,
                                            start_ns: 0,
                                            frame: Frame {
                                                function: Some("SuperShortFunction".to_string()),
                                                in_app: Some(false),
                                                line: Some(0),
                                                package: Some("vroom".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                        }))
                                    ],
                                    duration_ns: 20_000_000,
                                    end_ns: 20_000_000,
                                    fingerprint: 0,
                                    is_application: true,
                                    line: Some(0),
                                    name: "child2-1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    sample_count: 1,
                                    start_ns: 0,
                                    frame: Frame {
                                        function: Some("child2-1".to_string()),
                                        in_app: Some(true),
                                        line: Some(0),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                }))
                            ],
                            duration_ns: 20_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-1".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 0,
                            frame: Frame {
                                function: Some("child1-1".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        }))
                    ],
                    duration_ns: 30_000_000, // 30 * time.Millisecond
                    end_ns: 30_000_000,
                    fingerprint: 0,
                    is_application: true,
                    line: Some(0),
                    name: "root".to_string(),
                    package: "package".to_string(),
                    path: Some("path".to_string()),
                    sample_count: 1,
                    start_ns: 0,
                    frame: Frame {
                        function: Some("root".to_string()),
                        in_app: Some(true),
                        line: Some(0),
                        package: Some("package".to_string()),
                        path: Some("path".to_string()),
                        ..Default::default()
                    },
                })),
                want: HashMap::new(), // Empty - no nodes should be detected
            },
            TestStruct {
                name: "Do not detect frame in call tree under sample threshold".to_string(),
                job: Box::new(DetectExactFrameOptions {
                    duration_threshold: Duration::from_millis(16),
                    sample_threshold: 4,
                    functions_by_package: HashMap::from([
                        ("vroom", HashMap::from([
                            ("FunctionWithOneSample", FILE_READ),
                            ("FunctionWithManySamples", FILE_READ),
                        ]))
                    ]),
                    ..Default::default()
                }),
                node: Rc::new(RefCell::new(Node {
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    children: vec![
                                        Rc::new(RefCell::new(Node {
                                            children: vec![],
                                            duration_ns: 20_000_000,
                                            end_ns: 20_000_000,
                                            fingerprint: 0,
                                            is_application: false,
                                            line: Some(0),
                                            name: "FunctionWithOneSample".to_string(),
                                            package: "vroom".to_string(),
                                            path: Some("path".to_string()),
                                            sample_count: 1, // Below threshold of 4
                                            start_ns: 0,
                                            frame: Frame {
                                                function: Some("FunctionWithOneSample".to_string()),
                                                in_app: Some(false),
                                                line: Some(0),
                                                package: Some("vroom".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                        })),
                                        Rc::new(RefCell::new(Node {
                                            children: vec![
                                                Rc::new(RefCell::new(Node {
                                                    children: vec![],
                                                    duration_ns: 20_000_000,
                                                    end_ns: 20_000_000,
                                                    fingerprint: 0,
                                                    is_application: false,
                                                    line: Some(0),
                                                    name: "FunctionWithManySamples".to_string(),
                                                    package: "vroom".to_string(),
                                                    path: Some("path".to_string()),
                                                    sample_count: 4, // Meets threshold of 4
                                                    start_ns: 0,
                                                    frame: Frame {
                                                        function: Some("FunctionWithManySamples".to_string()),
                                                        in_app: Some(false),
                                                        line: Some(0),
                                                        package: Some("vroom".to_string()),
                                                        path: Some("path".to_string()),
                                                        ..Default::default()
                                                    },
                                                }))
                                            ],
                                            duration_ns: 20_000_000,
                                            end_ns: 20_000_000,
                                            fingerprint: 0,
                                            is_application: true,
                                            line: Some(0),
                                            name: "child3-1".to_string(),
                                            package: "package".to_string(),
                                            path: Some("path".to_string()),
                                            sample_count: 1,
                                            start_ns: 0,
                                            frame: Frame {
                                                function: Some("child3-1".to_string()),
                                                in_app: Some(true),
                                                line: Some(0),
                                                package: Some("package".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                        }))
                                    ],
                                    duration_ns: 20_000_000,
                                    end_ns: 20_000_000,
                                    fingerprint: 0,
                                    is_application: true,
                                    line: Some(0),
                                    name: "child2-1".to_string(),
                                    package: "package".to_string(),
                                    path: Some("path".to_string()),
                                    sample_count: 1,
                                    start_ns: 0,
                                    frame: Frame {
                                        function: Some("child2-1".to_string()),
                                        in_app: Some(true),
                                        line: Some(0),
                                        package: Some("package".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                }))
                            ],
                            duration_ns: 20_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-1".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 0,
                            frame: Frame {
                                function: Some("child1-1".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        }))
                    ],
                    duration_ns: 30_000_000,
                    end_ns: 30_000_000,
                    fingerprint: 0,
                    is_application: true,
                    line: Some(0),
                    name: "root".to_string(),
                    package: "package".to_string(),
                    path: Some("path".to_string()),
                    sample_count: 1,
                    start_ns: 0,
                    frame: Frame {
                        function: Some("root".to_string()),
                        in_app: Some(true),
                        line: Some(0),
                        package: Some("package".to_string()),
                        path: Some("path".to_string()),
                        ..Default::default()
                    },
                })),
                want: HashMap::from([
                    (
                        NodeKey {
                            package: "vroom".to_string(),
                            function: "FunctionWithManySamples".to_string(),
                        },
                        NodeInfo {
                            category: FILE_READ.to_string(),
                            node: Node {
                                children: vec![], // children cleared as per the logic
                                duration_ns: 20_000_000,
                                end_ns: 20_000_000,
                                fingerprint: 0,
                                is_application: false,
                                line: Some(0),
                                name: "FunctionWithManySamples".to_string(),
                                package: "vroom".to_string(),
                                path: Some("path".to_string()),
                                sample_count: 4,
                                start_ns: 0,
                                frame: Frame {
                                    function: Some("FunctionWithManySamples".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("vroom".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            },
                            stack_trace: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child1-1".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child2-1".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child3-1".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("FunctionWithManySamples".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("vroom".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        }
                                         )
                 ]),
            },
            TestStruct {
                name: "Detect deeper frame in call tree".to_string(),
                job: Box::new(DetectExactFrameOptions {
                    duration_threshold: Duration::from_millis(16),
                    functions_by_package: HashMap::from([
                        ("CoreFoundation", HashMap::from([
                            ("LeafFunction", FILE_READ),
                            ("RandomFunction", FILE_READ),
                        ]))
                    ]),
                    ..Default::default()
                }),
                node: Rc::new(RefCell::new(Node {
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            children: vec![
                                Rc::new(RefCell::new(Node {
                                    children: vec![
                                        Rc::new(RefCell::new(Node {
                                            children: vec![],
                                            duration_ns: 20_000_000,
                                            end_ns: 20_000_000,
                                            fingerprint: 0,
                                            is_application: false,
                                            line: Some(0),
                                            name: "LeafFunction".to_string(),
                                            package: "CoreFoundation".to_string(),
                                            path: Some("path".to_string()),
                                            sample_count: 1,
                                            start_ns: 0,
                                            frame: Frame {
                                                function: Some("LeafFunction".to_string()),
                                                in_app: Some(false),
                                                line: Some(0),
                                                package: Some("CoreFoundation".to_string()),
                                                path: Some("path".to_string()),
                                                ..Default::default()
                                            },
                                        }))
                                    ],
                                    duration_ns: 20_000_000,
                                    end_ns: 20_000_000,
                                    fingerprint: 0,
                                    is_application: true,
                                    line: Some(0),
                                    name: "RandomFunction".to_string(),
                                    package: "CoreFoundation".to_string(),
                                    path: Some("path".to_string()),
                                    sample_count: 1,
                                    start_ns: 0,
                                    frame: Frame {
                                        function: Some("RandomFunction".to_string()),
                                        in_app: Some(true),
                                        line: Some(0),
                                        package: Some("CoreFoundation".to_string()),
                                        path: Some("path".to_string()),
                                        ..Default::default()
                                    },
                                }))
                            ],
                            duration_ns: 20_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-1".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 0,
                            frame: Frame {
                                function: Some("child1-1".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        }))
                    ],
                    duration_ns: 30_000_000,
                    end_ns: 30_000_000,
                    fingerprint: 0,
                    is_application: true,
                    line: Some(0),
                    name: "root".to_string(),
                    package: "package".to_string(),
                    path: Some("path".to_string()),
                    sample_count: 1,
                    start_ns: 0,
                    frame: Frame {
                        function: Some("root".to_string()),
                        in_app: Some(true),
                        line: Some(0),
                        package: Some("package".to_string()),
                        path: Some("path".to_string()),
                        ..Default::default()
                    },
                })),
                want: HashMap::from([
                    (
                        NodeKey {
                            package: "CoreFoundation".to_string(),
                            function: "LeafFunction".to_string(),
                        },
                        NodeInfo {
                            category: FILE_READ.to_string(),
                            node: Node {
                                children: vec![], // children cleared as per the logic
                                duration_ns: 20_000_000,
                                end_ns: 20_000_000,
                                fingerprint: 0,
                                is_application: false,
                                line: Some(0),
                                name: "LeafFunction".to_string(),
                                package: "CoreFoundation".to_string(),
                                path: Some("path".to_string()),
                                sample_count: 1,
                                start_ns: 0,
                                frame: Frame {
                                    function: Some("LeafFunction".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            },
                            stack_trace: vec![
                                Frame {
                                    function: Some("root".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("child1-1".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("package".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("RandomFunction".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                                Frame {
                                    function: Some("LeafFunction".to_string()),
                                    in_app: Some(false),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        }
                                        )
                ]),
            },
            TestStruct {
                name: "Detect first frame".to_string(),
                job: Box::new(DetectExactFrameOptions {
                    duration_threshold: Duration::from_millis(16),
                    functions_by_package: HashMap::from([
                        ("CoreFoundation", HashMap::from([
                            ("RandomFunction", FILE_READ),
                        ]))
                    ]),
                    ..Default::default()
                }),
                node: Rc::new(RefCell::new(Node {
                    children: vec![
                        Rc::new(RefCell::new(Node {
                            children: vec![],
                            duration_ns: 20_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-1".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 0,
                            frame: Frame {
                                function: Some("child1-1".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        })),
                        Rc::new(RefCell::new(Node {
                            children: vec![],
                            duration_ns: 20_000_000,
                            end_ns: 20_000_000,
                            fingerprint: 0,
                            is_application: false,
                            line: Some(0),
                            name: "child1-2".to_string(),
                            package: "package".to_string(),
                            path: Some("path".to_string()),
                            sample_count: 1,
                            start_ns: 0,
                            frame: Frame {
                                function: Some("child1-2".to_string()),
                                in_app: Some(false),
                                line: Some(0),
                                package: Some("package".to_string()),
                                path: Some("path".to_string()),
                                ..Default::default()
                            },
                        }))
                    ],
                    duration_ns: 30_000_000,
                    end_ns: 30_000_000,
                    fingerprint: 0,
                    is_application: true,
                    line: Some(0),
                    name: "RandomFunction".to_string(),
                    package: "CoreFoundation".to_string(),
                    path: Some("path".to_string()),
                    sample_count: 1,
                    start_ns: 0,
                    frame: Frame {
                        function: Some("RandomFunction".to_string()),
                        in_app: Some(true),
                        line: Some(0),
                        package: Some("CoreFoundation".to_string()),
                        path: Some("path".to_string()),
                        ..Default::default()
                    },
                })),
                want: HashMap::from([
                    (
                        NodeKey {
                            package: "CoreFoundation".to_string(),
                            function: "RandomFunction".to_string(),
                        },
                        NodeInfo {
                            category: FILE_READ.to_string(),
                            node: Node {
                                children: vec![], // children cleared as per the logic
                                duration_ns: 30_000_000,
                                end_ns: 30_000_000,
                                fingerprint: 0,
                                is_application: true,
                                line: Some(0),
                                name: "RandomFunction".to_string(),
                                package: "CoreFoundation".to_string(),
                                path: Some("path".to_string()),
                                sample_count: 1,
                                start_ns: 0,
                                frame: Frame {
                                    function: Some("RandomFunction".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            },
                            stack_trace: vec![
                                Frame {
                                    function: Some("RandomFunction".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("CoreFoundation".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        }
                                            )
                ]),
            },
            TestStruct {
                name: "Detect android frame".to_string(),
                job: Box::new(DetectAndroidFrameOptions {
                    active_thread_only: false,
                    duration_threshold: Duration::from_millis(16),
                    sample_threshold: 1,
                    functions_by_package: HashMap::from([
                        ("android.graphics", HashMap::from([
                            ("android.graphics.BitmapFactory.decodeStream", IMAGE_DECODE),
                        ]))
                    ]),
                }),
                node: Rc::new(RefCell::new(Node {
                    children: vec![],
                    duration_ns: 30_000_000,
                    end_ns: 30_000_000,
                    fingerprint: 0,
                    is_application: true,
                    line: Some(0),
                    name: "android.graphics.BitmapFactory.decodeStream(java.io.InputStream, android.graphics.Rect, android.graphics.BitmapFactory$Options): android.graphics.Bitmap".to_string(),
                    package: "android.graphics".to_string(),
                    path: Some("path".to_string()),
                    sample_count: 1,
                    start_ns: 0,
                    frame: Frame {
                        function: Some("android.graphics.BitmapFactory.decodeStream(java.io.InputStream, android.graphics.Rect, android.graphics.BitmapFactory$Options): android.graphics.Bitmap".to_string()),
                        in_app: Some(true),
                        line: Some(0),
                        package: Some("android.graphics".to_string()),
                        path: Some("path".to_string()),
                        ..Default::default()
                    },
                })),
                want: HashMap::from([
                    (
                        NodeKey {
                            package: "android.graphics".to_string(),
                            function: "android.graphics.BitmapFactory.decodeStream(java.io.InputStream, android.graphics.Rect, android.graphics.BitmapFactory$Options): android.graphics.Bitmap".to_string(),
                        },
                        NodeInfo {
                            category: IMAGE_DECODE.to_string(),
                            node: Node {
                                children: vec![], // children cleared as per the logic
                                duration_ns: 30_000_000,
                                end_ns: 30_000_000,
                                fingerprint: 0,
                                is_application: true,
                                line: Some(0),
                                name: "android.graphics.BitmapFactory.decodeStream(java.io.InputStream, android.graphics.Rect, android.graphics.BitmapFactory$Options): android.graphics.Bitmap".to_string(),
                                package: "android.graphics".to_string(),
                                path: Some("path".to_string()),
                                sample_count: 1,
                                start_ns: 0,
                                frame: Frame {
                                    function: Some("android.graphics.BitmapFactory.decodeStream(java.io.InputStream, android.graphics.Rect, android.graphics.BitmapFactory$Options): android.graphics.Bitmap".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("android.graphics".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            },
                            stack_trace: vec![
                                Frame {
                                    function: Some("android.graphics.BitmapFactory.decodeStream(java.io.InputStream, android.graphics.Rect, android.graphics.BitmapFactory$Options): android.graphics.Bitmap".to_string()),
                                    in_app: Some(true),
                                    line: Some(0),
                                    package: Some("android.graphics".to_string()),
                                    path: Some("path".to_string()),
                                    ..Default::default()
                                },
                            ],
                        }
                    )
                ]),
            }
        ];

        for test in test_cases {
            let mut nodes = HashMap::new();
            detect_frame_in_call_tree(&test.node, test.job.as_ref(), &mut nodes);

            assert_eq!(nodes, test.want, "test '{}' failed", test.name);
        }
    }
}
