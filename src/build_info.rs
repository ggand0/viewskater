/// Build information captured at compile time
pub struct BuildInfo;

impl BuildInfo {
    /// Get the package version from Cargo.toml
    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    
    /// Get the build timestamp in YYYYMMDD.HHMMSS format
    pub fn build_timestamp() -> &'static str {
        env!("BUILD_TIMESTAMP")
    }
    
    /// Get the full git commit hash
    #[allow(dead_code)]
    pub fn git_hash() -> &'static str {
        env!("GIT_HASH")
    }
    
    /// Get the short git commit hash (first 7 characters)
    pub fn git_hash_short() -> &'static str {
        env!("GIT_HASH_SHORT")
    }
    
    /// Get the target platform (arch-os)
    pub fn target_platform() -> &'static str {
        env!("TARGET_PLATFORM")
    }
    
    /// Get the build profile (debug/release)
    pub fn build_profile() -> &'static str {
        env!("BUILD_PROFILE")
    }
    
    /// Get the combined build string (version.timestamp)
    pub fn build_string() -> &'static str {
        env!("BUILD_STRING")
    }
    
    /// Get the bundle version (macOS specific)
    #[cfg(target_os = "macos")]
    pub fn bundle_version() -> &'static str {
        env!("BUNDLE_VERSION")
    }
    
    /// Get a formatted version string for display
    pub fn display_version() -> String {
        format!("{} ({})", Self::version(), Self::build_timestamp())
    }
    
    /// Get detailed build information for about dialogs
    #[allow(dead_code, unused_mut)]
    pub fn detailed_info() -> String {
        let mut info = format!(
            "Version: {}\nBuild: {}\nCommit: {}\nPlatform: {}\nProfile: {}",
            Self::version(),
            Self::build_timestamp(),
            Self::git_hash_short(),
            Self::target_platform(),
            Self::build_profile()
        );
        
        #[cfg(target_os = "macos")]
        {
            info.push_str(&format!("\nBundle: {}", Self::bundle_version()));
        }
        
        info
    }
    
    /// Get bundle version information for display (returns empty string on non-macOS)
    pub fn bundle_version_display() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            Self::bundle_version()
        }
        #[cfg(not(target_os = "macos"))]
        {
            ""
        }
    }
} 