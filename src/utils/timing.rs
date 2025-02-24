use std::time::{Duration, Instant};
use log::{debug, info};

pub struct TimingStats {
    pub name: String,
    pub total_time: Duration,
    pub count: u32,
}

impl TimingStats {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            total_time: Duration::from_secs(0),
            count: 0,
        }
    }

    pub fn add_measurement(&mut self, duration: Duration) {
        self.total_time += duration;
        self.count += 1;
        
        let avg_ms = self.average_ms();
        info!("{} - Current: {:.2}ms, Avg: {:.2}ms, Count: {}", 
            self.name,
            duration.as_secs_f64() * 1000.0,
            avg_ms,
            self.count
        );
    }

    pub fn average_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            (self.total_time.as_secs_f64() * 1000.0) / self.count as f64
        }
    }
}

pub struct ScopedTimer<'a> {
    start: Instant,
    stats: &'a mut TimingStats,
}

impl<'a> ScopedTimer<'a> {
    pub fn new(stats: &'a mut TimingStats) -> Self {
        Self {
            start: Instant::now(),
            stats,
        }
    }
}

impl<'a> Drop for ScopedTimer<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.stats.add_measurement(duration);
    }
}