use std::io;

#[allow(unused_imports)]
use log::{debug, info, warn, error};

/// Shift the cache array to the left, adding a new item at the end.
/// Updates the indices accordingly.
pub fn shift_cache_left<T>(
    cached_items: &mut Vec<Option<T>>,
    cached_indices: &mut Vec<isize>,
    new_item: Option<T>,
    current_offset: &mut isize,
) {
    cached_items.remove(0);
    cached_items.push(new_item);

    // Update indices
    cached_indices.remove(0);
    let next_index = cached_indices[cached_indices.len() - 1] + 1;
    cached_indices.push(next_index);

    *current_offset -= 1;
    debug!("shift_cache_left - current_offset: {}", current_offset);
}

/// Shift the cache array to the right, adding a new item at the front.
/// Updates the indices accordingly.
pub fn shift_cache_right<T>(
    cached_items: &mut Vec<Option<T>>,
    cached_indices: &mut Vec<isize>,
    new_item: Option<T>,
    current_offset: &mut isize,
) {
    cached_items.pop();
    cached_items.insert(0, new_item);

    // Update indices
    cached_indices.pop();
    let prev_index = cached_indices[0] - 1;
    cached_indices.insert(0, prev_index);

    *current_offset += 1;
    debug!("shift_cache_right - current_offset: {}", current_offset);
}

/// Load an item into a specific position in the cache.
/// Returns `true` if the position corresponds to the center of the cache.
pub fn load_pos<T>(
    cached_items: &mut Vec<Option<T>>,
    cached_indices: &mut Vec<isize>,
    pos: usize,
    item: Option<T>,
    image_index: isize,
    cache_count: usize,
) -> Result<bool, io::Error> {
    if pos >= cached_items.len() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Position out of bounds",
        ));
    }

    cached_items[pos] = item;
    cached_indices[pos] = image_index;

    if pos == cache_count {
        Ok(true) // Center of the cache
    } else {
        Ok(false)
    }
}
