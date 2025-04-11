//! A minimal BC1 compression library in pure Rust.

// Remove `#![no_std]` since we need standard library features like Vec.

/// Represents the compressed BC1 block (8 bytes).
pub type Bc1Block = [u8; 8];

/// Represents an uncompressed 4x4 block of RGBA pixels.
pub type RgbaBlock = [[u8; 4]; 16];

/// Defines available compression algorithms
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum CompressionAlgorithm {
    /// RangeFit algorithm - faster with good quality
    RangeFit,
}

impl Default for CompressionAlgorithm {
    fn default() -> Self {
        CompressionAlgorithm::RangeFit
    }
}

/// Converts RGB values to the RGB565 format.
fn rgb_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}

/// Computes the Euclidean distance between two colors.
fn color_distance(c1: (u8, u8, u8), c2: (u8, u8, u8)) -> f32 {
    let dr = c1.0 as f32 - c2.0 as f32;
    let dg = c1.1 as f32 - c2.1 as f32;
    let db = c1.2 as f32 - c2.2 as f32;
    (dr * dr + dg * dg + db * db).sqrt()
}

/// Computes the principal axis using covariance matrix
fn compute_principal_axis(colors: &[(u8, u8, u8)]) -> (f32, f32, f32) {
    let mut sum_r = 0.0f32;
    let mut sum_g = 0.0f32;
    let mut sum_b = 0.0f32;
    let mut count = 0.0f32;

    // Calculate mean color
    for &(r, g, b) in colors {
        sum_r += r as f32;
        sum_g += g as f32;
        sum_b += b as f32;
        count += 1.0;
    }

    let mean_r = sum_r / count;
    let mean_g = sum_g / count;
    let mean_b = sum_b / count;

    // Calculate covariance matrix
    let mut cov = [[0.0f32; 3]; 3];
    for &(r, g, b) in colors {
        let dr = (r as f32) - mean_r;
        let dg = (g as f32) - mean_g;
        let db = (b as f32) - mean_b;

        cov[0][0] += dr * dr;
        cov[0][1] += dr * dg;
        cov[0][2] += dr * db;
        cov[1][1] += dg * dg;
        cov[1][2] += dg * db;
        cov[2][2] += db * db;
    }

    cov[1][0] = cov[0][1];
    cov[2][0] = cov[0][2];
    cov[2][1] = cov[1][2];

    // Simple power iteration to find principal eigenvector
    let mut axis = (1.0f32, 1.0f32, 1.0f32);
    for _ in 0..4 {
        let x = cov[0][0] * axis.0 + cov[0][1] * axis.1 + cov[0][2] * axis.2;
        let y = cov[1][0] * axis.0 + cov[1][1] * axis.1 + cov[1][2] * axis.2;
        let z = cov[2][0] * axis.0 + cov[2][1] * axis.1 + cov[2][2] * axis.2;
        
        let length = (x * x + y * y + z * z).sqrt();
        if length > 0.0 {
            axis = (x / length, y / length, z / length);
        }
    }

    axis
}

