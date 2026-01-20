use std::path::PathBuf;
use std::sync::Arc;
use std::io::Read;
use std::collections::HashMap;

#[allow(unused_imports)]
use log::{debug, error, warn};

#[derive(Debug, Clone)]
pub enum ArchiveType {
    Zip,
    Rar,
    SevenZ,
}

/// Archive cache that stores reusable archive instances per pane
pub struct ArchiveCache {
    /// Current compressed file being accessed
    current_archive: Option<(PathBuf, ArchiveType)>,
    
    /// Cached ZIP archive instance to avoid reopening the file
    zip_archive: Option<Arc<std::sync::Mutex<zip::ZipArchive<std::io::BufReader<std::fs::File>>>>>,
    
    /// Cached 7z archive instance 
    sevenz_archive: Option<Arc<std::sync::Mutex<sevenz_rust2::ArchiveReader<std::fs::File>>>>,
    
    /// Preloaded file data for small solid archives (filename -> bytes)
    preloaded_data: HashMap<String, Vec<u8>>,
}

impl ArchiveCache {
    pub fn new() -> Self {
        Self {
            current_archive: None,
            zip_archive: None,
            sevenz_archive: None,
            preloaded_data: HashMap::new(),
        }
    }
    
    /// Set the current archive that this cache is working with
    /// Clears existing cache if switching to a different archive file
    pub fn set_current_archive(&mut self, path: PathBuf, archive_type: ArchiveType) {
        // Clear cache if switching to a different archive
        if let Some((current_path, _)) = &self.current_archive {
            if *current_path != path {
                debug!("Switching archives, clearing cache: {current_path:?} -> {path:?}");
                self.clear_cache();
            }
        }
        
        self.current_archive = Some((path, archive_type));
    }
    
    /// Clear all cached archive instances
    pub fn clear_cache(&mut self) {
        self.zip_archive = None;
        self.sevenz_archive = None;
        self.preloaded_data.clear();
        debug!("Archive cache cleared");
    }
    
    /// Add preloaded data for a file (used for solid 7z preloading)
    pub fn add_preloaded_data(&mut self, filename: String, data: Vec<u8>) {
        self.preloaded_data.insert(filename, data);
    }
    
    /// Get preloaded data for a file if available
    pub fn get_preloaded_data(&self, filename: &str) -> Option<&[u8]> {
        self.preloaded_data.get(filename).map(|v| v.as_slice())
    }
    
    /// Clear all preloaded data
    pub fn clear_preloaded_data(&mut self) {
        self.preloaded_data.clear();
    }

    /// Read a file from the current compressed archive
    /// This is the main entry point for archive-only operations
    pub fn read_from_archive(&mut self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (path, archive_type) = match self.current_archive.as_ref() {
            Some((p, t)) => (p.clone(), t.clone()),
            None => return Err("No current archive set".into()),
        };
            
        match archive_type {
            ArchiveType::Zip => self.read_zip_file(&path, filename),
            ArchiveType::Rar => self.read_rar_file(&path, filename),
            ArchiveType::SevenZ => self.read_7z_file(&path, filename),
        }
    }
    
    /// Read a file from ZIP archive using cached ZipArchive instance
    fn read_zip_file(&mut self, path: &PathBuf, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Get or create cached ZIP archive
        if self.zip_archive.is_none() {
            debug!("Creating new ZIP archive instance for {path:?}");
            let file = std::io::BufReader::new(std::fs::File::open(path)?);
            let zip_archive = zip::ZipArchive::new(file)?;
            self.zip_archive = Some(Arc::new(std::sync::Mutex::new(zip_archive)));
        }
        
        // Read from cached archive
        let zip_arc = self.zip_archive.as_ref().unwrap();
        let mut zip = zip_arc.lock().unwrap();
        let mut buffer = Vec::new();
        zip.by_name(filename)?.read_to_end(&mut buffer)?;
        debug!("Read {} bytes from ZIP file: {}", buffer.len(), filename);
        Ok(buffer)
    }
    
    /// Read a file from RAR archive using simple filename comparison
    /// Uses the contributor's straightforward approach - simple and intuitive
    fn read_rar_file(&mut self, path: &PathBuf, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut archive = unrar::Archive::new(path).open_for_processing()?;
        let buffer = Vec::new();
        
        while let Some(header) = archive.read_header()? {
            let entry_filename = header.entry().filename.as_os_str();

            // NOTE: Printing this in the while loop is very slow
            //debug!("reading rar {} ?= {:?}", filename, entry_filename);
            
            archive = if filename == entry_filename {
                let (data, rest) = header.read()?;
                drop(rest);
                debug!("Read {} bytes from RAR file: {}", data.len(), filename);
                return Ok(data);
            } else {
                header.skip()?
            };
        }
        
        Ok(buffer)
    }

    /// Read a file from 7z archive using cached ArchiveReader instance
    fn read_7z_file(&mut self, path: &PathBuf, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Get or create cached 7z archive
        if self.sevenz_archive.is_none() {
            debug!("Creating new 7z archive instance for {path:?}");
            let reader = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
            self.sevenz_archive = Some(Arc::new(std::sync::Mutex::new(reader)));
        }
        
        // Read from cached archive
        let sevenz_arc = self.sevenz_archive.as_ref()
            .ok_or("7z archive not initialized")?;
        let data = match sevenz_arc.lock() {
            Ok(mut sevenz) => sevenz.read_file(filename)?,
            Err(e) => {
                error!("Failed to lock 7z archive: {e}");
                return Err("Failed to lock 7z archive".into());
            }
        };
        
        debug!("Read {} bytes from 7z file: {}", data.len(), filename);
        Ok(data)
    }
    
}

impl Default for ArchiveCache {
    fn default() -> Self {
        Self::new()
    }
}
