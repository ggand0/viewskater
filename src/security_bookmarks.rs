#[cfg(target_os = "macos")]
pub mod macos_file_handler {
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;
    use std::collections::HashMap;
    use std::time::Instant;
    use objc2::rc::autoreleasepool;
    use objc2::{msg_send, sel};
    use objc2::declare::ClassBuilder;
    use objc2::runtime::{AnyObject, Sel, AnyClass};
    use objc2_app_kit::{NSApplication, NSModalResponse, NSModalResponseOK};
    use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSDictionary, NSUserDefaults, NSURL, NSData};
    use objc2::rc::Retained;
    use once_cell::sync::Lazy;
    use std::io::Write;
    
    #[allow(unused_imports)]
    use log::{debug, info, warn, error};

    static mut FILE_CHANNEL: Option<Sender<String>> = None;
    
    // Store security-scoped URLs globally for session access  
    // FIXED: Store both the URL and whether it has active security scope
    #[derive(Clone, Debug)]
    struct SecurityScopedURLInfo {
        url: Retained<NSURL>,
        has_active_scope: bool,
        original_path: String,
        // NEW: Track if this URL was resolved from a bookmark vs. directly granted
        from_bookmark: bool,
        // NEW: Track when this was resolved to avoid re-resolving
        resolved_at: std::time::Instant,
    }
    
    static SECURITY_SCOPED_URLS: Lazy<Mutex<HashMap<String, SecurityScopedURLInfo>>> = 
        Lazy::new(|| Mutex::new(HashMap::new()));
    
    // NEW: Cache resolved bookmark URLs to avoid re-resolving the same bookmark
    static RESOLVED_BOOKMARK_CACHE: Lazy<Mutex<HashMap<String, Retained<NSURL>>>> = 
        Lazy::new(|| Mutex::new(HashMap::new()));
    
    // NEW: Track failed bookmark resolution attempts to avoid repeated failures
    static FAILED_BOOKMARK_PATHS: Lazy<Mutex<std::collections::HashSet<String>>> = 
        Lazy::new(|| Mutex::new(std::collections::HashSet::new()));
    
    // Constants for security-scoped bookmarks
    const NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE: u64 = 1 << 11;  // 0x800
    const NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE: u64 = 1 << 8;  // 0x100
    
    // ENABLED: Re-enable bookmark restoration after cleanup
    const DISABLE_BOOKMARK_RESTORATION: bool = false;
    
    // ENABLED: Re-enable bookmark creation after implementing safer methods
    const DISABLE_BOOKMARK_CREATION: bool = false;

    // ==================== CRASH DEBUG LOGGING ====================
    
    /// Writes a crash debug log entry immediately to disk (not buffered)
    /// This ensures we can see what happened even if the process crashes immediately after
    fn write_crash_debug_log(message: &str) {
        // Use the public function from the parent module
        crate::write_crash_debug_log(message);
    }
    
    /// Build stable UserDefaults keys for storing bookmarks
    /// Modern: uses full absolute path to avoid collisions and truncation
    /// Legacy: previous sanitized/truncated format for backward compatibility
    fn make_bookmark_keys(directory_path: &str) -> (
        Retained<NSString>,
        Retained<NSString>,
    ) {
        // Modern key retains full path
        let modern_key = format!("VSBookmark|{}", directory_path);
        let modern_ns = NSString::from_str(&modern_key);
        
        // Legacy key: first 50 alnum/_ chars
        let legacy_simple: String = directory_path
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(50)
            .collect();
        let legacy_key = format!("VSBookmark_{}", legacy_simple);
        let legacy_ns = NSString::from_str(&legacy_key);
        
        (modern_ns, legacy_ns)
    }
    
    // ==================== END CRASH DEBUG LOGGING ====================

    pub fn set_file_channel(sender: Sender<String>) {
        debug!("Setting file channel for macOS file handler");
        unsafe {
            FILE_CHANNEL = Some(sender);
        }
    }

    /// Stores a security-scoped URL for session access
    /// FIXED: Store URL info with active scope status
    fn store_security_scoped_url(path: &str, url: Retained<NSURL>) {
        store_security_scoped_url_with_info(path, url, false);
    }
    
    /// Stores a security-scoped URL with additional metadata
    fn store_security_scoped_url_with_info(path: &str, url: Retained<NSURL>, from_bookmark: bool) {
        debug!("Storing security-scoped URL for path: {}", path);
        
        let info = SecurityScopedURLInfo {
            url: url.clone(),
            has_active_scope: true,  // Assume it has active scope when stored
            original_path: path.to_string(),
            from_bookmark,
            resolved_at: std::time::Instant::now(),
        };
        
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            urls.insert(path.to_string(), info);
            debug!("Stored security-scoped URL (total count: {})", urls.len());
        } else {
            error!("Failed to lock security-scoped URLs mutex");
        }
    }

    /// FIXED: Get the actual resolved path from the security-scoped URL
    pub fn get_security_scoped_path(original_path: &str) -> Option<String> {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(info) = urls.get(original_path) {
                if info.has_active_scope {
                    // Get the actual path from the resolved URL
                    autoreleasepool(|pool| unsafe {
                        if let Some(path_nsstring) = info.url.path() {
                            let resolved_path = path_nsstring.as_str(pool);
                            debug!("Resolved security-scoped path: {} -> {}", original_path, resolved_path);
                            Some(resolved_path.to_string())
                        } else {
                            debug!("No path available from security-scoped URL for: {}", original_path);
                            None
                        }
                    })
                } else {
                    debug!("Security-scoped URL exists but scope is not active for: {}", original_path);
                    None
                }
            } else {
                debug!("No security-scoped URL found for: {}", original_path);
                None
            }
        } else {
            error!("Failed to lock security-scoped URLs mutex");
            None
        }
    }

    /// Checks if we have security-scoped access to a path
    pub fn has_security_scoped_access(path: &str) -> bool {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(info) = urls.get(path) {
                info.has_active_scope
            } else {
                false
            }
        } else {
            false
        }
    }

    /// FIXED: Stop security-scoped access for a path
    pub fn stop_security_scoped_access(path: &str) {
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(mut info) = urls.get_mut(path) {
                if info.has_active_scope {
                    unsafe {
                        let _: () = msg_send![&*info.url, stopAccessingSecurityScopedResource];
                        info.has_active_scope = false;
                        debug!("Stopped security-scoped access for: {}", path);
                    }
                }
            }
        }
    }

    /// Gets all accessible paths for debugging
    pub fn get_accessible_paths() -> Vec<String> {
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            urls.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    /// Clean up all active security-scoped access (call on app shutdown)
    /// ADDED: Proper lifecycle management
    pub fn cleanup_all_security_scoped_access() {
        debug!("Cleaning up all active security-scoped access");
        
        if let Ok(mut urls) = SECURITY_SCOPED_URLS.lock() {
            let mut stopped_count = 0;
            for (path, info) in urls.iter_mut() {
                if info.has_active_scope {
                    unsafe {
                        let _: () = msg_send![&*info.url, stopAccessingSecurityScopedResource];
                        info.has_active_scope = false;
                        stopped_count += 1;
                        debug!("Stopped security-scoped access for: {}", path);
                    }
                }
            }
            debug!("Cleaned up {} active security-scoped URLs", stopped_count);
        } else {
            error!("Failed to lock security-scoped URLs mutex during cleanup");
        }
    }

    /// Creates a security-scoped bookmark from a security-scoped URL and stores it persistently
    /// FIXED: Simplified and corrected implementation following Apple's documented pattern
    fn create_and_store_security_scoped_bookmark(url: &Retained<NSURL>, directory_path: &str) -> bool {
        if DISABLE_BOOKMARK_CREATION {
            eprintln!("BOOKMARK_CREATE_FIXED: disabled - skipping");
            return true;
        }
        
        write_crash_debug_log(&format!("BOOKMARK_CREATE_FIXED: Starting for path: {}", directory_path));
        debug!("Creating security-scoped bookmark for: {}", directory_path);
        
        let result = autoreleasepool(|_pool| unsafe {
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: Entered autoreleasepool");
            
            // Validate input path
            if directory_path.is_empty() || directory_path.len() > 500 {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - invalid directory path");
                return false;
            }
            
            // Create bookmark data from the security-scoped URL (from NSOpenPanel)
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: About to create bookmark data from NSOpenPanel URL");
            let bookmark_data: *mut AnyObject = msg_send![
                &**url,
                bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                relativeToURL: std::ptr::null::<AnyObject>()
                error: &mut error
            ];
            
            // Check for errors
            if !error.is_null() {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - bookmark creation failed");
                return false;
            }
            
            if bookmark_data.is_null() {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - bookmark data is null");
                return false;
            }
            
            // Verify it's NSData
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            
            if !is_nsdata {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - bookmark data is not NSData");
                return false;
            }
            
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: Bookmark data created successfully");
            
            // Store in NSUserDefaults with modern key (and legacy for back-compat)
            let defaults = NSUserDefaults::standardUserDefaults();
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            
            write_crash_debug_log("BOOKMARK_CREATE_FIXED: About to store in NSUserDefaults");
            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*modern_key];
            // Also store legacy key for back-compat migration
            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*legacy_key];
            
            // Synchronize to ensure it's persisted
            let sync_ok: bool = msg_send![&*defaults, synchronize];
            if sync_ok {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: SUCCESS - bookmark stored and synchronized");
                debug!("Successfully stored security-scoped bookmark");
                // Immediate read-back verification and logging
                let modern_obj: *mut AnyObject = msg_send![&*defaults, objectForKey: &*modern_key];
                if modern_obj.is_null() {
                    write_crash_debug_log("BOOKMARK_CREATE_FIXED: READBACK - modern key not found after store");
                } else {
                    let is_data: bool = msg_send![modern_obj, isKindOfClass: nsdata_class];
                    if is_data {
                        let len: usize = msg_send![modern_obj, length];
                        write_crash_debug_log(&format!("BOOKMARK_CREATE_FIXED: READBACK - modern key present, length={} bytes", len));
                        crate::write_immediate_crash_log(&format!("BOOKMARK_STORE: key='VSBookmark|{}' sync_ok=true len={} bytes", directory_path, len));
                    } else {
                        write_crash_debug_log("BOOKMARK_CREATE_FIXED: READBACK - modern key present but not NSData");
                        crate::write_immediate_crash_log(&format!("BOOKMARK_STORE: key='VSBookmark|{}' sync_ok=true (non-NSData)", directory_path));
                    }
                }
                true
            } else {
                write_crash_debug_log("BOOKMARK_CREATE_FIXED: ERROR - failed to synchronize");
                crate::write_immediate_crash_log(&format!("BOOKMARK_STORE: key='VSBookmark|{}' sync_ok=false", directory_path));
                false
            }
        });
        
        write_crash_debug_log(&format!("BOOKMARK_CREATE_FIXED: Final result: {}", result));
        result
    }
    
    /// FIXED: Proper implementation following Apple's documented pattern with lifecycle management
    fn resolve_security_scoped_bookmark(directory_path: &str) -> Option<Retained<NSURL>> {
        if DISABLE_BOOKMARK_RESTORATION {
            eprintln!("RESOLVE_FIXED: disabled - skipping");
            return None;
        }
        
        eprintln!("RESOLVE_FIXED: Starting for path: {}", directory_path);
        debug!("Resolving security-scoped bookmark for: {}", directory_path);
        crate::write_immediate_crash_log(&format!("RESOLVE: begin path={}", directory_path));
        
        let result = autoreleasepool(|pool| unsafe {
            eprintln!("RESOLVE_FIXED: Entered autoreleasepool");
            
            // Validate input
            if directory_path.is_empty() || directory_path.len() > 500 {
                eprintln!("RESOLVE_FIXED: ERROR - invalid path");
                crate::write_immediate_crash_log("RESOLVE: invalid path");
                return None;
            }
            
            // Log standardized form (best-effort via NSString)
            let orig_ns = NSString::from_str(directory_path);
            let std_ns: *mut AnyObject = msg_send![&*orig_ns, stringByStandardizingPath];
            if !std_ns.is_null() {
                let std_str = (&*(std_ns as *const NSString)).as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("RESOLVE: standardized='{}'", std_str));
            }
            
            // Also try canonicalize via std (may fail in sandbox)
            if let Ok(canon) = std::fs::canonicalize(directory_path) {
                if let Some(s) = canon.to_str() {
                    crate::write_immediate_crash_log(&format!("RESOLVE: canonicalize='{}'", s));
                }
            }
            
            let defaults = NSUserDefaults::standardUserDefaults();
            
            // STEP 1: Try direct lookup first
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            eprintln!("RESOLVE_FIXED: Looking for modern key");
            let mut bookmark_data: *mut AnyObject = msg_send![&*defaults, objectForKey: &*modern_key];
            if !bookmark_data.is_null() {
                let len: usize = msg_send![bookmark_data, length];
                crate::write_immediate_crash_log(&format!("RESOLVE: found modern key len={} bytes", len));
            }
            if bookmark_data.is_null() {
                eprintln!("RESOLVE_FIXED: Modern key not found, trying legacy");
                crate::write_immediate_crash_log("RESOLVE: modern key not found, trying legacy");
                bookmark_data = msg_send![&*defaults, objectForKey: &*legacy_key];
                if !bookmark_data.is_null() {
                    let len: usize = msg_send![bookmark_data, length];
                    crate::write_immediate_crash_log(&format!("RESOLVE: found legacy key len={} bytes", len));
                }
            }
            
            // STEP 2: If direct lookup failed, try tolerant restore (enumerate all VSBookmark keys)
            if bookmark_data.is_null() {
                crate::write_immediate_crash_log("RESOLVE: direct lookup failed, attempting tolerant restore");
                eprintln!("RESOLVE_FIXED: Direct lookup failed, attempting tolerant restore");
                
                let dict: *mut AnyObject = msg_send![&*defaults, dictionaryRepresentation];
                if !dict.is_null() {
                    let nsdict = &*(dict as *const NSDictionary<NSString, AnyObject>);
                    let keys = nsdict.allKeys();
                    let mut total = 0usize;
                    let mut vs_count = 0usize;
                    let mut tested_count = 0usize;
                    
                    // Normalize target path for comparison
                    let target_path_normalized = std::fs::canonicalize(directory_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| directory_path.to_string());
                    
                    crate::write_immediate_crash_log(&format!("RESOLVE: tolerant restore target_normalized='{}'", target_path_normalized));
                    
                    for i in 0..keys.len() {
                        total += 1;
                        let k = &keys[i];
                        let ks = k.as_str(pool).to_owned();
                        
                        if ks.starts_with("VSBookmark") {
                            vs_count += 1;
                            tested_count += 1;
                            
                            crate::write_immediate_crash_log(&format!("RESOLVE: testing key '{}'", ks));
                            
                            let test_bookmark: *mut AnyObject = msg_send![&*defaults, objectForKey: k];
                            if !test_bookmark.is_null() {
                                // Try to resolve this bookmark
                                let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
                                let mut error: *mut AnyObject = std::ptr::null_mut();
                                
                                let test_resolved: *mut AnyObject = msg_send![
                                    objc2::runtime::AnyClass::get("NSURL").unwrap(),
                                    URLByResolvingBookmarkData: test_bookmark
                                    options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                                    relativeToURL: std::ptr::null::<AnyObject>()
                                    bookmarkDataIsStale: &mut is_stale
                                    error: &mut error
                                ];
                                
                                if error.is_null() && !test_resolved.is_null() {
                                    // Get the resolved path and compare
                                    if let Some(path_nsstring) = (&*(test_resolved as *const NSURL)).path() {
                                        let resolved_path = path_nsstring.as_str(pool).to_owned();
                                        let resolved_normalized = std::fs::canonicalize(&resolved_path)
                                            .map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_else(|_| resolved_path.clone());
                                        
                                        crate::write_immediate_crash_log(&format!("RESOLVE: key '{}' -> resolved='{}'", ks, resolved_normalized));
                                        
                                        if resolved_normalized == target_path_normalized {
                                            crate::write_immediate_crash_log(&format!("RESOLVE: tolerant match found! using key '{}'", ks));
                                            eprintln!("RESOLVE_FIXED: Tolerant match found for key: {}", ks);
                                            bookmark_data = test_bookmark;
                                            
                                            // Learn this alias for future direct lookups
                                            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*modern_key];
                                            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*legacy_key];
                                            let sync_ok: bool = msg_send![&*defaults, synchronize];
                                            crate::write_immediate_crash_log(&format!("RESOLVE: learned alias sync_ok={}", sync_ok));
                                            break;
                                        }
                                    }
                                }
                            }
                            
                            // Limit testing to avoid excessive work
                            if tested_count >= 10 {
                                crate::write_immediate_crash_log("RESOLVE: tolerant restore tested 10 keys, stopping");
                                break;
                            }
                        }
                    }
                    
                    crate::write_immediate_crash_log(&format!("RESOLVE: tolerant restore complete - keys total={} vs_count={} tested={}", total, vs_count, tested_count));
                } else {
                    crate::write_immediate_crash_log("RESOLVE: dictionaryRepresentation unavailable");
                }
                
                if bookmark_data.is_null() {
                    eprintln!("RESOLVE_FIXED: No bookmark found after tolerant restore");
                    crate::write_immediate_crash_log("RESOLVE: no bookmark found after tolerant restore");
                    return None;
                }
            }
            
            // Verify it's NSData
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            
            if !is_nsdata {
                eprintln!("RESOLVE_FIXED: ERROR - not NSData, cleaning up");
                crate::write_immediate_crash_log("RESOLVE: object for key not NSData");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            eprintln!("RESOLVE_FIXED: Found valid bookmark data");
            crate::write_immediate_crash_log("RESOLVE: attempting URLByResolvingBookmarkData");
            
            // CRITICAL: Resolve bookmark to get NEW security-scoped URL instance
            let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            eprintln!("RESOLVE_FIXED: About to resolve bookmark data to NEW URL instance");
            let resolved_url: *mut AnyObject = msg_send![
                objc2::runtime::AnyClass::get("NSURL").unwrap(),
                URLByResolvingBookmarkData: bookmark_data
                options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                relativeToURL: std::ptr::null::<AnyObject>()
                bookmarkDataIsStale: &mut is_stale
                error: &mut error
            ];
            
            if !error.is_null() {
                eprintln!("RESOLVE_FIXED: ERROR - bookmark resolution failed, removing stale bookmark");
                crate::write_immediate_crash_log("RESOLVE: URLByResolvingBookmarkData error");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            if resolved_url.is_null() {
                eprintln!("RESOLVE_FIXED: ERROR - resolved URL is null, removing bookmark");
                crate::write_immediate_crash_log("RESOLVE: resolved_url is null");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            // Verify it's an NSURL
            let nsurl_class = objc2::runtime::AnyClass::get("NSURL").unwrap();
            let is_nsurl: bool = msg_send![resolved_url, isKindOfClass: nsurl_class];
            
            if !is_nsurl {
                eprintln!("RESOLVE_FIXED: ERROR - resolved object is not NSURL");
                crate::write_immediate_crash_log("RESOLVE: resolved object not NSURL");
                return None;
            }
            
            eprintln!("RESOLVE_FIXED: Successfully resolved bookmark to security-scoped URL");
            
            // Log resolved path string
            if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                let rs = path_nsstring.as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("RESOLVE: resolved_url.path='{}'", rs));
            }
            
            // CRITICAL: Call startAccessingSecurityScopedResource on the RESOLVED URL instance
            eprintln!("RESOLVE_FIXED: About to start accessing security-scoped resource on RESOLVED URL");
            let access_granted: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
            crate::write_immediate_crash_log(&format!("RESOLVE: startAccessingSecurityScopedResource={}", access_granted));
            
            // COMPREHENSIVE DIAGNOSTICS: If startAccessing failed, diagnose why
            if !access_granted {
                // Check macOS version - there's a known bug in macOS 15.0 Sequoia
                let os_version_info = std::process::Command::new("sw_vers")
                    .arg("-productVersion")
                    .output()
                    .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                
                crate::write_immediate_crash_log(&format!("RESOLVE: macOS version={}", os_version_info));
                
                // Check if this is the known macOS Sequoia bug
                let is_sequoia_bug = os_version_info.starts_with("15.0");
                if is_sequoia_bug {
                    crate::write_immediate_crash_log("RESOLVE: WARNING - macOS 15.0 Sequoia has known ScopedBookmarksAgent bug");
                    crate::write_immediate_crash_log("RESOLVE: This is a known macOS bug fixed in 15.1+");
                }
                
                // Additional diagnostics
                let url_is_file_url: bool = msg_send![resolved_url, isFileURL];
                let url_has_directory_path: bool = msg_send![resolved_url, hasDirectoryPath];
                
                if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                    let path_str = path_nsstring.as_str(pool).to_owned();
                    let path_exists = std::path::Path::new(&path_str).exists();
                    let path_is_dir = std::path::Path::new(&path_str).is_dir();
                    let path_readable = std::fs::read_dir(&path_str).is_ok();
                    
                    crate::write_immediate_crash_log(&format!("RESOLVE: URL diagnostics - isFileURL={} hasDirectoryPath={} pathExists={} isDir={} readable={}", 
                        url_is_file_url, url_has_directory_path, path_exists, path_is_dir, path_readable));
                    
                    // Try to get file attributes to see if there are permission issues
                    if let Ok(metadata) = std::fs::metadata(&path_str) {
                        let permissions = metadata.permissions();
                        crate::write_immediate_crash_log(&format!("RESOLVE: path metadata - readonly={}", permissions.readonly()));
                    } else {
                        crate::write_immediate_crash_log("RESOLVE: could not get path metadata");
                    }
                }
                
                // Check if we can create a non-security-scoped bookmark from this resolved URL
                let diagnostic_bookmark: *mut AnyObject = msg_send![
                    resolved_url,
                    bookmarkDataWithOptions: 0u64  // No security scope
                    includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                    relativeToURL: std::ptr::null::<AnyObject>()
                    error: std::ptr::null_mut::<*mut AnyObject>()
                ];
                
                if !diagnostic_bookmark.is_null() {
                    let diag_len: usize = msg_send![diagnostic_bookmark, length];
                    crate::write_immediate_crash_log(&format!("RESOLVE: non-security-scoped bookmark created successfully len={}", diag_len));
                } else {
                    crate::write_immediate_crash_log("RESOLVE: even non-security-scoped bookmark creation failed");
                }
                
                // For macOS Sequoia bug, try a workaround: kill and restart ScopedBookmarksAgent
                if is_sequoia_bug {
                    crate::write_immediate_crash_log("RESOLVE: attempting ScopedBookmarksAgent restart workaround");
                    
                    // Try to kill ScopedBookmarksAgent (this might fail silently, which is fine)
                    let _kill_result = std::process::Command::new("pkill")
                        .arg("-f")
                        .arg("ScopedBookmarksAgent")
                        .output();
                    
                    // Wait a moment for the agent to restart
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    // Try startAccessing again after the agent restart
                    let retry_access: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
                    crate::write_immediate_crash_log(&format!("RESOLVE: post-agent-restart startAccessing={}", retry_access));
                    
                    if retry_access {
                        crate::write_immediate_crash_log("RESOLVE: SUCCESS - agent restart workaround worked!");
                        
                        // Create Retained<NSURL> from the resolved URL
                        let _: *mut AnyObject = msg_send![resolved_url, retain];
                        let nsurl_ptr = resolved_url as *mut NSURL;
                        
                        if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                            store_security_scoped_url(directory_path, retained_url.clone());
                            return Some(retained_url);
                        } else {
                            let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                        }
                    }
                }
            }
            
            if access_granted {
                // Handle stale bookmarks by refreshing them
                if is_stale.as_bool() {
                    eprintln!("RESOLVE_FIXED: Bookmark is stale, refreshing it");
                    // Create fresh bookmark from the resolved URL (which now has active scope)
                    let fresh_bookmark_result: *mut AnyObject = msg_send![
                        resolved_url,
                        bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                        includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                        relativeToURL: std::ptr::null::<AnyObject>()
                        error: std::ptr::null_mut::<*mut AnyObject>()
                    ];
                    
                    if !fresh_bookmark_result.is_null() {
                        eprintln!("RESOLVE_FIXED: Created fresh bookmark, storing it");
                        crate::write_immediate_crash_log("RESOLVE: refreshed stale bookmark and stored");
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*modern_key];
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*legacy_key];
                        let _: bool = msg_send![&*defaults, synchronize];
                    } else {
                        eprintln!("RESOLVE_FIXED: WARNING - failed to create fresh bookmark");
                        crate::write_immediate_crash_log("RESOLVE: failed to refresh stale bookmark");
                    }
                }
                
                // Create Retained<NSURL> from the resolved URL
                let _: *mut AnyObject = msg_send![resolved_url, retain];
                let nsurl_ptr = resolved_url as *mut NSURL;
                
                if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                    eprintln!("RESOLVE_FIXED: SUCCESS - created Retained<NSURL> from resolved security-scoped URL");
                    crate::write_immediate_crash_log("RESOLVE: success");
                    
                    // Store the resolved URL for session use
                    store_security_scoped_url(directory_path, retained_url.clone());
                    debug!("Successfully restored directory access from bookmark");
                    Some(retained_url)
                } else {
                    eprintln!("RESOLVE_FIXED: ERROR - failed to create Retained<NSURL>");
                    crate::write_immediate_crash_log("RESOLVE: failed to retain resolved URL");
                    // Clean up - stop accessing the resource
                    let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                    None
                }
            } else {
                // All diagnostics have already been performed above
                eprintln!("RESOLVE_FIXED: ERROR - failed to start accessing security-scoped resource");
                debug!("Failed to start accessing restored security-scoped resource");
                crate::write_immediate_crash_log("RESOLVE: startAccessing failed (final)");
                None
            }
        });
        
        match &result {
            Some(_) => eprintln!("RESOLVE_FIXED: FINAL SUCCESS"),
            None => eprintln!("RESOLVE_FIXED: FINAL FAILURE"),
        }
        
        result
    }
    
    /// FIXED: Proper implementation following Apple's documented pattern with lifecycle management
    fn resolve_security_scoped_bookmark_OLD_REMOVE_ME(directory_path: &str) -> Option<Retained<NSURL>> {
        if DISABLE_BOOKMARK_RESTORATION {
            eprintln!("RESOLVE_FIXED: disabled - skipping");
            return None;
        }
        
        eprintln!("RESOLVE_FIXED: Starting for path: {}", directory_path);
        debug!("Resolving security-scoped bookmark for: {}", directory_path);
        crate::write_immediate_crash_log(&format!("RESOLVE: begin path={}", directory_path));
        
        let result = autoreleasepool(|pool| unsafe {
            eprintln!("RESOLVE_FIXED: Entered autoreleasepool");
            
            // Validate input
            if directory_path.is_empty() || directory_path.len() > 500 {
                eprintln!("RESOLVE_FIXED: ERROR - invalid path");
                crate::write_immediate_crash_log("RESOLVE: invalid path");
                return None;
            }
            
            // Log standardized form (best-effort via NSString)
            let orig_ns = NSString::from_str(directory_path);
            let std_ns: *mut AnyObject = msg_send![&*orig_ns, stringByStandardizingPath];
            if !std_ns.is_null() {
                let std_str = (&*(std_ns as *const NSString)).as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("RESOLVE: standardized='{}'", std_str));
            }
            
            // Also try canonicalize via std (may fail in sandbox)
            if let Ok(canon) = std::fs::canonicalize(directory_path) {
                if let Some(s) = canon.to_str() {
                    crate::write_immediate_crash_log(&format!("RESOLVE: canonicalize='{}'", s));
                }
            }
            
            let defaults = NSUserDefaults::standardUserDefaults();
            
            // STEP 1: Try direct lookup first
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            eprintln!("RESOLVE_FIXED: Looking for modern key");
            let mut bookmark_data: *mut AnyObject = msg_send![&*defaults, objectForKey: &*modern_key];
            if !bookmark_data.is_null() {
                let len: usize = msg_send![bookmark_data, length];
                crate::write_immediate_crash_log(&format!("RESOLVE: found modern key len={} bytes", len));
            }
            if bookmark_data.is_null() {
                eprintln!("RESOLVE_FIXED: Modern key not found, trying legacy");
                crate::write_immediate_crash_log("RESOLVE: modern key not found, trying legacy");
                bookmark_data = msg_send![&*defaults, objectForKey: &*legacy_key];
                if !bookmark_data.is_null() {
                    let len: usize = msg_send![bookmark_data, length];
                    crate::write_immediate_crash_log(&format!("RESOLVE: found legacy key len={} bytes", len));
                }
            }
            
            // STEP 2: If direct lookup failed, try tolerant restore (enumerate all VSBookmark keys)
            if bookmark_data.is_null() {
                crate::write_immediate_crash_log("RESOLVE: direct lookup failed, attempting tolerant restore");
                eprintln!("RESOLVE_FIXED: Direct lookup failed, attempting tolerant restore");
                
                let dict: *mut AnyObject = msg_send![&*defaults, dictionaryRepresentation];
                if !dict.is_null() {
                    let nsdict = &*(dict as *const NSDictionary<NSString, AnyObject>);
                    let keys = nsdict.allKeys();
                    let mut total = 0usize;
                    let mut vs_count = 0usize;
                    let mut tested_count = 0usize;
                    
                    // Normalize target path for comparison
                    let target_path_normalized = std::fs::canonicalize(directory_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| directory_path.to_string());
                    
                    crate::write_immediate_crash_log(&format!("RESOLVE: tolerant restore target_normalized='{}'", target_path_normalized));
                    
                    for i in 0..keys.len() {
                        total += 1;
                        let k = &keys[i];
                        let ks = k.as_str(pool).to_owned();
                        
                        if ks.starts_with("VSBookmark") {
                            vs_count += 1;
                            tested_count += 1;
                            
                            crate::write_immediate_crash_log(&format!("RESOLVE: testing key '{}'", ks));
                            
                            let test_bookmark: *mut AnyObject = msg_send![&*defaults, objectForKey: k];
                            if !test_bookmark.is_null() {
                                // Try to resolve this bookmark
                                let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
                                let mut error: *mut AnyObject = std::ptr::null_mut();
                                
                                let test_resolved: *mut AnyObject = msg_send![
                                    objc2::runtime::AnyClass::get("NSURL").unwrap(),
                                    URLByResolvingBookmarkData: test_bookmark
                                    options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                                    relativeToURL: std::ptr::null::<AnyObject>()
                                    bookmarkDataIsStale: &mut is_stale
                                    error: &mut error
                                ];
                                
                                if error.is_null() && !test_resolved.is_null() {
                                    // Get the resolved path and compare
                                    if let Some(path_nsstring) = (&*(test_resolved as *const NSURL)).path() {
                                        let resolved_path = path_nsstring.as_str(pool).to_owned();
                                        let resolved_normalized = std::fs::canonicalize(&resolved_path)
                                            .map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_else(|_| resolved_path.clone());
                                        
                                        crate::write_immediate_crash_log(&format!("RESOLVE: key '{}' -> resolved='{}'", ks, resolved_normalized));
                                        
                                        if resolved_normalized == target_path_normalized {
                                            crate::write_immediate_crash_log(&format!("RESOLVE: tolerant match found! using key '{}'", ks));
                                            eprintln!("RESOLVE_FIXED: Tolerant match found for key: {}", ks);
                                            bookmark_data = test_bookmark;
                                            
                                            // Learn this alias for future direct lookups
                                            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*modern_key];
                                            let _: () = msg_send![&*defaults, setObject: bookmark_data forKey: &*legacy_key];
                                            let sync_ok: bool = msg_send![&*defaults, synchronize];
                                            crate::write_immediate_crash_log(&format!("RESOLVE: learned alias sync_ok={}", sync_ok));
                                            break;
                                        }
                                    }
                                }
                            }
                            
                            // Limit testing to avoid excessive work
                            if tested_count >= 10 {
                                crate::write_immediate_crash_log("RESOLVE: tolerant restore tested 10 keys, stopping");
                                break;
                            }
                        }
                    }
                    
                    crate::write_immediate_crash_log(&format!("RESOLVE: tolerant restore complete - keys total={} vs_count={} tested={}", total, vs_count, tested_count));
                } else {
                    crate::write_immediate_crash_log("RESOLVE: dictionaryRepresentation unavailable");
                }
                
                if bookmark_data.is_null() {
                    eprintln!("RESOLVE_FIXED: No bookmark found after tolerant restore");
                    crate::write_immediate_crash_log("RESOLVE: no bookmark found after tolerant restore");
                    return None;
                }
            }
            
            // Verify it's NSData
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            
            if !is_nsdata {
                eprintln!("RESOLVE_FIXED: ERROR - not NSData, cleaning up");
                crate::write_immediate_crash_log("RESOLVE: object for key not NSData");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            eprintln!("RESOLVE_FIXED: Found valid bookmark data");
            crate::write_immediate_crash_log("RESOLVE: attempting URLByResolvingBookmarkData");
            
            // CRITICAL: Resolve bookmark to get NEW security-scoped URL instance
            let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            eprintln!("RESOLVE_FIXED: About to resolve bookmark data to NEW URL instance");
            let resolved_url: *mut AnyObject = msg_send![
                objc2::runtime::AnyClass::get("NSURL").unwrap(),
                URLByResolvingBookmarkData: bookmark_data
                options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                relativeToURL: std::ptr::null::<AnyObject>()
                bookmarkDataIsStale: &mut is_stale
                error: &mut error
            ];
            
            if !error.is_null() {
                eprintln!("RESOLVE_FIXED: ERROR - bookmark resolution failed, removing stale bookmark");
                crate::write_immediate_crash_log("RESOLVE: URLByResolvingBookmarkData error");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            if resolved_url.is_null() {
                eprintln!("RESOLVE_FIXED: ERROR - resolved URL is null, removing bookmark");
                crate::write_immediate_crash_log("RESOLVE: resolved_url is null");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            // Verify it's an NSURL
            let nsurl_class = objc2::runtime::AnyClass::get("NSURL").unwrap();
            let is_nsurl: bool = msg_send![resolved_url, isKindOfClass: nsurl_class];
            
            if !is_nsurl {
                eprintln!("RESOLVE_FIXED: ERROR - resolved object is not NSURL");
                crate::write_immediate_crash_log("RESOLVE: resolved object not NSURL");
                return None;
            }
            
            eprintln!("RESOLVE_FIXED: Successfully resolved bookmark to security-scoped URL");
            
            // Log resolved path string
            if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                let rs = path_nsstring.as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("RESOLVE: resolved_url.path='{}'", rs));
            }
            
            // CRITICAL: Call startAccessingSecurityScopedResource on the RESOLVED URL instance
            eprintln!("RESOLVE_FIXED: About to start accessing security-scoped resource on RESOLVED URL");
            let access_granted: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
            crate::write_immediate_crash_log(&format!("RESOLVE: startAccessingSecurityScopedResource={}", access_granted));
            
            // COMPREHENSIVE DIAGNOSTICS: If startAccessing failed, diagnose why
            if !access_granted {
                // Check macOS version - there's a known bug in macOS 15.0 Sequoia
                let os_version_info = std::process::Command::new("sw_vers")
                    .arg("-productVersion")
                    .output()
                    .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                
                crate::write_immediate_crash_log(&format!("RESOLVE: macOS version={}", os_version_info));
                
                // Check if this is the known macOS Sequoia bug
                let is_sequoia_bug = os_version_info.starts_with("15.0");
                if is_sequoia_bug {
                    crate::write_immediate_crash_log("RESOLVE: WARNING - macOS 15.0 Sequoia has known ScopedBookmarksAgent bug");
                    crate::write_immediate_crash_log("RESOLVE: This is a known macOS bug fixed in 15.1+");
                }
                
                // Additional diagnostics
                let url_is_file_url: bool = msg_send![resolved_url, isFileURL];
                let url_has_directory_path: bool = msg_send![resolved_url, hasDirectoryPath];
                
                if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                    let path_str = path_nsstring.as_str(pool).to_owned();
                    let path_exists = std::path::Path::new(&path_str).exists();
                    let path_is_dir = std::path::Path::new(&path_str).is_dir();
                    let path_readable = std::fs::read_dir(&path_str).is_ok();
                    
                    crate::write_immediate_crash_log(&format!("RESOLVE: URL diagnostics - isFileURL={} hasDirectoryPath={} pathExists={} isDir={} readable={}", 
                        url_is_file_url, url_has_directory_path, path_exists, path_is_dir, path_readable));
                    
                    // Try to get file attributes to see if there are permission issues
                    if let Ok(metadata) = std::fs::metadata(&path_str) {
                        let permissions = metadata.permissions();
                        crate::write_immediate_crash_log(&format!("RESOLVE: path metadata - readonly={}", permissions.readonly()));
                    } else {
                        crate::write_immediate_crash_log("RESOLVE: could not get path metadata");
                    }
                }
                
                // Check if we can create a non-security-scoped bookmark from this resolved URL
                let diagnostic_bookmark: *mut AnyObject = msg_send![
                    resolved_url,
                    bookmarkDataWithOptions: 0u64  // No security scope
                    includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                    relativeToURL: std::ptr::null::<AnyObject>()
                    error: std::ptr::null_mut::<*mut AnyObject>()
                ];
                
                if !diagnostic_bookmark.is_null() {
                    let diag_len: usize = msg_send![diagnostic_bookmark, length];
                    crate::write_immediate_crash_log(&format!("RESOLVE: non-security-scoped bookmark created successfully len={}", diag_len));
                } else {
                    crate::write_immediate_crash_log("RESOLVE: even non-security-scoped bookmark creation failed");
                }
                
                // For macOS Sequoia bug, try a workaround: kill and restart ScopedBookmarksAgent
                if is_sequoia_bug {
                    crate::write_immediate_crash_log("RESOLVE: attempting ScopedBookmarksAgent restart workaround");
                    
                    // Try to kill ScopedBookmarksAgent (this might fail silently, which is fine)
                    let _kill_result = std::process::Command::new("pkill")
                        .arg("-f")
                        .arg("ScopedBookmarksAgent")
                        .output();
                    
                    // Wait a moment for the agent to restart
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    // Try startAccessing again after the agent restart
                    let retry_access: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
                    crate::write_immediate_crash_log(&format!("RESOLVE: post-agent-restart startAccessing={}", retry_access));
                    
                    if retry_access {
                        crate::write_immediate_crash_log("RESOLVE: SUCCESS - agent restart workaround worked!");
                        
                        // Create Retained<NSURL> from the resolved URL
                        let _: *mut AnyObject = msg_send![resolved_url, retain];
                        let nsurl_ptr = resolved_url as *mut NSURL;
                        
                        if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                            store_security_scoped_url(directory_path, retained_url.clone());
                            return Some(retained_url);
                        } else {
                            let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                        }
                    }
                }
            }
            
            if access_granted {
                // Handle stale bookmarks by refreshing them
                if is_stale.as_bool() {
                    eprintln!("RESOLVE_FIXED: Bookmark is stale, refreshing it");
                    // Create fresh bookmark from the resolved URL (which now has active scope)
                    let fresh_bookmark_result: *mut AnyObject = msg_send![
                        resolved_url,
                        bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                        includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                        relativeToURL: std::ptr::null::<AnyObject>()
                        error: std::ptr::null_mut::<*mut AnyObject>()
                    ];
                    
                    if !fresh_bookmark_result.is_null() {
                        eprintln!("RESOLVE_FIXED: Created fresh bookmark, storing it");
                        crate::write_immediate_crash_log("RESOLVE: refreshed stale bookmark and stored");
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*modern_key];
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*legacy_key];
                        let _: bool = msg_send![&*defaults, synchronize];
                    } else {
                        eprintln!("RESOLVE_FIXED: WARNING - failed to create fresh bookmark");
                        crate::write_immediate_crash_log("RESOLVE: failed to refresh stale bookmark");
                    }
                }
                
                // Create Retained<NSURL> from the resolved URL
                let _: *mut AnyObject = msg_send![resolved_url, retain];
                let nsurl_ptr = resolved_url as *mut NSURL;
                
                if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                    eprintln!("RESOLVE_FIXED: SUCCESS - created Retained<NSURL> from resolved security-scoped URL");
                    crate::write_immediate_crash_log("RESOLVE: success");
                    
                    // Store the resolved URL for session use
                    store_security_scoped_url(directory_path, retained_url.clone());
                    debug!("Successfully restored directory access from bookmark");
                    Some(retained_url)
                } else {
                    eprintln!("RESOLVE_FIXED: ERROR - failed to create Retained<NSURL>");
                    crate::write_immediate_crash_log("RESOLVE: failed to retain resolved URL");
                    // Clean up - stop accessing the resource
                    let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                    None
                }
            } else {
                // All diagnostics have already been performed above
                eprintln!("RESOLVE_FIXED: ERROR - failed to start accessing security-scoped resource");
                debug!("Failed to start accessing restored security-scoped resource");
                crate::write_immediate_crash_log("RESOLVE: startAccessing failed (final)");
                None
            }
        });
        
        match &result {
            Some(_) => eprintln!("RESOLVE_FIXED: FINAL SUCCESS"),
            None => eprintln!("RESOLVE_FIXED: FINAL FAILURE"),
        }
        
        result
    }
    
    /// Public function to restore directory access for a specific path using stored bookmarks
    /// This is called when the app needs to regain access to a previously granted directory
    pub fn restore_directory_access_for_path(directory_path: &str) -> bool {
        debug!("Restoring directory access for path: {}", directory_path);
        
        // Use the new simplified resolution function
        match get_resolved_security_scoped_url(directory_path) {
            Some(_url) => {
                debug!("Successfully restored directory access via bookmark");
                true
            }
            None => {
                debug!("Failed to restore directory access via bookmark");
                false
            }
        }
    }
    
    /// Normalizes a directory path for use as a UserDefaults key - SIMPLIFIED VERSION
    fn normalize_path_for_key(path: &str) -> String {
        // Create a simple, safe key from the directory path
        path.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .take(50)
            .collect::<String>()
    }

    /// Requests directory access via NSOpenPanel and creates persistent bookmark
    /// FIXED: Proper handling of NSOpenPanel security-scoped URLs
    fn request_directory_access_with_nsopenpanel(requested_path: &str) -> bool {
        eprintln!("PANEL_FIXED: Starting for path: {}", requested_path);
        debug!("Requesting directory access via NSOpenPanel for: {}", requested_path);
        
        let result = autoreleasepool(|_pool| unsafe {
            eprintln!("PANEL_FIXED: Entered autoreleasepool");
            
            let mtm = MainThreadMarker::new().expect("Must be on main thread");
            eprintln!("PANEL_FIXED: Main thread marker created");
                
            // Create NSOpenPanel
            eprintln!("PANEL_FIXED: Getting NSOpenPanel class");
            let panel_class = objc2::runtime::AnyClass::get("NSOpenPanel").expect("NSOpenPanel class not found");
            eprintln!("PANEL_FIXED: Creating NSOpenPanel instance");
            let panel: *mut AnyObject = msg_send![panel_class, openPanel];
            eprintln!("PANEL_FIXED: NSOpenPanel created");
                
            // Configure panel for directory selection
            eprintln!("PANEL_FIXED: Configuring panel");
            let _: () = msg_send![panel, setCanChooseDirectories: true];
            let _: () = msg_send![panel, setCanChooseFiles: false];
            let _: () = msg_send![panel, setAllowsMultipleSelection: false];
            let _: () = msg_send![panel, setCanCreateDirectories: false];
            
            // Set initial directory to the requested path's parent if possible
            if let Some(parent_dir) = std::path::Path::new(requested_path).parent() {
                eprintln!("PANEL_FIXED: Setting initial directory");
                let parent_str = parent_dir.to_string_lossy();
                let parent_nsstring = NSString::from_str(&parent_str);
                let parent_url = NSURL::fileURLWithPath(&parent_nsstring);
                let _: () = msg_send![panel, setDirectoryURL: &*parent_url];
                eprintln!("PANEL_FIXED: Initial directory set");
            }
                
            // Set dialog title and message
            eprintln!("PANEL_FIXED: Setting panel text");
            let title = NSString::from_str("Grant Directory Access");
            let _: () = msg_send![panel, setTitle: &*title];
                
            let message = NSString::from_str(&format!(
                "ViewSkater needs access to browse images in this directory:\n\n{}\n\nPlease select the directory to grant persistent access.",
                requested_path
            ));
            let _: () = msg_send![panel, setMessage: &*message];
                
            // Show the panel and get user response
            eprintln!("PANEL_FIXED: About to show modal");
            debug!("Showing NSOpenPanel...");
            let response: NSModalResponse = msg_send![panel, runModal];
            eprintln!("PANEL_FIXED: Modal completed with response: {:?}", response as i32);
                
            if response == NSModalResponseOK {
                eprintln!("PANEL_FIXED: User granted access");
                debug!("User granted directory access via NSOpenPanel");
                
                // Get the selected URLs array
                eprintln!("PANEL_FIXED: Getting selected URLs");
                let selected_urls: *mut AnyObject = msg_send![panel, URLs];
                
                if selected_urls.is_null() {
                    eprintln!("PANEL_FIXED: ERROR - URLs array is null");
                    return false;
                }
                
                // Cast to NSArray and get first URL
                let urls_array = &*(selected_urls as *const NSArray<NSURL>);
                if urls_array.len() == 0 {
                    eprintln!("PANEL_FIXED: ERROR - URLs array is empty");
                    return false;
                }
                
                let selected_url = &urls_array[0];
                
                // Get the path string
                if let Some(path_nsstring) = selected_url.path() {
                    let selected_path = path_nsstring.as_str(_pool);
                    eprintln!("PANEL_FIXED: Selected path: '{}'", selected_path);
                    debug!("Selected directory: {}", selected_path);
                    
                    // Ensure we have active scope before creating the bookmark
                    eprintln!("PANEL_FIXED: Ensuring active scope on selected URL prior to bookmark creation");
                    let started: bool = msg_send![&*selected_url, startAccessingSecurityScopedResource];
                    crate::write_immediate_crash_log(&format!("PANEL: startAccessing on selected dir={}", started));
                    
                    // Convert &NSURL to Retained<NSURL>
                    let _: *mut AnyObject = msg_send![selected_url, retain];
                    let retained_url = Retained::from_raw(selected_url as *const NSURL as *mut NSURL).unwrap();
                    
                    // Store the URL for immediate session use
                    store_security_scoped_url(selected_path, retained_url.clone());
                    eprintln!("PANEL_FIXED: URL stored for session use");
                    
                    // Create and store persistent bookmark for future sessions
                    eprintln!("PANEL_FIXED: About to create persistent bookmark");
                    let store_ok = create_and_store_security_scoped_bookmark(&retained_url, selected_path);
                    if started {
                        // Balance the initial startAccessing call for the panel URL
                        let _: () = msg_send![&*selected_url, stopAccessingSecurityScopedResource];
                        crate::write_immediate_crash_log("PANEL: stopAccessing on selected dir (balanced)");
                    }
                    if store_ok {
                        eprintln!("PANEL_FIXED: SUCCESS - bookmark created and stored");
                        debug!("Successfully created persistent bookmark");
                        true
                    } else {
                        eprintln!("PANEL_FIXED: WARNING - bookmark creation failed, but have session access");
                        debug!("Failed to create persistent bookmark, but have session access");
                        true // Still have temporary access for this session
                    }
                } else {
                    eprintln!("PANEL_FIXED: ERROR - selected URL has no path");
                    debug!("No path returned from selected URL");
                    false
                }
            } else {
                eprintln!("PANEL_FIXED: User cancelled");
                debug!("User cancelled NSOpenPanel");
                false
            }
        });
        
        eprintln!("PANEL_FIXED: Final result: {}", result);
        result
    }

    /// Attempts to read a directory using existing security-scoped access
    /// FIXED: Use the resolved URL path for file operations
    pub fn read_directory_with_security_scoped_access(path: &str) -> Option<Result<std::fs::ReadDir, std::io::Error>> {
        debug!("Attempting to read directory with existing security-scoped access: {}", path);
        
        // Try to get a resolved security-scoped URL
        if let Some(resolved_url) = get_resolved_security_scoped_url(path) {
            debug!("Have security-scoped URL, attempting directory read");
            
            // Get the resolved path from the URL for file operations
            let resolved_path = autoreleasepool(|pool| unsafe {
                if let Some(path_nsstring) = resolved_url.path() {
                    Some(path_nsstring.as_str(pool).to_owned())
                } else {
                    None
                }
            });
            
            if let Some(resolved_path) = resolved_path {
                debug!("Using resolved security-scoped path: {}", resolved_path);
                let result = Some(std::fs::read_dir(resolved_path));
                
                // Clean up - stop accessing the security scoped resource
                unsafe {
                    let _: () = msg_send![&*resolved_url, stopAccessingSecurityScopedResource];
                }
                
                return result;
        } else {
                error!("Failed to get path from resolved security-scoped URL for: {}", path);
                
                // Clean up - stop accessing the security scoped resource
                unsafe {
                    let _: () = msg_send![&*resolved_url, stopAccessingSecurityScopedResource];
                }
                
                return Some(std::fs::read_dir(path)); // Fallback to original path
            }
        }
        
            debug!("No security-scoped access available for directory read");
            
            // Check for "Open With" scenario - individual file access within this directory
            if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
                let has_file_in_directory = urls.keys().any(|key| {
                    let key_path = std::path::Path::new(key);
                    if key_path.is_file() {
                        if let Some(file_parent) = key_path.parent() {
                            return file_parent.to_string_lossy() == path;
                        }
                    }
                    false
                });
                
                if has_file_in_directory {
                    debug!("Detected 'Open With' scenario - requesting directory access");
                    drop(urls); // Release lock before calling request function
                    
                    if request_directory_access_with_nsopenpanel(path) {
                        debug!("Directory access granted, retrying read");
                    // After granting access, try to get the resolved URL again
                    if let Some(resolved_url) = get_resolved_security_scoped_url(path) {
                        let resolved_path = autoreleasepool(|pool| unsafe {
                            if let Some(path_nsstring) = resolved_url.path() {
                                Some(path_nsstring.as_str(pool).to_owned())
                            } else {
                                None
                            }
                        });
                        
                        if let Some(resolved_path) = resolved_path {
                            debug!("Using newly resolved security-scoped path: {}", resolved_path);
                            let result = Some(std::fs::read_dir(resolved_path));
                            
                            // Clean up
                            unsafe {
                                let _: () = msg_send![&*resolved_url, stopAccessingSecurityScopedResource];
                            }
                            
                            return result;
                        } else {
                            // Clean up
                            unsafe {
                                let _: () = msg_send![&*resolved_url, stopAccessingSecurityScopedResource];
                            }
                            
                            return Some(std::fs::read_dir(path)); // Fallback
                        }
                    } else {
                            return Some(std::fs::read_dir(path)); // Fallback
                        }
                    }
                }
            }
            
            None
    }

    /// Helper function to request parent directory access for a file
    pub fn request_parent_directory_permission_dialog(file_path: &str) -> bool {
        debug!("🔍 Requesting parent directory access for file: {}", file_path);
        
        if let Some(parent_dir) = std::path::Path::new(file_path).parent() {
            let parent_dir_str = parent_dir.to_string_lossy();
            request_directory_access_with_nsopenpanel(&parent_dir_str)
        } else {
            debug!("Could not determine parent directory for: {}", file_path);
            false
        }
    }

    /// Placeholder for full disk access - in a sandboxed environment, we use directory-specific access
    pub fn restore_full_disk_access() -> bool {
        debug!("🔍 restore_full_disk_access() called - deferring to directory-specific restoration");
        false // We handle restoration per-directory via restore_directory_access_for_path
    }

    /// Check if we have full disk access (simplified check)
    pub fn has_full_disk_access() -> bool {
        // Try to read a protected directory
        if let Some(home_dir) = dirs::home_dir() {
            let desktop_dir = home_dir.join("Desktop");
            match std::fs::read_dir(&desktop_dir) {
                Ok(_) => {
                    debug!("✅ Full disk access confirmed");
                    true
                }
                Err(_) => {
                    debug!("❌ No full disk access");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Simplified full disk access request - actually requests directory access
    pub fn request_full_disk_access_once() -> bool {
        debug!("🔍 request_full_disk_access_once() - using directory access instead");
        
        // In a sandboxed environment, we can't get true "full disk access"
        // Instead, we request access to the user's home directory as a reasonable default
        if let Some(home_dir) = dirs::home_dir() {
            let home_path = home_dir.to_string_lossy();
            request_directory_access_with_nsopenpanel(&home_path)
        } else {
            false
        }
    }

    /// Handle opening a file via "Open With" from Finder
    unsafe extern "C" fn handle_opened_file(
        _this: &mut AnyObject,
        _sel: Sel,
        _sender: &AnyObject,
        files: &NSArray<NSString>,
    ) {
        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Function entry");
        write_crash_debug_log("FINDER_OPEN: handle_opened_file called");
        debug!("handle_opened_file called with {} files", files.len());
        
        if files.is_empty() {
            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Empty files array");
            write_crash_debug_log("FINDER_OPEN: Empty files array received");
            debug!("Empty files array received");
            return;
        }
        
        crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Processing {} files", files.len()));
        write_crash_debug_log(&format!("FINDER_OPEN: Processing {} files", files.len()));
        
        autoreleasepool(|pool| {
            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Entered autoreleasepool");
            write_crash_debug_log("FINDER_OPEN: Entered autoreleasepool");
            
            for (i, file) in files.iter().enumerate() {
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Processing file {} of {}", i + 1, files.len()));
                write_crash_debug_log(&format!("FINDER_OPEN: Processing file {} of {}", i + 1, files.len()));
                
                let path = file.as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: File path: {}", path));
                debug!("Processing file: {}", path);
                write_crash_debug_log(&format!("FINDER_OPEN: File path: {}", path));
                
                crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to create NSURL");
                write_crash_debug_log("FINDER_OPEN: About to create NSURL");
                // Create NSURL and try to get security-scoped access
                let url = NSURL::fileURLWithPath(&file);
                crate::write_immediate_crash_log("HANDLE_OPENED_FILE: NSURL created");
                write_crash_debug_log("FINDER_OPEN: NSURL created, about to call startAccessingSecurityScopedResource");
                let file_accessed: bool = msg_send![&*url, startAccessingSecurityScopedResource];
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Security access result: {}", file_accessed));
                write_crash_debug_log(&format!("FINDER_OPEN: Security access result: {}", file_accessed));
                
                if file_accessed {
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Security access granted");
                    debug!("Gained security-scoped access to file: {}", path);
                    
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to store file URL");
                    write_crash_debug_log("FINDER_OPEN: About to store file URL");
                    // Store the file URL
                    store_security_scoped_url(&path, url.clone());
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: File URL stored");
                    write_crash_debug_log("FINDER_OPEN: File URL stored successfully");
                    
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to get parent directory");
                    write_crash_debug_log("FINDER_OPEN: About to get parent directory");
                    // Try to get parent directory access
                    if let Some(parent_url) = url.URLByDeletingLastPathComponent() {
                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Got parent URL");
                        write_crash_debug_log("FINDER_OPEN: Got parent URL, about to get path");
                        if let Some(parent_path) = parent_url.path() {
                            let parent_path_str = parent_path.as_str(pool).to_owned();
                            crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Parent directory: {}", parent_path_str));
                            debug!("Checking parent directory: {}", parent_path_str);
                            write_crash_debug_log(&format!("FINDER_OPEN: Parent directory: {}", parent_path_str));
                            
                            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to test directory access");
                            write_crash_debug_log("FINDER_OPEN: About to test directory access");
                            // Test if we already have directory access
                            match std::fs::read_dir(&parent_path_str) {
                                Ok(_) => {
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Have directory access");
                                    debug!("Already have parent directory access");
                                    write_crash_debug_log("FINDER_OPEN: Have directory access, storing parent URL");
                                    store_security_scoped_url(&parent_path_str, parent_url.clone());
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Parent URL stored");
                                    write_crash_debug_log("FINDER_OPEN: Parent URL stored successfully");
                                }
                                Err(_) => {
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: No directory access");
                                    debug!("No parent directory access - will restore from bookmark if available");
                                    write_crash_debug_log("FINDER_OPEN: No directory access - bookmark restoration needed");
                                    // EARLY RESTORE: attempt to restore and retry
                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Attempting early restore_directory_access_for_path on parent [CALLSITE=handle_opened_file]");
                                    if restore_directory_access_for_path(&parent_path_str) {
                                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Early restore succeeded");
                                        debug!("Early bookmark restoration for parent directory succeeded");
                                        if let Some(resolved_parent) = get_security_scoped_path(&parent_path_str) {
                                            crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Using resolved parent path: {}", resolved_parent));
                                            match std::fs::read_dir(&resolved_parent) {
                                                Ok(_) => {
                                                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Directory read successful after early restore");
                                                    debug!("Directory access confirmed after early restore");
                                                }
                                                Err(e2) => {
                                                    crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Directory read still failed after early restore: {}", e2));
                                                    debug!("Directory read still failed after early restore: {}", e2);
                                                }
                                            }
                                        } else {
                                            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: No resolved parent path available after early restore");
                                        }
                                    } else {
                                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Early restore failed or no bookmark available");
                                        debug!("Early bookmark restoration failed or no bookmark available for parent directory");
                                    }
                                }
                            }
                        } else {
                            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Parent URL has no path");
                            write_crash_debug_log("FINDER_OPEN: Parent URL has no path");
                        }
                    } else {
                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Could not get parent URL");
                        write_crash_debug_log("FINDER_OPEN: Could not get parent URL");
                    }
                    
                    crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to send file path to main thread");
                    write_crash_debug_log("FINDER_OPEN: About to send file path to main thread");
                    // Send file path to main app
                    if let Some(ref sender) = FILE_CHANNEL {
                        match sender.send(path.clone()) {
                            Ok(_) => {
                                crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Successfully sent to main thread");
                                debug!("Successfully sent file path to main thread");
                                write_crash_debug_log("FINDER_OPEN: Successfully sent to main thread");
                            },
                            Err(e) => {
                                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Failed to send: {}", e));
                                error!("Failed to send file path: {}", e);
                                write_crash_debug_log(&format!("FINDER_OPEN: Failed to send: {}", e));
                            },
                        }
                    } else {
                        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: FILE_CHANNEL is None");
                        write_crash_debug_log("FINDER_OPEN: FILE_CHANNEL is None");
                    }
                } else {
                    crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Failed security access for: {}", path));
                    debug!("Failed to get security-scoped access for file: {}", path);
                    write_crash_debug_log(&format!("FINDER_OPEN: Failed security access for: {}", path));
                }
                
                crate::write_immediate_crash_log(&format!("HANDLE_OPENED_FILE: Completed file {} of {}", i + 1, files.len()));
                write_crash_debug_log(&format!("FINDER_OPEN: Completed file {} of {}", i + 1, files.len()));
            }
            
            crate::write_immediate_crash_log("HANDLE_OPENED_FILE: About to exit autoreleasepool");
            write_crash_debug_log("FINDER_OPEN: About to exit autoreleasepool");
        });
        
        crate::write_immediate_crash_log("HANDLE_OPENED_FILE: Function completed successfully");
        write_crash_debug_log("FINDER_OPEN: handle_opened_file completed successfully");
    }

    /// Handle opening a single file via legacy "Open With" method (application:openFile:)
    unsafe extern "C" fn handle_opened_file_single(
        _this: &mut AnyObject,
        _sel: Sel,
        _sender: &AnyObject,
        filename: &NSString,
    ) {
        debug!("handle_opened_file_single called");
        
        autoreleasepool(|pool| {
            let path = filename.as_str(pool).to_owned();
            debug!("Processing single file: {}", path);
            
            // Create NSURL and try to get security-scoped access
            let url = NSURL::fileURLWithPath(&filename);
            let file_accessed: bool = msg_send![&*url, startAccessingSecurityScopedResource];
            
            if file_accessed {
                debug!("Gained security-scoped access to single file");
                store_security_scoped_url(&path, url);
                
                // Send the file path to the main app
                if let Some(ref sender) = FILE_CHANNEL {
                    match sender.send(path.clone()) {
                        Ok(_) => debug!("Successfully sent single file path to main thread"),
                        Err(e) => error!("Failed to send single file path: {}", e),
                    }
                }
            } else {
                debug!("Failed to get security-scoped access for single file: {}", path);
            }
        });
    }

    /// Handle app launch detection to see if we're launched with files
    unsafe extern "C" fn handle_will_finish_launching(
        _this: &mut AnyObject,
        _sel: Sel,
        _notification: &AnyObject,
    ) {
        debug!("App will finish launching");
        
        // Check command line arguments
        let args: Vec<String> = std::env::args().collect();
        debug!("Command line arguments count: {}", args.len());
        
        for (i, arg) in args.iter().enumerate() {
            if i > 0 && std::path::Path::new(arg).exists() {
                debug!("Found potential file argument: {}", arg);
            }
        }
    }

    pub fn register_file_handler() {
        crate::write_immediate_crash_log("REGISTER_HANDLER: Function entry");
        debug!("Registering file handler for macOS");
        
        crate::write_immediate_crash_log("REGISTER_HANDLER: About to create MainThreadMarker");
        let mtm = MainThreadMarker::new().expect("Must be on main thread");
        crate::write_immediate_crash_log("REGISTER_HANDLER: MainThreadMarker created");
        
        unsafe {
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to get NSApplication");
            let app = NSApplication::sharedApplication(mtm);
            crate::write_immediate_crash_log("REGISTER_HANDLER: NSApplication obtained");
            
            // Get the existing delegate
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to get delegate");
            let delegate = app.delegate().unwrap();
            crate::write_immediate_crash_log("REGISTER_HANDLER: Delegate obtained");
            
            // Find out class of the NSApplicationDelegate
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to get delegate class");
            let class: &AnyClass = msg_send![&delegate, class];
            crate::write_immediate_crash_log("REGISTER_HANDLER: Delegate class obtained");
            
            // Create a subclass of the existing delegate
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to create ClassBuilder");
            let mut my_class = ClassBuilder::new("ViewSkaterApplicationDelegate", class).unwrap();
            crate::write_immediate_crash_log("REGISTER_HANDLER: ClassBuilder created");
            
            // Add file handling methods
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to add methods");
            my_class.add_method(
                sel!(application:openFiles:),
                handle_opened_file as unsafe extern "C" fn(_, _, _, _),
            );
            
            my_class.add_method(
                sel!(application:openFile:),
                handle_opened_file_single as unsafe extern "C" fn(_, _, _, _),
            );
            
            my_class.add_method(
                sel!(applicationWillFinishLaunching:),
                handle_will_finish_launching as unsafe extern "C" fn(_, _, _),
            );
            crate::write_immediate_crash_log("REGISTER_HANDLER: Methods added");
            
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to register class");
            let class = my_class.register();
            crate::write_immediate_crash_log("REGISTER_HANDLER: Class registered");
            
            // Cast and set the class
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to cast delegate");
            let delegate_obj = Retained::cast::<AnyObject>(delegate);
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to set delegate class");
            AnyObject::set_class(&delegate_obj, class);
            crate::write_immediate_crash_log("REGISTER_HANDLER: Delegate class set");
            
            // Prevent AppKit from interpreting our command line
            crate::write_immediate_crash_log("REGISTER_HANDLER: About to configure AppKit");
            let key = NSString::from_str("NSTreatUnknownArgumentsAsOpen");
            let keys = vec![key.as_ref()];
            let objects = vec![Retained::cast::<AnyObject>(NSString::from_str("NO"))];
            let dict = NSDictionary::from_vec(&keys, objects);
            NSUserDefaults::standardUserDefaults().registerDefaults(dict.as_ref());
            crate::write_immediate_crash_log("REGISTER_HANDLER: AppKit configuration completed");
            
            debug!("File handler registration completed");
            crate::write_immediate_crash_log("REGISTER_HANDLER: Function completed successfully");
        }
    }

    /// CRITICAL FIX: Clear corrupted bookmark data that may be causing crashes
    pub fn clear_corrupted_bookmarks() {
        eprintln!("BOOKMARK_CLEANUP: Starting cleanup of potentially corrupted bookmarks");
        
        autoreleasepool(|_pool| unsafe {
            let defaults = NSUserDefaults::standardUserDefaults();
            
            // Safer cleanup: Only remove legacy/broken prefixes and debug counters
            // DO NOT remove new-format VSBookmark_ keys, as they hold valid bookmarks
            let bookmark_prefixes = [
                "ViewSkaterSecurityBookmark_", // legacy broken format
                // "VSBookmark_",               // KEEP new format intact
                "ViewSkaterLastCrashLog",
                "ViewSkaterCrashCounter",
                "ViewSkaterImmediateCrashLog",
            ];
            
            for prefix in &bookmark_prefixes {
                eprintln!("BOOKMARK_CLEANUP: Clearing keys with prefix: {}", prefix);
                
                // Create a simple test key to see if any exist
                for i in 0..50 { // Check up to 50 possible entries
                    let test_key_str = format!("{}{}", prefix, i);
                    let test_key = NSString::from_str(&test_key_str);
                    
                    let obj: *mut AnyObject = msg_send![&*defaults, objectForKey: &*test_key];
                    if !obj.is_null() {
                        eprintln!("BOOKMARK_CLEANUP: Removing key: {}", test_key_str);
                        let _: () = msg_send![&*defaults, removeObjectForKey: &*test_key];
                    }
                }
                
                // Also try to remove the base prefix key
                let base_key = NSString::from_str(prefix);
                let _: () = msg_send![&*defaults, removeObjectForKey: &*base_key];
            }
            
            // Also clean up any entries that match the exact pattern we use for the legacy format
            let common_paths = [
                "/Users", "/Applications", "/Documents", "/Desktop", "/Downloads",
                "/Pictures", "/Movies", "/Music", "/Library",
            ];
            
            for path in &common_paths {
                let normalized = normalize_path_for_key(path);
                let key_str = format!("ViewSkaterSecurityBookmark_{}", normalized);
                let key = NSString::from_str(&key_str);
                
                eprintln!("BOOKMARK_CLEANUP: Attempting to remove: {}", key_str);
                let _: () = msg_send![&*defaults, removeObjectForKey: &*key];
            }
            
            // Force synchronization to ensure cleanup is persisted
            let sync_result: bool = msg_send![&*defaults, synchronize];
            eprintln!("BOOKMARK_CLEANUP: Synchronization result: {}", sync_result);
            eprintln!("BOOKMARK_CLEANUP: Cleanup completed successfully");
        });
    }

    /// SIMPLIFIED: Resolve security-scoped bookmark and return the actual NSURL instance for direct use
    /// The key insight: we must use the resolved NSURL instance directly, not convert it to a path string
    fn resolve_security_scoped_bookmark_for_direct_use(directory_path: &str) -> Option<Retained<NSURL>> {
        if DISABLE_BOOKMARK_RESTORATION {
            eprintln!("RESOLVE_DIRECT: disabled - skipping");
            return None;
        }
        
        eprintln!("RESOLVE_DIRECT: Starting direct resolution for path: {}", directory_path);
        crate::write_immediate_crash_log(&format!("RESOLVE_DIRECT: begin path={}", directory_path));
        
        let result = autoreleasepool(|pool| unsafe {
            // Validate input
            if directory_path.is_empty() || directory_path.len() > 500 {
                crate::write_immediate_crash_log("RESOLVE_DIRECT: invalid path");
                return None;
            }
            
            let defaults = NSUserDefaults::standardUserDefaults();
            
            // Try direct lookup first
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            let mut bookmark_data: *mut AnyObject = msg_send![&*defaults, objectForKey: &*modern_key];
            if bookmark_data.is_null() {
                bookmark_data = msg_send![&*defaults, objectForKey: &*legacy_key];
            }
            
            // If direct lookup failed, try tolerant restore (simplified)
            if bookmark_data.is_null() {
                crate::write_immediate_crash_log("RESOLVE_DIRECT: direct lookup failed, attempting tolerant restore");
                
                let dict: *mut AnyObject = msg_send![&*defaults, dictionaryRepresentation];
                if !dict.is_null() {
                    let nsdict = &*(dict as *const NSDictionary<NSString, AnyObject>);
                    let keys = nsdict.allKeys();
                    
                    let target_path_normalized = std::fs::canonicalize(directory_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| directory_path.to_string());
                    
                    for i in 0..keys.len().min(5) {  // Limit to 5 keys for performance
                        let k = &keys[i];
                        let ks = k.as_str(pool).to_owned();
                        
                        if ks.starts_with("VSBookmark") {
                            let test_bookmark: *mut AnyObject = msg_send![&*defaults, objectForKey: k];
                            if !test_bookmark.is_null() {
                                // Quick test resolve to check path match
                                let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
                                let mut error: *mut AnyObject = std::ptr::null_mut();
                                
                                let test_resolved: *mut AnyObject = msg_send![
                                    objc2::runtime::AnyClass::get("NSURL").unwrap(),
                                    URLByResolvingBookmarkData: test_bookmark
                                    options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                                    relativeToURL: std::ptr::null::<AnyObject>()
                                    bookmarkDataIsStale: &mut is_stale
                                    error: &mut error
                                ];
                                
                                if error.is_null() && !test_resolved.is_null() {
                                    if let Some(path_nsstring) = (&*(test_resolved as *const NSURL)).path() {
                                        let resolved_path = path_nsstring.as_str(pool).to_owned();
                                        let resolved_normalized = std::fs::canonicalize(&resolved_path)
                                            .map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_else(|_| resolved_path.clone());
                                        
                                        if resolved_normalized == target_path_normalized {
                                            crate::write_immediate_crash_log(&format!("RESOLVE_DIRECT: tolerant match found! using key '{}'", ks));
                                            bookmark_data = test_bookmark;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if bookmark_data.is_null() {
                    crate::write_immediate_crash_log("RESOLVE_DIRECT: no bookmark found after tolerant restore");
                    return None;
                }
            }
            
            // Verify it's NSData
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            if !is_nsdata {
                crate::write_immediate_crash_log("RESOLVE_DIRECT: object for key not NSData");
                return None;
            }
            
            crate::write_immediate_crash_log("RESOLVE_DIRECT: attempting URLByResolvingBookmarkData");
            
            // Resolve bookmark to get NEW security-scoped URL instance
            let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            let resolved_url: *mut AnyObject = msg_send![
                objc2::runtime::AnyClass::get("NSURL").unwrap(),
                URLByResolvingBookmarkData: bookmark_data
                options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                relativeToURL: std::ptr::null::<AnyObject>()
                bookmarkDataIsStale: &mut is_stale
                error: &mut error
            ];
            
            if !error.is_null() || resolved_url.is_null() {
                crate::write_immediate_crash_log("RESOLVE_DIRECT: URLByResolvingBookmarkData failed");
                return None;
            }
            
            // Verify it's an NSURL
            let nsurl_class = objc2::runtime::AnyClass::get("NSURL").unwrap();
            let is_nsurl: bool = msg_send![resolved_url, isKindOfClass: nsurl_class];
            if !is_nsurl {
                crate::write_immediate_crash_log("RESOLVE_DIRECT: resolved object not NSURL");
                return None;
            }
            
            // Log resolved path
            if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                let rs = path_nsstring.as_str(pool).to_owned();
                crate::write_immediate_crash_log(&format!("RESOLVE_DIRECT: resolved_url.path='{}'", rs));
            }
            
            // CRITICAL: Call startAccessingSecurityScopedResource on the RESOLVED URL instance
            crate::write_immediate_crash_log("RESOLVE_DIRECT: calling startAccessingSecurityScopedResource on resolved URL");
            let access_granted: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
            crate::write_immediate_crash_log(&format!("RESOLVE_DIRECT: startAccessingSecurityScopedResource={}", access_granted));
            
            if access_granted {
                // Handle stale bookmarks
                if is_stale.as_bool() {
                    crate::write_immediate_crash_log("RESOLVE_DIRECT: bookmark is stale, refreshing");
                    let fresh_bookmark_result: *mut AnyObject = msg_send![
                        resolved_url,
                        bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                        includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                        relativeToURL: std::ptr::null::<AnyObject>()
                        error: std::ptr::null_mut::<*mut AnyObject>()
                    ];
                    
                    if !fresh_bookmark_result.is_null() {
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*modern_key];
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark_result forKey: &*legacy_key];
                        let _: bool = msg_send![&*defaults, synchronize];
                        crate::write_immediate_crash_log("RESOLVE_DIRECT: refreshed stale bookmark");
                    }
                }
                
                // Create Retained<NSURL> from the resolved URL
                let _: *mut AnyObject = msg_send![resolved_url, retain];
                let nsurl_ptr = resolved_url as *mut NSURL;
                
                if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                    crate::write_immediate_crash_log("RESOLVE_DIRECT: SUCCESS - returning resolved URL for direct use");
                    
                    // Store for session use (but don't over-complicate with caching)
                    store_security_scoped_url_with_info(directory_path, retained_url.clone(), true);
                    Some(retained_url)
                } else {
                    crate::write_immediate_crash_log("RESOLVE_DIRECT: failed to retain resolved URL");
                    let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                    None
                }
            } else {
                crate::write_immediate_crash_log("RESOLVE_DIRECT: startAccessingSecurityScopedResource failed");
                
                // Log diagnostic info
                let url_is_file_url: bool = msg_send![resolved_url, isFileURL];
                if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                    let path_str = path_nsstring.as_str(pool).to_owned();
                    let path_exists = std::path::Path::new(&path_str).exists();
                    let path_is_dir = std::path::Path::new(&path_str).is_dir();
                    crate::write_immediate_crash_log(&format!(
                        "RESOLVE_DIRECT: diagnostics - isFileURL={} pathExists={} pathIsDir={} path='{}'", 
                        url_is_file_url, path_exists, path_is_dir, path_str
                    ));
                } else {
                    crate::write_immediate_crash_log("RESOLVE_DIRECT: no path available from resolved URL");
                }
                
                None
            }
        });
        
        match &result {
            Some(_) => {
                eprintln!("RESOLVE_DIRECT: SUCCESS");
                crate::write_immediate_crash_log("RESOLVE_DIRECT: success");
            }
            None => {
                eprintln!("RESOLVE_DIRECT: FAILURE");
                crate::write_immediate_crash_log("RESOLVE_DIRECT: failure");
            }
        }
        
        result
    }
    
    /// Get a resolved NSURL instance for direct file operations (not a path string!)
    /// This follows Apple's documented pattern - resolve fresh each time, use the exact returned instance
    pub fn get_resolved_security_scoped_url(directory_path: &str) -> Option<Retained<NSURL>> {
        if DISABLE_BOOKMARK_RESTORATION {
            crate::write_immediate_crash_log("RESOLVE_SIMPLE: DISABLED - bookmark restoration is disabled");
            return None;
        }
        
        crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Starting for path: {}", directory_path));
        
        autoreleasepool(|pool| unsafe {
            let defaults = NSUserDefaults::standardUserDefaults();
            let (modern_key, legacy_key) = make_bookmark_keys(directory_path);
            
            crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Looking for keys - modern:'{}' legacy:'{}'", 
                modern_key.as_str(pool), legacy_key.as_str(pool)));
            
            // Get bookmark data
            let mut bookmark_data: *mut AnyObject = msg_send![&*defaults, objectForKey: &*modern_key];
            let mut used_modern = true;
            if bookmark_data.is_null() {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: Modern key not found, trying legacy");
                bookmark_data = msg_send![&*defaults, objectForKey: &*legacy_key];
                used_modern = false;
            }
            
            if bookmark_data.is_null() {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: No bookmark found (neither modern nor legacy)");
                return None;
            }
            
            crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Found bookmark using {} key", 
                if used_modern { "modern" } else { "legacy" }));
            
            // Verify it's NSData and get size
            let nsdata_class = objc2::runtime::AnyClass::get("NSData").unwrap();
            let is_nsdata: bool = msg_send![bookmark_data, isKindOfClass: nsdata_class];
            if !is_nsdata {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: ERROR - bookmark data is not NSData, removing");
                let _: () = msg_send![&*defaults, removeObjectForKey: &*modern_key];
                let _: () = msg_send![&*defaults, removeObjectForKey: &*legacy_key];
                return None;
            }
            
            let bookmark_size: usize = msg_send![bookmark_data, length];
            crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Bookmark data is valid NSData, size={} bytes", bookmark_size));
            
            // CRITICAL: Resolve bookmark to get NEW URL instance - MUST use this exact instance
            let mut is_stale: objc2::runtime::Bool = objc2::runtime::Bool::new(false);
            let mut error: *mut AnyObject = std::ptr::null_mut();
            
            crate::write_immediate_crash_log("RESOLVE_SIMPLE: Calling URLByResolvingBookmarkData with security scope");
            let resolved_url: *mut AnyObject = msg_send![
                objc2::runtime::AnyClass::get("NSURL").unwrap(),
                URLByResolvingBookmarkData: bookmark_data
                options: NSURL_BOOKMARK_RESOLUTION_WITH_SECURITY_SCOPE
                relativeToURL: std::ptr::null::<AnyObject>()
                bookmarkDataIsStale: &mut is_stale
                error: &mut error
            ];
            
            // Enhanced error diagnostics
            if !error.is_null() {
                // Try to get error description
                let error_desc: *mut AnyObject = msg_send![error, localizedDescription];
                if !error_desc.is_null() {
                    let desc_nsstring = &*(error_desc as *const NSString);
                    let error_msg = desc_nsstring.as_str(pool);
                    crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: URLByResolvingBookmarkData ERROR: {}", error_msg));
                } else {
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: URLByResolvingBookmarkData failed with unknown error");
                }
                return None;
            }
            
            if resolved_url.is_null() {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: URLByResolvingBookmarkData returned null URL (no error)");
                return None;
            }
            
            crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: URLByResolvingBookmarkData succeeded, is_stale={}", is_stale.as_bool()));
            
            // Verify it's NSURL
            let nsurl_class = objc2::runtime::AnyClass::get("NSURL").unwrap();
            let is_nsurl: bool = msg_send![resolved_url, isKindOfClass: nsurl_class];
            if !is_nsurl {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: ERROR - resolved object is not NSURL");
                return None;
            }
            
            // Get resolved path for logging
            if let Some(path_nsstring) = (&*(resolved_url as *const NSURL)).path() {
                let resolved_path = path_nsstring.as_str(pool);
                crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Resolved URL path: '{}'", resolved_path));
                
                // Check if path exists and is accessible
                let path_exists = std::path::Path::new(resolved_path).exists();
                let path_is_dir = std::path::Path::new(resolved_path).is_dir();
                crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Path diagnostics - exists={} is_dir={}", path_exists, path_is_dir));
            } else {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: WARNING - resolved URL has no path");
            }
            
            // URL property diagnostics
            let url_is_file_url: bool = msg_send![resolved_url, isFileURL];
            let url_has_directory_path: bool = msg_send![resolved_url, hasDirectoryPath];
            crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: URL properties - isFileURL={} hasDirectoryPath={}", 
                url_is_file_url, url_has_directory_path));
            
            // CRITICAL: Call startAccessingSecurityScopedResource on the EXACT SAME instance
            crate::write_immediate_crash_log("RESOLVE_SIMPLE: About to call startAccessingSecurityScopedResource on resolved URL instance");
            let access_granted: bool = msg_send![resolved_url, startAccessingSecurityScopedResource];
            crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: startAccessingSecurityScopedResource={}", access_granted));
            
            if access_granted {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: Security scope activated successfully");
                
                // Handle stale bookmarks
                if is_stale.as_bool() {
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: Bookmark is stale, refreshing");
                    let fresh_bookmark: *mut AnyObject = msg_send![
                        resolved_url,
                        bookmarkDataWithOptions: NSURL_BOOKMARK_CREATION_WITH_SECURITY_SCOPE
                        includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                        relativeToURL: std::ptr::null::<AnyObject>()
                        error: std::ptr::null_mut::<*mut AnyObject>()
                    ];
                    
                    if !fresh_bookmark.is_null() {
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark forKey: &*modern_key];
                        let _: () = msg_send![&*defaults, setObject: fresh_bookmark forKey: &*legacy_key];
                        let sync_result: bool = msg_send![&*defaults, synchronize];
                        crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Refreshed stale bookmark, sync_result={}", sync_result));
                    } else {
                        crate::write_immediate_crash_log("RESOLVE_SIMPLE: WARNING - failed to create fresh bookmark for stale data");
                    }
                }
                
                // Return the resolved URL instance
                let _: *mut AnyObject = msg_send![resolved_url, retain];
                let nsurl_ptr = resolved_url as *mut NSURL;
                
                if let Some(retained_url) = Retained::from_raw(nsurl_ptr) {
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: SUCCESS - returning active security-scoped URL");
                    Some(retained_url)
                } else {
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: ERROR - failed to create Retained<NSURL>");
                    let _: () = msg_send![resolved_url, stopAccessingSecurityScopedResource];
                    None
                }
            } else {
                crate::write_immediate_crash_log("RESOLVE_SIMPLE: FAILURE - startAccessingSecurityScopedResource returned false");
                
                // Enhanced failure diagnostics
                
                // Check macOS version for known issues
                if let Ok(output) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: macOS version: {}", version));
                    
                    if version.starts_with("15.0") {
                        crate::write_immediate_crash_log("RESOLVE_SIMPLE: WARNING - macOS 15.0 has known ScopedBookmarksAgent bugs");
                    }
                }
                
                // Try to create a non-security-scoped bookmark as a test
                let test_bookmark: *mut AnyObject = msg_send![
                    resolved_url,
                    bookmarkDataWithOptions: 0u64  // No security scope
                    includingResourceValuesForKeys: std::ptr::null::<AnyObject>()
                    relativeToURL: std::ptr::null::<AnyObject>()
                    error: std::ptr::null_mut::<*mut AnyObject>()
                ];
                
                if !test_bookmark.is_null() {
                    let test_size: usize = msg_send![test_bookmark, length];
                    crate::write_immediate_crash_log(&format!("RESOLVE_SIMPLE: Non-security-scoped bookmark creation succeeded (size={})", test_size));
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: This suggests the URL is valid but security scope activation failed");
                } else {
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: Even non-security-scoped bookmark creation failed");
                    crate::write_immediate_crash_log("RESOLVE_SIMPLE: This suggests a deeper issue with the resolved URL");
                }
                
                None
            }
        })
    }

    /// CACHE-FIRST: Resolve security-scoped bookmark only once per session to avoid iOS-style re-resolution issues
    /// This addresses the core problem where URLByResolvingBookmarkData can only be called successfully once
    fn resolve_security_scoped_bookmark_cached(directory_path: &str) -> Option<Retained<NSURL>> {
        // STEP 0: Check if this path has already failed resolution in this session
        if let Ok(failed_paths) = FAILED_BOOKMARK_PATHS.lock() {
            if failed_paths.contains(directory_path) {
                crate::write_immediate_crash_log(&format!("RESOLUTION_BLOCKED: Path previously failed resolution in this session: {}", directory_path));
                debug!("Blocking repeated resolution attempt for previously failed path: {}", directory_path);
                return None;
            }
        }
        
        // STEP 1: Check session cache first - avoid re-resolution if we already have access
        if let Ok(urls) = SECURITY_SCOPED_URLS.lock() {
            if let Some(info) = urls.get(directory_path) {
                if info.has_active_scope {
                    crate::write_immediate_crash_log(&format!("CACHE_HIT: Using cached security-scoped URL for: {}", directory_path));
                    debug!("Using cached security-scoped URL with active scope for: {}", directory_path);
                    return Some(info.url.clone());
                } else {
                    crate::write_immediate_crash_log(&format!("CACHE_STALE: Found cached URL but scope is inactive for: {}", directory_path));
                    debug!("Found cached URL but scope is inactive for: {}", directory_path);
                }
            } else {
                crate::write_immediate_crash_log(&format!("CACHE_MISS: No cached URL found for: {}", directory_path));
                debug!("No cached URL found for: {}", directory_path);
            }
        }
        
        // STEP 2: Check resolved bookmark cache to avoid re-resolution
        if let Ok(cache) = RESOLVED_BOOKMARK_CACHE.lock() {
            if let Some(cached_url) = cache.get(directory_path) {
                crate::write_immediate_crash_log(&format!("BOOKMARK_CACHE_HIT: Using cached resolved URL for: {}", directory_path));
                debug!("Using cached resolved bookmark URL for: {}", directory_path);
                
                // Try to activate security scope on cached URL
                let access_granted: bool = unsafe { msg_send![&**cached_url, startAccessingSecurityScopedResource] };
                crate::write_immediate_crash_log(&format!("BOOKMARK_CACHE: startAccessing on cached URL={}", access_granted));
                
                if access_granted {
                    // Store in session cache with active scope
                    store_security_scoped_url_with_info(directory_path, cached_url.clone(), true);
                    return Some(cached_url.clone());
                } else {
                    crate::write_immediate_crash_log("BOOKMARK_CACHE: Cached URL failed startAccessing - marking as failed");
                    debug!("Cached resolved URL failed startAccessing - marking as failed for this session");
                    
                    // Mark this path as failed to prevent repeated attempts
                    if let Ok(mut failed_paths) = FAILED_BOOKMARK_PATHS.lock() {
                        failed_paths.insert(directory_path.to_string());
                    }
                    return None;
                }
            }
        }
        
        // STEP 3: Only resolve from bookmark if not in any cache (and limit to one attempt per session)
        crate::write_immediate_crash_log(&format!("RESOLVING_FRESH: No valid cache found, resolving bookmark for: {}", directory_path));
        debug!("No valid cache found, resolving bookmark from scratch for: {}", directory_path);
        
        if let Some(resolved_url) = resolve_security_scoped_bookmark_for_direct_use(directory_path) {
            // STEP 4: Cache the resolved URL for future use
            if let Ok(mut cache) = RESOLVED_BOOKMARK_CACHE.lock() {
                cache.insert(directory_path.to_string(), resolved_url.clone());
                crate::write_immediate_crash_log(&format!("BOOKMARK_CACHED: Stored resolved URL in cache for: {}", directory_path));
                debug!("Stored resolved URL in bookmark cache for: {}", directory_path);
            }
            
            Some(resolved_url)
        } else {
            crate::write_immediate_crash_log(&format!("RESOLVE_FAILED: Could not resolve bookmark for: {}", directory_path));
            debug!("Could not resolve bookmark for: {}", directory_path);
            
            // Mark this path as failed to prevent repeated attempts in this session
            if let Ok(mut failed_paths) = FAILED_BOOKMARK_PATHS.lock() {
                failed_paths.insert(directory_path.to_string());
                crate::write_immediate_crash_log(&format!("MARKED_FAILED: Added path to failed resolution cache: {}", directory_path));
            }
            
            None
        }
    }

    /// Read directory contents using the resolved security-scoped NSURL directly
    /// This follows Apple's pattern - use the URL instance directly, don't convert to path
    pub fn read_directory_with_security_scoped_url(directory_path: &str) -> Option<Vec<String>> {
        if let Some(resolved_url) = get_resolved_security_scoped_url(directory_path) {
            let result = autoreleasepool(|pool| unsafe {
                // Use NSFileManager directly with the resolved NSURL
                let file_manager_class = objc2::runtime::AnyClass::get("NSFileManager").unwrap();
                let file_manager: *mut AnyObject = msg_send![file_manager_class, defaultManager];
                let mut error: *mut AnyObject = std::ptr::null_mut();
                
                let contents: *mut AnyObject = msg_send![
                    file_manager,
                    contentsOfDirectoryAtURL: &*resolved_url
                    includingPropertiesForKeys: std::ptr::null::<AnyObject>()
                    options: 0u64
                    error: &mut error
                ];
                
                if !error.is_null() || contents.is_null() {
                    return None;
                }
                
                let nsarray_class = objc2::runtime::AnyClass::get("NSArray").unwrap();
                let is_nsarray: bool = msg_send![contents, isKindOfClass: nsarray_class];
                if !is_nsarray {
                    return None;
                }
                
                let nsarray = &*(contents as *const NSArray<NSURL>);
                let mut file_paths = Vec::new();
                
                for i in 0..nsarray.len() {
                    let url = &nsarray[i];
                    if let Some(path_nsstring) = url.path() {
                        let path_str = path_nsstring.as_str(pool).to_owned();
                        file_paths.push(path_str);
                    }
                }
                
                Some(file_paths)
            });
            
            // Clean up - stop accessing the security scoped resource
            unsafe {
                let _: () = msg_send![&*resolved_url, stopAccessingSecurityScopedResource];
            }
            
            result
        } else {
            None
        }
    }
}

/// Test function to verify all crash logging methods work
/// Call this during startup to confirm logs are being written
pub fn test_crash_logging_methods() {
    crate::write_crash_debug_log("========== CRASH LOGGING TEST START ==========");
    crate::write_crash_debug_log("Testing stderr output");
    crate::write_crash_debug_log("Testing stdout output"); 
    crate::write_crash_debug_log("Testing syslog output");
    crate::write_crash_debug_log("Testing NSUserDefaults output");
    crate::write_crash_debug_log("Testing file output");
    crate::write_crash_debug_log("========== CRASH LOGGING TEST END ==========");
    
    // Test retrieval immediately
    #[cfg(target_os = "macos")]
    {
        let logs = crate::get_crash_debug_logs_from_userdefaults();
        println!("Retrieved logs from UserDefaults:");
        for log in logs {
            println!("  {}", log);
        }
    }
}

// ==================== END CRASH DEBUG LOGGING ====================
