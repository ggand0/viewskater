use {
    std::{
        env,
        io,
        process::Command,
        fs,
    },
    winres::WindowsResource,
};

fn main() -> io::Result<()> {
    // Capture build information
    capture_build_info();

    // Windows resource setup
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        WindowsResource::new()
            // This path can be absolute, or relative to your crate root.
            // When building on Windows, comment out the first 3 lines of this block (just call set_icon)
            //.set_toolkit_path("/usr/bin")
            //.set_windres_path("x86_64-w64-mingw32-windres")
            //.set_ar_path("x86_64-w64-mingw32-ar")
            .set_icon("./assets/icon.ico")
            .compile()?;
    }
    Ok(())
}

fn capture_build_info() {
    // Uncomment to override version string if needed:
    //println!("cargo:rustc-env=CARGO_PKG_VERSION={}", env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string()));

    // Generate build timestamp
    let build_timestamp = chrono::Utc::now().format("%Y%m%d.%H%M%S").to_string();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp);

    // Get git commit hash
    let git_hash = get_git_hash().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // Get git commit hash (short version)
    let git_hash_short = if git_hash.len() >= 7 {
        git_hash[0..7].to_string()
    } else {
        git_hash.clone()
    };
    println!("cargo:rustc-env=GIT_HASH_SHORT={}", git_hash_short);

    // Target platform info
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=TARGET_PLATFORM={}-{}", target_arch, target_os);

    // Build profile
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);

    // Create a combined build string
    let build_string = format!("{}.{}", env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string()), build_timestamp);
    println!("cargo:rustc-env=BUILD_STRING={}", build_string);

    // For macOS, automatically update Info.plist with the build timestamp
    if target_os == "macos" {
        update_info_plist(&build_timestamp);
        println!("cargo:rustc-env=BUNDLE_VERSION={}", build_timestamp);
    } else {
        // For non-macOS, still set the bundle version but don't update plist
        println!("cargo:rustc-env=BUNDLE_VERSION={}", build_timestamp);
    }

    // Tell cargo to rerun this if git changes or Info.plist changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");
    println!("cargo:rerun-if-changed=resources/macos/Info.plist");
}

fn get_git_hash() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let hash = String::from_utf8(output.stdout).ok()?;
        Some(hash.trim().to_string())
    } else {
        None
    }
}

fn update_info_plist(build_timestamp: &str) {
    let plist_path = "resources/macos/Info.plist";

    // Check if Info.plist exists
    if !std::path::Path::new(plist_path).exists() {
        println!("cargo:warning=Info.plist not found at {}, skipping update", plist_path);
        return;
    }

    // Read the current Info.plist
    let content = match fs::read_to_string(plist_path) {
        Ok(content) => content,
        Err(e) => {
            println!("cargo:warning=Failed to read Info.plist: {}", e);
            return;
        }
    };

    // Update CFBundleVersion using regex replacement
    let updated_content = if let Some(start) = content.find("<key>CFBundleVersion</key>") {
        if let Some(value_start) = content[start..].find("<string>") {
            if let Some(value_end) = content[start + value_start + 8..].find("</string>") {
                let before = &content[..start + value_start + 8];
                let after = &content[start + value_start + 8 + value_end..];
                format!("{}{}{}", before, build_timestamp, after)
            } else {
                content
            }
        } else {
            content
        }
    } else {
        content
    };

    // Write back the updated content
    if let Err(e) = fs::write(plist_path, updated_content) {
        println!("cargo:warning=Failed to write updated Info.plist: {}", e);
    } else {
        println!("cargo:warning=Updated CFBundleVersion in Info.plist to {}", build_timestamp);
    }
}