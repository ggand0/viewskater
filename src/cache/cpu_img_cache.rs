
use std::io;
use std::fs;
use std::path::PathBuf;
use crate::cache::img_cache::{ImageCache, CachedData, ImageCacheBackend};
use crate::cache::cache_utils::{shift_cache_left, shift_cache_right, load_pos};

use crate::loading_status::LoadingStatus;
use crate::cache::img_cache::{LoadOperation, LoadOperationType};
use std::path::Path;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

pub struct CpuImageCache;

impl ImageCacheBackend for CpuImageCache {

    /*fn load_initial_images(
        &mut self,
        image_paths: &Vec<PathBuf>,
        cache_count: usize,
        current_index: usize,
        current_offset: &mut isize,
        cached_data: &mut [Option<CachedData>],
        cached_image_indices: &mut [isize],
    ) -> Result<(), io::Error> {
        

        let start_index: isize;
        let end_index: isize;
        if current_index <= cache_count {
            start_index = 0;
            end_index = (cache_count * 2 + 1) as isize;
            *current_offset = -(cache_count as isize - current_index as isize);
        } else if current_index > (image_paths.len() - 1) - cache_count {
            start_index = image_paths.len() as isize - cache_count as isize * 2 - 1;
            end_index = image_paths.len() as isize;
            *current_offset = cache_count as isize - ((image_paths.len() - 1) as isize - current_index as isize);
        } else {
            start_index = current_index as isize - cache_count as isize;
            end_index = current_index as isize + cache_count as isize + 1;
        }

        for (i, cache_index) in (start_index..end_index).enumerate() {
            if cache_index < 0 {
                continue;
            }
            if cache_index > image_paths.len() as isize - 1 {
                break;
            }
            let image = self.load_image(&image_paths[cache_index as usize])?;
            cached_data[i] = Some(image);
            cached_image_indices[i] = cache_index;
        }

        Ok(())
    }*/
    fn load_image(&self, path: &Path) -> Result<CachedData, io::Error> {
        println!("CpuCache: Loading image from {:?}", path);
        Ok(CachedData::Cpu(fs::read(path)?))
    }

    fn load_pos(&mut self, new_image: Option<CachedData>, pos: usize, image_index: isize) -> Result<bool, io::Error> {
        match new_image {
            Some(CachedData::Cpu(_)) => {
                println!("CpuCache: Setting image at position {}", pos);
                Ok(pos == image_index as usize)
            }
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid data for CPU cache")),
        }
    }

    fn load_initial_images(&mut self, image_paths: &[PathBuf], cache_count: usize, current_index: usize, cached_data: &mut Vec<Option<CachedData>>, cached_image_indices: &mut Vec<isize>, current_offset: &mut isize) -> Result<(), io::Error> {
        let start_index: isize;
        let end_index: isize;
        if current_index <= cache_count {
            start_index = 0;
            end_index = (cache_count * 2 + 1) as isize;
            *current_offset = -(cache_count as isize - current_index as isize);
        } else if current_index > (image_paths.len() - 1) - cache_count {
            start_index = image_paths.len() as isize - cache_count as isize * 2 - 1;
            end_index = image_paths.len() as isize;
            *current_offset = cache_count as isize - ((image_paths.len() - 1) as isize - current_index as isize);
        } else {
            start_index = current_index as isize - cache_count as isize;
            end_index = current_index as isize + cache_count as isize + 1;
        }

        for (i, cache_index) in (start_index..end_index).enumerate() {
            if cache_index < 0 {
                continue;
            }
            if cache_index > image_paths.len() as isize - 1 {
                break;
            }
            let image = self.load_image(&image_paths[cache_index as usize])?;
            cached_data[i] = Some(image);
            cached_image_indices[i] = cache_index;
        }

        Ok(())
    }