/// Compresses a 4x4 block using the RangeFit algorithm
fn compress_bc1_block_rangefit(block: &RgbaBlock) -> Bc1Block {
    let mut colors: Vec<(u8, u8, u8)> = Vec::new();
    
    // Collect non-transparent pixels
    for &pixel in block.iter() {
        if pixel[3] > 128 {
            colors.push((pixel[0], pixel[1], pixel[2]));
        }
    }

    if colors.is_empty() {
        return [0u8; 8];
    }

    // Get principal axis
    let axis = compute_principal_axis(&colors);

    // Project colors onto principal axis
    let mut min_proj = f32::MAX;
    let mut max_proj = f32::MIN;
    let mut min_color = (0, 0, 0);
    let mut max_color = (0, 0, 0);

    for &(r, g, b) in &colors {
        let proj = (r as f32) * axis.0 + (g as f32) * axis.1 + (b as f32) * axis.2;
        
        if proj < min_proj {
            min_proj = proj;
            min_color = (r, g, b);
        }
        if proj > max_proj {
            max_proj = proj;
            max_color = (r, g, b);
        }
    }

    // Convert endpoints to RGB565
    let color0 = rgb_to_rgb565(min_color.0, min_color.1, min_color.2);
    let color1 = rgb_to_rgb565(max_color.0, max_color.1, max_color.2);

    // Create color palette
    let mut palette = [min_color, max_color, (0, 0, 0), (0, 0, 0)];
    
    if color0 > color1 {
        palette[2] = (
            ((2 * (min_color.0 as u16) + max_color.0 as u16) / 3) as u8,
            ((2 * (min_color.1 as u16) + max_color.1 as u16) / 3) as u8,
            ((2 * (min_color.2 as u16) + max_color.2 as u16) / 3) as u8,
        );
        palette[3] = (
            ((min_color.0 as u16 + 2 * (max_color.0 as u16)) / 3) as u8,
            ((min_color.1 as u16 + 2 * (max_color.1 as u16)) / 3) as u8,
            ((min_color.2 as u16 + 2 * (max_color.2 as u16)) / 3) as u8,
        );
    } else {
        palette[2] = (
            ((min_color.0 as u16 + max_color.0 as u16) / 2) as u8,
            ((min_color.1 as u16 + max_color.1 as u16) / 2) as u8,
            ((min_color.2 as u16 + max_color.2 as u16) / 2) as u8,
        );
        palette[3] = (0, 0, 0);
    }

    // Build indices
    let mut indices = 0u32;
    for (i, &pixel) in block.iter().enumerate() {
        let mut best_index = if pixel[3] <= 128 { 3 } else { 0 };
        let mut best_distance = f32::MAX;

        if pixel[3] > 128 {
            for (j, &palette_color) in palette.iter().enumerate() {
                let distance = color_distance((pixel[0], pixel[1], pixel[2]), palette_color);
                if distance < best_distance {
                    best_distance = distance;
                    best_index = j;
                }
            }
        }

        indices |= (best_index as u32) << (2 * i);
    }

    let mut block_data = [0u8; 8];
    block_data[0..2].copy_from_slice(&color0.to_le_bytes());
    block_data[2..4].copy_from_slice(&color1.to_le_bytes());
    block_data[4..8].copy_from_slice(&indices.to_le_bytes());

    block_data
}

/// Compresses a 4x4 block of RGBA pixels using the specified algorithm
pub fn compress_bc1_block(block: &RgbaBlock, algorithm: CompressionAlgorithm) -> Bc1Block {
    match algorithm {
        CompressionAlgorithm::RangeFit => compress_bc1_block_rangefit(block),
    }
}

/// Compresses an entire image of RGBA pixels
pub fn compress_image_bc1(
    image: &[u8], 
    width: usize, 
    height: usize,
    algorithm: CompressionAlgorithm
) -> Vec<Bc1Block> {
    use rayon::prelude::*;
    
    // Calculate the positions of all blocks to process
    let blocks_x = (width + 3) / 4;
    let blocks_y = (height + 3) / 4;
    let total_blocks = blocks_x * blocks_y;
    
    // Create a parallel iterator for all block positions
    (0..total_blocks)
        .into_par_iter()
        .map(|block_idx| {
            let block_x = (block_idx % blocks_x) * 4;
            let block_y = (block_idx / blocks_x) * 4;
            
            // Build the 4x4 block
            let mut block = [[0u8; 4]; 16];
            for by in 0..4 {
                for bx in 0..4 {
                    let px = block_x + bx;
                    let py = block_y + by;
                    let idx = 4 * (py * width + px);
                    if px < width && py < height {
                        block[by * 4 + bx] = [
                            image[idx],
                            image[idx + 1],
                            image[idx + 2],
                            image[idx + 3],
                        ];
                    }
                }
            }
            
            // Compress the block and return it
            compress_bc1_block(&block, algorithm)
        })
        .collect()
}
