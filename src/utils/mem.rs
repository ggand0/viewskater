use crate::CURRENT_MEMORY_USAGE;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

pub fn update_memory_usage() -> f64 {
    #[cfg(target_os = "linux")]
    {
        use std::fs::File;
        use std::io::Read;
        
        let mut status = String::new();
        if let Ok(mut file) = File::open("/proc/self/status") {
            let _ = file.read_to_string(&mut status);
        }
        
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<f64>() {
                        let mb = kb / 1024.0;
                        // Update global memory tracking
                        if let Ok(mut mem) = CURRENT_MEMORY_USAGE.lock() {
                            *mem = (mb * 1024.0 * 1024.0) as u64;
                        }
                        return mb;
                    }
                }
            }
        }
    }
    
    0.0
}

pub fn log_memory(label: &str) {
    let mem_mb = update_memory_usage();
    info!("MEMORY [{:.2}MB] - {}", mem_mb, label);
}