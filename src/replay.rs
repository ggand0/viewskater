use std::time::{Duration, Instant};
use std::path::PathBuf;
use log::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub test_directories: Vec<PathBuf>,
    pub duration_per_directory: Duration,
    pub navigation_interval: Duration,
    pub directions: Vec<ReplayDirection>,
    pub output_file: Option<PathBuf>,
    pub verbose: bool,
    pub iterations: u32,
    pub auto_exit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayDirection {
    Right,
    Left,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ReplayState {
    Inactive,
    LoadingDirectory { directory_index: usize },
    WaitingForReady { directory_index: usize },
    NavigatingRight { start_time: Instant, directory_index: usize },
    NavigatingLeft { start_time: Instant, directory_index: usize },
    Pausing { start_time: Instant, directory_index: usize },
    Finished,
}

#[derive(Debug, Clone)]
pub struct ReplayMetrics {
    pub directory_path: PathBuf,
    pub direction: ReplayDirection,
    pub start_time: Instant,
    pub end_time: Instant,
    pub total_frames: u32,
    pub ui_fps_samples: Vec<f32>,
    pub image_fps_samples: Vec<f32>,
    pub memory_samples: Vec<f64>,
    pub min_ui_fps: f32,
    pub max_ui_fps: f32,
    pub avg_ui_fps: f32,
    pub min_image_fps: f32,
    pub max_image_fps: f32,
    pub avg_image_fps: f32,
    pub min_memory_mb: f64,
    pub max_memory_mb: f64,
    pub avg_memory_mb: f64,
}

impl ReplayMetrics {
    pub fn new(directory_path: PathBuf, direction: ReplayDirection) -> Self {
        let now = Instant::now();
        Self {
            directory_path,
            direction,
            start_time: now,
            end_time: now,
            total_frames: 0,
            ui_fps_samples: Vec::new(),
            image_fps_samples: Vec::new(),
            memory_samples: Vec::new(),
            min_ui_fps: f32::MAX,
            max_ui_fps: 0.0,
            avg_ui_fps: 0.0,
            min_image_fps: f32::MAX,
            max_image_fps: 0.0,
            avg_image_fps: 0.0,
            min_memory_mb: f64::MAX,
            max_memory_mb: 0.0,
            avg_memory_mb: 0.0,
        }
    }

    pub fn add_sample(&mut self, ui_fps: f32, image_fps: f32, memory_mb: f64) {
        self.total_frames += 1;
        
        // Collect samples
        self.ui_fps_samples.push(ui_fps);
        self.image_fps_samples.push(image_fps);
        self.memory_samples.push(memory_mb);
        
        // Update min/max for UI FPS
        if ui_fps < self.min_ui_fps { self.min_ui_fps = ui_fps; }
        if ui_fps > self.max_ui_fps { self.max_ui_fps = ui_fps; }
        
        // Update min/max for Image FPS
        if image_fps < self.min_image_fps { self.min_image_fps = image_fps; }
        if image_fps > self.max_image_fps { self.max_image_fps = image_fps; }
        
        // Update min/max for Memory (if valid)
        if memory_mb >= 0.0 {
            if memory_mb < self.min_memory_mb { self.min_memory_mb = memory_mb; }
            if memory_mb > self.max_memory_mb { self.max_memory_mb = memory_mb; }
        }
    }

    pub fn finalize(&mut self) {
        self.end_time = Instant::now();
        
        // Calculate averages
        if !self.ui_fps_samples.is_empty() {
            self.avg_ui_fps = self.ui_fps_samples.iter().sum::<f32>() / self.ui_fps_samples.len() as f32;
        }
        
        if !self.image_fps_samples.is_empty() {
            self.avg_image_fps = self.image_fps_samples.iter().sum::<f32>() / self.image_fps_samples.len() as f32;
        }
        
        let valid_memory_samples: Vec<f64> = self.memory_samples.iter().filter(|&&m| m >= 0.0).cloned().collect();
        if !valid_memory_samples.is_empty() {
            self.avg_memory_mb = valid_memory_samples.iter().sum::<f64>() / valid_memory_samples.len() as f64;
        }
        
        // Handle edge case where no valid memory samples exist
        if self.min_memory_mb == f64::MAX {
            self.min_memory_mb = -1.0; // Use -1 to indicate N/A
        }
    }

    pub fn duration(&self) -> Duration {
        self.end_time.duration_since(self.start_time)
    }

    pub fn print_summary(&self) {
        let duration = self.duration();
        info!("=== Replay Metrics Summary ===");
        info!("Directory: {}", self.directory_path.display());
        info!("Direction: {:?}", self.direction);
        info!("Duration: {:.2}s", duration.as_secs_f64());
        info!("Total Frames: {}", self.total_frames);
        info!("UI FPS - Min: {:.1}, Max: {:.1}, Avg: {:.1}", self.min_ui_fps, self.max_ui_fps, self.avg_ui_fps);
        info!("Image FPS - Min: {:.1}, Max: {:.1}, Avg: {:.1}", self.min_image_fps, self.max_image_fps, self.avg_image_fps);
        
        if self.min_memory_mb >= 0.0 {
            info!("Memory (MB) - Min: {:.1}, Max: {:.1}, Avg: {:.1}", self.min_memory_mb, self.max_memory_mb, self.avg_memory_mb);
        } else {
            info!("Memory: N/A");
        }
        
        if self.avg_ui_fps < 30.0 {
            warn!("UI FPS below 30 - potential performance issue");
        }
        if self.avg_image_fps < 30.0 {
            warn!("Image FPS below 30 - potential rendering bottleneck");
        }
    }
}

pub struct ReplayController {
    pub config: ReplayConfig,
    pub state: ReplayState,
    pub current_metrics: Option<ReplayMetrics>,
    pub completed_metrics: Vec<ReplayMetrics>,
    pub last_navigation_time: Instant,
    pub current_iteration: u32,
    pub completed_iterations: u32,
}

impl ReplayController {
    pub fn new(config: ReplayConfig) -> Self {
        Self {
            config,
            state: ReplayState::Inactive,
            current_metrics: None,
            completed_metrics: Vec::new(),
            last_navigation_time: Instant::now(),
            current_iteration: 0,
            completed_iterations: 0,
        }
    }

    pub fn start(&mut self) {
        if !self.config.test_directories.is_empty() && self.completed_iterations < self.config.iterations {
            self.current_iteration += 1;
            info!("Starting replay mode iteration {}/{} with {} directories", 
                  self.current_iteration, self.config.iterations, self.config.test_directories.len());
            self.state = ReplayState::LoadingDirectory { directory_index: 0 };
        } else {
            if self.completed_iterations >= self.config.iterations {
                info!("All {} iterations completed", self.config.iterations);
            } else {
                warn!("No test directories configured for replay mode");
            }
            self.state = ReplayState::Finished;
        }
    }

    pub fn is_active(&self) -> bool {
        !matches!(self.state, ReplayState::Inactive | ReplayState::Finished)
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.state, ReplayState::Finished) && self.completed_iterations >= self.config.iterations
    }

    #[allow(dead_code)]
    pub fn should_navigate_right(&self) -> bool {
        match &self.state {
            ReplayState::NavigatingRight { start_time, .. } => {
                let elapsed = start_time.elapsed();
                elapsed < self.config.duration_per_directory &&
                self.last_navigation_time.elapsed() >= self.config.navigation_interval
            }
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub fn should_navigate_left(&self) -> bool {
        match &self.state {
            ReplayState::NavigatingLeft { start_time, .. } => {
                let elapsed = start_time.elapsed();
                elapsed < self.config.duration_per_directory &&
                self.last_navigation_time.elapsed() >= self.config.navigation_interval
            }
            _ => false,
        }
    }

    pub fn get_current_directory(&self) -> Option<&PathBuf> {
        match &self.state {
            ReplayState::LoadingDirectory { directory_index } |
            ReplayState::NavigatingRight { directory_index, .. } |
            ReplayState::NavigatingLeft { directory_index, .. } |
            ReplayState::Pausing { directory_index, .. } => {
                self.config.test_directories.get(*directory_index)
            }
            _ => None,
        }
    }

    pub fn on_navigation_performed(&mut self) {
        self.last_navigation_time = Instant::now();
    }

    pub fn on_directory_loaded(&mut self, directory_index: usize) {
        if let ReplayState::LoadingDirectory { directory_index: expected_index } = &self.state {
            if *expected_index == directory_index {
                let directory_path = self.config.test_directories[directory_index].clone();
                info!("Directory loaded for replay: {}, waiting for app to be ready...", directory_path.display());
                
                // Transition to waiting state instead of immediately starting navigation
                self.state = ReplayState::WaitingForReady { directory_index };
            }
        }
    }
    
    pub fn on_ready_to_navigate(&mut self) {
        if let ReplayState::WaitingForReady { directory_index } = &self.state {
            let directory_index = *directory_index;
            let directory_path = self.config.test_directories[directory_index].clone();
            
            // Start with the first direction for this directory
            let direction = self.config.directions.get(0).unwrap_or(&ReplayDirection::Right);
            
            match direction {
                ReplayDirection::Right => {
                    self.state = ReplayState::NavigatingRight { 
                        start_time: Instant::now(), 
                        directory_index 
                    };
                    self.current_metrics = Some(ReplayMetrics::new(directory_path.clone(), ReplayDirection::Right));
                }
                ReplayDirection::Left => {
                    self.state = ReplayState::NavigatingLeft { 
                        start_time: Instant::now(), 
                        directory_index 
                    };
                    self.current_metrics = Some(ReplayMetrics::new(directory_path.clone(), ReplayDirection::Left));
                }
                ReplayDirection::Both => {
                    // Start with right navigation first
                    self.state = ReplayState::NavigatingRight { 
                        start_time: Instant::now(), 
                        directory_index 
                    };
                    self.current_metrics = Some(ReplayMetrics::new(directory_path.clone(), ReplayDirection::Right));
                }
            }
            
            info!("App ready - started replay navigation for directory: {}", directory_path.display());
        }
    }

    pub fn update_metrics(&mut self, ui_fps: f32, image_fps: f32, memory_mb: f64) {
        if let Some(ref mut metrics) = self.current_metrics {
            metrics.add_sample(ui_fps, image_fps, memory_mb);
        }
    }

    pub fn update(&mut self) -> Option<ReplayAction> {
        match &self.state {
            ReplayState::NavigatingRight { start_time, directory_index } => {
                let elapsed = start_time.elapsed();
                
                if elapsed >= self.config.duration_per_directory {
                    // Finish current metrics
                    if let Some(mut metrics) = self.current_metrics.take() {
                        metrics.finalize();
                        if self.config.verbose {
                            metrics.print_summary();
                        }
                        self.completed_metrics.push(metrics);
                    }
                    
                    // Check if we need to test left navigation for this directory
                    if self.config.directions.contains(&ReplayDirection::Both) {
                        let directory_path = self.config.test_directories[*directory_index].clone();
                        info!("Switching from right to left navigation for directory: {}", directory_path.display());
                        // Switch immediately to left navigation
                        self.state = ReplayState::NavigatingLeft { 
                            start_time: Instant::now(), 
                            directory_index: *directory_index 
                        };
                        self.current_metrics = Some(ReplayMetrics::new(directory_path, ReplayDirection::Left));
                        return Some(ReplayAction::StartNavigatingLeft);
                    } else {
                        // Move to next directory or finish
                        self.advance_to_next_directory(*directory_index)
                    }
                } else if self.last_navigation_time.elapsed() >= self.config.navigation_interval {
                    // Navigate right if enough time has passed since last navigation
                    Some(ReplayAction::NavigateRight)
                } else {
                    // Still within duration but need to wait for navigation interval
                    None
                }
            }
            
            ReplayState::NavigatingLeft { start_time, directory_index } => {
                let elapsed = start_time.elapsed();
                
                // Debug: Log timing during left navigation
                if elapsed.as_millis() % 500 < 50 { // Log every ~500ms
                    debug!("Left navigation progress: {:.2}s / {:.2}s (target: {:.2}s)", 
                           elapsed.as_secs_f64(), 
                           self.config.duration_per_directory.as_secs_f64(),
                           self.config.duration_per_directory.as_secs_f64());
                }
                
                if elapsed >= self.config.duration_per_directory {
                    // Left navigation duration completed - stop regardless of current image index
                    // This is time-based testing, not completion-based (we don't need to reach index 0)
                    if let Some(mut metrics) = self.current_metrics.take() {
                        metrics.finalize();
                        if self.config.verbose {
                            metrics.print_summary();
                        }
                        self.completed_metrics.push(metrics);
                    }
                    
                    // Move to next directory or finish
                    self.advance_to_next_directory(*directory_index)
                } else if self.last_navigation_time.elapsed() >= self.config.navigation_interval {
                    // Navigate left if enough time has passed since last navigation
                    Some(ReplayAction::NavigateLeft)
                } else {
                    // Still within duration but need to wait for navigation interval
                    None
                }
            }
            
            ReplayState::Pausing { start_time, directory_index } => {
                // Brief pause between operations if needed
                if start_time.elapsed() >= Duration::from_millis(100) {
                    self.advance_to_next_directory(*directory_index)
                } else {
                    None
                }
            }
            
            ReplayState::WaitingForReady { .. } => {
                // Wait for app to signal readiness via on_ready_to_navigate()
                None
            }
            
            _ => None,
        }
    }

    fn advance_to_next_directory(&mut self, current_directory_index: usize) -> Option<ReplayAction> {
        let next_index = current_directory_index + 1;
        
        if next_index < self.config.test_directories.len() {
            // Still have more directories in this iteration
            self.state = ReplayState::LoadingDirectory { directory_index: next_index };
            Some(ReplayAction::LoadDirectory(self.config.test_directories[next_index].clone()))
        } else {
            // Completed all directories in this iteration
            self.completed_iterations += 1;
            info!("Completed iteration {}/{}", self.completed_iterations, self.config.iterations);
            
            if self.completed_iterations < self.config.iterations {
                // Start next iteration - use RestartIteration for same directory to avoid reload delay
                info!("Starting next iteration...");
                debug!("Transitioning from completed iteration {} to iteration {}", self.completed_iterations, self.completed_iterations + 1);
                self.current_iteration += 1;
                self.state = ReplayState::LoadingDirectory { directory_index: 0 };
                debug!("Set state to LoadingDirectory, returning RestartIteration action");
                Some(ReplayAction::RestartIteration(self.config.test_directories[0].clone()))
            } else {
                // All iterations completed
                self.state = ReplayState::Finished;
                info!("Replay mode completed! All {} iterations finished.", self.config.iterations);
                self.print_final_summary();
                Some(ReplayAction::Finish)
            }
        }
    }

    pub fn print_final_summary(&self) {
        info!("=== FINAL REPLAY SUMMARY ===");
        info!("Total directories tested: {}", self.completed_metrics.len());
        
        if !self.completed_metrics.is_empty() {
            let total_ui_fps: f32 = self.completed_metrics.iter().map(|m| m.avg_ui_fps).sum();
            let total_image_fps: f32 = self.completed_metrics.iter().map(|m| m.avg_image_fps).sum();
            let count = self.completed_metrics.len() as f32;
            
            info!("Overall Average UI FPS: {:.1}", total_ui_fps / count);
            info!("Overall Average Image FPS: {:.1}", total_image_fps / count);
            
            let min_ui_fps = self.completed_metrics.iter().map(|m| m.min_ui_fps).fold(f32::INFINITY, f32::min);
            let max_ui_fps = self.completed_metrics.iter().map(|m| m.max_ui_fps).fold(0.0, f32::max);
            
            info!("UI FPS Range: {:.1} - {:.1}", min_ui_fps, max_ui_fps);
            
            // Export to file if requested
            if let Some(ref output_file) = self.config.output_file {
                if let Err(e) = self.export_metrics_to_file(output_file) {
                    warn!("Failed to export metrics to file: {}", e);
                } else {
                    info!("Metrics exported to: {}", output_file.display());
                }
            }
        }
    }

    fn export_metrics_to_file(&self, output_file: &PathBuf) -> Result<(), std::io::Error> {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(output_file)?;
        
        writeln!(file, "ViewSkater Replay Benchmark Results")?;
        writeln!(file, "Generated: {:?}", std::time::SystemTime::now())?;
        writeln!(file)?;
        
        for (i, metrics) in self.completed_metrics.iter().enumerate() {
            writeln!(file, "Test {}: {}", i + 1, metrics.directory_path.display())?;
            writeln!(file, "Direction: {:?}", metrics.direction)?;
            writeln!(file, "Duration: {:.2}s", metrics.duration().as_secs_f64())?;
            writeln!(file, "Total Frames: {}", metrics.total_frames)?;
            writeln!(file, "UI FPS - Min: {:.1}, Max: {:.1}, Avg: {:.1}", 
                     metrics.min_ui_fps, metrics.max_ui_fps, metrics.avg_ui_fps)?;
            writeln!(file, "Image FPS - Min: {:.1}, Max: {:.1}, Avg: {:.1}", 
                     metrics.min_image_fps, metrics.max_image_fps, metrics.avg_image_fps)?;
            
            if metrics.min_memory_mb >= 0.0 {
                writeln!(file, "Memory (MB) - Min: {:.1}, Max: {:.1}, Avg: {:.1}", 
                         metrics.min_memory_mb, metrics.max_memory_mb, metrics.avg_memory_mb)?;
            } else {
                writeln!(file, "Memory: N/A")?;
            }
            writeln!(file)?;
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ReplayAction {
    LoadDirectory(PathBuf),
    RestartIteration(PathBuf), // Same directory, new iteration - no need to reload
    NavigateRight,
    NavigateLeft,
    StartNavigatingLeft,
    Finish,
}
