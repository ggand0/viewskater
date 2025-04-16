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
    memory_used
}

pub fn log_memory(label: &str) {
    // Use the unified memory tracking function
    let memory_bytes = update_memory_usage();
    let memory_mb = memory_bytes as f64 / 1024.0 / 1024.0;
    info!("MEMORY [{:.2}MB] - {}", memory_mb, label);
}