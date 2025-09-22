use crate::CURRENT_MEMORY_USAGE;
use crate::LAST_MEMORY_UPDATE;
use std::time::Instant;

#[allow(unused_imports)]
use log::{debug, info, warn, error, trace};

// Import sysinfo for cross-platform memory tracking
use sysinfo::{System, Pid, ProcessesToUpdate};

pub fn update_memory_usage() -> u64 {
    // Check if we should update (only once per second)
    let should_update = {
        if let Ok(last_update) = LAST_MEMORY_UPDATE.lock() {
            last_update.elapsed().as_secs() >= 1
        } else {
            true
        }
    };
    
    if !should_update {
        // Return current value if we're not updating
        return CURRENT_MEMORY_USAGE.lock().map(|mem| *mem).unwrap_or(0);
    }
    
    // Update the timestamp
    if let Ok(mut last_update) = LAST_MEMORY_UPDATE.lock() {
        *last_update = Instant::now();
    }
    
    // Special handling for Apple platforms where sandboxed apps can't access process info
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // Try to get memory info, but don't expect it to work in sandboxed environments
        let memory_used = match try_get_memory_usage() {
            Some(mem) => {
                // If it works, update the global memory tracking
                if let Ok(mut mem_lock) = CURRENT_MEMORY_USAGE.lock() {
                    *mem_lock = mem;
                }
                mem
            },
            None => {
                // Default to -1 as a marker that it's not available
                if let Ok(mut mem_lock) = CURRENT_MEMORY_USAGE.lock() {
                    *mem_lock = u64::MAX; // Special value to indicate unavailable
                }
                u64::MAX
            }
        };
        
        return memory_used;
    }
    
    // For non-Apple platforms, use the original implementation
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        // Use sysinfo for cross-platform memory tracking
        let mut system = System::new();
        let pid = Pid::from_u32(std::process::id());
        
        // Refresh specifically for this process
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        
        let memory_used = if let Some(process) = system.process(pid) {
            let memory = process.memory();
            
            // Update the global memory tracking
            if let Ok(mut mem) = CURRENT_MEMORY_USAGE.lock() {
                *mem = memory;
            }
            
            memory
        } else {
            0
        };
        
        trace!("Memory usage updated: {} bytes", memory_used);
        return memory_used;
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn try_get_memory_usage() -> Option<u64> {
    // Try to get memory usage, but return None if it fails
    // This allows us to gracefully handle the sandboxed environment
    let mut system = System::new();
    let pid = Pid::from_u32(std::process::id());
    
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.memory())
}

pub fn log_memory(label: &str) {
    // Use the unified memory tracking function
    let memory_bytes = update_memory_usage();
    if memory_bytes == u64::MAX {
        info!("MEMORY [N/A] - {} (unavailable in sandbox)", label);
    } else {
        let memory_mb = memory_bytes as f64 / 1024.0 / 1024.0;
        info!("MEMORY [{:.2}MB] - {}", memory_mb, label);
    }
}

/// Check if system has enough memory for a large archive
/// Returns (available_gb, recommended_proceed)
pub fn check_memory_for_archive(archive_size_mb: u64) -> (f64, bool) {
    let mut system = System::new();
    system.refresh_memory();
    
    let available_bytes = system.available_memory();
    let available_gb = available_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
    
    // Recommend proceeding if available memory is at least 2x archive size
    let archive_gb = archive_size_mb as f64 / 1024.0;
    let recommended = available_gb > (archive_gb * 2.0);
    
    debug!("Memory check: Available {:.1}GB, Archive {:.1}GB, Recommended: {}", 
           available_gb, archive_gb, recommended);
    
    (available_gb, recommended)
}