    /*fn load_image(&mut self, path: &Path) -> Result<(), io::Error> {
        let data = std::fs::read(path)?; // Read raw bytes from file
        let index = 0; // Assume some index resolution logic here
        self.cached_data[index] = Some(data);
        Ok(())
    }

    fn load_initial_images(&mut self) -> Result<(), io::Error> {
        let _cache_size = self.base.cache_count * 2 + 1;

        // Calculate the starting & ending indices for the cache array
        let start_index: isize;
        let end_index: isize;
        if self.base.current_index <= self.base.cache_count {
            start_index = 0;
            end_index = (self.base.cache_count * 2 + 1) as isize;
            self.base.current_offset = -(self.base.cache_count as isize - self.base.current_index as isize);
        } else if self.base.current_index > (self.base.image_paths.len()-1) - self.base.cache_count {
            //start_index = (self.base.image_paths.len()-1) as isize - self.base.cache_count as isize ;
            start_index = self.base.image_paths.len() as isize - self.base.cache_count as isize * 2 - 1;
            end_index = (self.base.image_paths.len()) as isize;
            self.base.current_offset = self.base.cache_count  as isize - ((self.base.image_paths.len()-1) as isize - self.base.current_index as isize);
        } else {
            start_index = self.base.current_index as isize - self.base.cache_count as isize;
            end_index = self.base.current_index as isize + self.base.cache_count as isize + 1;
        }
        debug!("start_index: {}, end_index: {}, current_offset: {}", start_index, end_index, self.base.current_offset);
        
        // Fill in the cache array with image paths
        for (i, cache_index) in (start_index..end_index).enumerate() {
            debug!("i: {}, cache_index: {}", i, cache_index);
            if cache_index < 0 {
                continue;
            }
            if cache_index > self.base.image_paths.len() as isize - 1 {
                break;
            }
            let image = self.base.load_image(cache_index as usize)?;
            self.cached_data[i] = Some(image);
            self.base.cached_image_indices[i] = cache_index;
        }

        // Display information about each image
        for (index, image_option) in self.cached_data.iter().enumerate() {
            match image_option {
                Some(image_bytes) => {
                    let image_info = format!("Image {} - Size: {} bytes", index, image_bytes.len());
                    debug!("{}", image_info);
                }
                None => {
                    let no_image_info = format!("No image at index {}", index);
                    debug!("{}", no_image_info);
                }
            }
        }

        // Display the indices
        for (index, cache_index) in self.base.cached_image_indices.iter().enumerate() {
            let index_info = format!("Index {} - Cache Index: {}", index, cache_index);
            debug!("{}", index_info);
        }

        self.base.num_files = self.base.image_paths.len();

        // Set the cache states
        self.base.cache_states = vec![true; self.base.image_paths.len()];

        Ok(())
    }

    #[allow(dead_code)]
    fn load_pos(&mut self, new_image: Option<CachedData>, pos: usize, image_index: isize) -> Result<bool, io::Error> {
        // If `pos` is at the center of the cache return true to reload the current_image
        self.cached_data[pos] = new_image;
        self.base.cached_image_indices[pos] = image_index as isize;
        self.print_cache();

        if pos == self.base.cache_count {
            Ok(true)
        } else {
            Ok(false)
        }
    }*/

}


/*
impl CacheBehavior for CpuCacheBehavior {
    fn get_current_image(&self) -> Result<&CachedData, io::Error> {
        let cache_index = self.base.cache_count; // center element of the cache
        debug!("    Current index: {}, Cache index: {}", self.base.current_index, cache_index);

        if let Some(image_data_option) = self.cached_data.get(cache_index) {
            if let Some(image_data) = image_data_option {
                Ok(image_data)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Image data is not cached",
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid cache index",
            ))
        }
    }

    fn get_initial_image(&self) -> Result<&CachedData, io::Error> {
        let index = self.base.current_index % self.cached_data.len();
        self.cached_data.get(index).and_then(Option::as_ref).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "No initial image found")
        })
    }

    /*fn get_cached_data(&self, index: usize) -> Option<CachedData> {
        self.cached_data
            .get(index)
            .and_then(|opt| opt.clone()) // Clone Vec<u8>
            .map(CachedData::Cpu)
    }*/

    fn get_cached_data(&self) -> Option<&CachedData> {
        self.cached_data.map(|data| CachedData::Cpu(data))
    }

    fn print_cache(&self) {
        for (index, data_option) in self.cached_data.iter().enumerate() {
            match data_option {
                Some(_) => {
                    let cache_info = format!(
                        "Cache Data {} - Index {}: Loaded",
                        index, self.base.cached_image_indices[index]
                    );
                    debug!("{}", cache_info);
                }
                None => {
                    let no_cache_info = format!("No data at index {}", index);
                    debug!("{}", no_cache_info);
                }
            }
        }
    }

    #[allow(dead_code)]
    fn clear_cache(&mut self) {
        self.base.cached_image_indices = vec![-1; self.base.cache_count * 2 + 1];
        self.cached_data.clear();
        self.cached_data.resize_with(self.base.cache_count * 2 + 1, || None);
    }

    fn is_some_at_index(&self, index: usize) -> bool {
        // Check if the cached_data at the given index contains Some<T>
        self.cached_data
            .get(index)
            .map_or(false, |data_option| data_option.is_some())
    }

    fn is_cache_index_within_bounds(&self, index: usize) -> bool {
        if !(0..self.cached_data.len()).contains(&index) {
            debug!(
                "is_cache_index_within_bounds - index: {}, cached_data.len(): {}",
                index,
                self.cached_data.len()
            );
            return false;
        }
        self.is_some_at_index(index)
    }

    fn move_next(&mut self, new_data: Option<CachedData>, image_index: isize) -> Result<bool, io::Error> {
        let cache_index = self.base.current_index % self.cached_data.len();
        self.cached_data[cache_index] = new_data;
        self.base.cached_image_indices[cache_index] = image_index;
        Ok(true)
    }

    fn move_prev(&mut self, new_data: Option<CachedData>, image_index: isize) -> Result<bool, io::Error> {
        let cache_index = (self.base.current_index + self.cached_data.len() - 1) % self.cached_data.len();
        self.cached_data[cache_index] = new_data;
        self.base.cached_image_indices[cache_index] = image_index;
        Ok(true)
    }

    fn move_next_edge(
        &mut self,
        _new_data: Option<CachedData>,
        _data_index: isize,
    ) -> Result<bool, io::Error> {
        if self.base.current_index < self.base.image_paths.len() - 1 {
            Ok(false)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "No more data to display",
            ))
        }
    }

    fn move_prev_edge(
        &mut self,
        _new_data: Option<CachedData>,
        _data_index: isize,
    ) -> Result<bool, io::Error> {
        if self.base.current_index > 0 {
            Ok(false)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "No previous data to display",
            ))
        }
    }
    
}
*/
