/// Determines whether the image represents that of the application
/// binary (or a binary embedded in the application binary) by checking its package path.
pub fn is_cocoa_application_package(p: &str) -> bool {
    // These are the path patterns that iOS uses for applications,
    // system libraries are stored elsewhere.
    p.starts_with("/private/var/containers")
        || p.starts_with("/var/containers")
        || p.contains("/Developer/Xcode/DerivedData")
        || p.contains("/data/Containers/Bundle/Application")
}
