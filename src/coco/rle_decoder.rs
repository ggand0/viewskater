/// RLE (Run-Length Encoding) decoder for COCO segmentation masks
///
/// This module decodes COCO RLE format masks and converts them to polygons for rendering.
/// RLE format: {"size": [height, width], "counts": [run1, run2, ...]}
/// Counts alternate between 0s and 1s, starting with 0s.
/// IMPORTANT: COCO RLE uses COLUMN-MAJOR (Fortran) order!

use crate::coco::parser::CocoRLE;

/// Decode RLE to binary mask
/// COCO RLE uses column-major order (Fortran-style), meaning it fills column-by-column
pub fn decode_rle(rle: &CocoRLE) -> Vec<u8> {
    if rle.size.len() != 2 {
        return Vec::new();
    }

    let height = rle.size[0] as usize;
    let width = rle.size[1] as usize;
    let total_pixels = height * width;

    // Create mask in row-major order for easier access
    let mut mask = vec![0u8; total_pixels];

    let mut col = 0;
    let mut row = 0;
    let mut value = 0u8; // Start with 0 (background)

    for &count in &rle.counts {
        let count = count as usize;

        // Fill pixels with current value in column-major order
        for _ in 0..count {
            if col < width && row < height {
                // Convert column-major to row-major indexing
                let idx = row + col * height;
                if idx < total_pixels {
                    // Store in row-major format for rendering
                    mask[row * width + col] = value;
                }

                // Move to next position in column-major order
                row += 1;
                if row >= height {
                    row = 0;
                    col += 1;
                }
            } else {
                break;
            }
        }

        // Alternate between 0 and 1
        value = 1 - value;
    }

    mask
}

/// Find contours in a binary mask using a simple marching squares algorithm
/// Returns a list of polygons (each polygon is a list of (x, y) coordinates)
pub fn mask_to_polygons(mask: &[u8], width: usize, height: usize, simplify_epsilon: f32) -> Vec<Vec<(f32, f32)>> {
    if mask.is_empty() || width == 0 || height == 0 {
        return Vec::new();
    }

    let mut visited = vec![false; mask.len()];
    let mut polygons = Vec::new();

    // Find all contours
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;

            // Look for foreground pixels that haven't been visited
            if mask[idx] > 0 && !visited[idx] {
                // Trace contour starting from this pixel
                if let Some(contour) = trace_contour(mask, width, height, x, y, &mut visited) {
                    if contour.len() >= 3 {
                        // Simplify polygon using Douglas-Peucker algorithm
                        let simplified = simplify_polygon(&contour, simplify_epsilon);
                        if simplified.len() >= 3 {
                            polygons.push(simplified);
                        }
                    }
                }
            }
        }
    }

    polygons
}

/// Trace contour starting from a foreground pixel using Moore neighborhood tracing
fn trace_contour(
    mask: &[u8],
    width: usize,
    height: usize,
    start_x: usize,
    start_y: usize,
    visited: &mut [bool],
) -> Option<Vec<(f32, f32)>> {
    let mut contour = Vec::new();

    // Find the boundary by checking if any neighbor is background
    let idx = start_y * width + start_x;
    if !is_boundary_pixel(mask, width, height, start_x, start_y) {
        // Mark interior pixels as visited but don't trace them
        visited[idx] = true;
        return None;
    }

    // Moore neighborhood (8-connected): N, NE, E, SE, S, SW, W, NW
    let dx: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
    let dy: [i32; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];

    let mut x = start_x as i32;
    let mut y = start_y as i32;
    let mut dir = 0; // Start looking north

    let start_idx = idx;
    visited[start_idx] = true;
    contour.push((x as f32, y as f32));

    let max_iterations = width * height; // Prevent infinite loops
    let mut iterations = 0;

    loop {
        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        // Look for next boundary pixel in clockwise direction
        let mut found = false;
        for i in 0..8 {
            let check_dir = (dir + i) % 8;
            let nx = x + dx[check_dir];
            let ny = y + dy[check_dir];

            if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                let nidx = (ny as usize) * width + (nx as usize);

                if mask[nidx] > 0 {
                    // Found next foreground pixel
                    x = nx;
                    y = ny;
                    dir = (check_dir + 6) % 8; // Turn left for next search

                    // Check if we're back at start
                    if nidx == start_idx && contour.len() > 2 {
                        return Some(contour);
                    }

                    if !visited[nidx] || contour.len() < 3 {
                        visited[nidx] = true;

                        // Add point if it changes direction enough
                        if should_add_point(&contour, x as f32, y as f32) {
                            contour.push((x as f32, y as f32));
                        }
                    }

                    found = true;
                    break;
                }
            }
        }

        if !found {
            break; // No next pixel found, end contour
        }
    }

    if contour.len() >= 3 {
        Some(contour)
    } else {
        None
    }
}

/// Check if a pixel is on the boundary (has at least one background neighbor)
fn is_boundary_pixel(mask: &[u8], width: usize, height: usize, x: usize, y: usize) -> bool {
    // Check 4-connected neighbors
    let neighbors = [
        (x.wrapping_sub(1), y),
        (x + 1, y),
        (x, y.wrapping_sub(1)),
        (x, y + 1),
    ];

    for (nx, ny) in neighbors {
        if nx >= width || ny >= height {
            return true; // Edge of image
        }
        let nidx = ny * width + nx;
        if mask[nidx] == 0 {
            return true; // Has background neighbor
        }
    }

    false
}

/// Check if a point should be added to contour (reduces redundant collinear points)
fn should_add_point(contour: &[(f32, f32)], x: f32, y: f32) -> bool {
    if contour.len() < 2 {
        return true;
    }

    let p0 = contour[contour.len() - 2];
    let p1 = contour[contour.len() - 1];
    let p2 = (x, y);

    // Check if points are collinear
    let dx1 = p1.0 - p0.0;
    let dy1 = p1.1 - p0.1;
    let dx2 = p2.0 - p1.0;
    let dy2 = p2.1 - p1.1;

    // Cross product
    let cross = dx1 * dy2 - dy1 * dx2;

    // If cross product is near zero, points are collinear
    cross.abs() > 0.5
}

/// Simplify polygon using Douglas-Peucker algorithm
fn simplify_polygon(points: &[(f32, f32)], epsilon: f32) -> Vec<(f32, f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut result = Vec::new();
    douglas_peucker_recursive(points, epsilon, &mut result);
    result
}

fn douglas_peucker_recursive(points: &[(f32, f32)], epsilon: f32, result: &mut Vec<(f32, f32)>) {
    if points.is_empty() {
        return;
    }

    if points.len() <= 2 {
        result.extend_from_slice(points);
        return;
    }

    // Find the point with maximum distance from line segment
    let start = points[0];
    let end = points[points.len() - 1];
    let mut max_dist = 0.0f32;
    let mut max_idx = 0;

    for (i, &point) in points.iter().enumerate().skip(1).take(points.len() - 2) {
        let dist = perpendicular_distance(point, start, end);
        if dist > max_dist {
            max_dist = dist;
            max_idx = i;
        }
    }

    if max_dist > epsilon {
        // Recursively simplify
        douglas_peucker_recursive(&points[..=max_idx], epsilon, result);
        result.pop(); // Remove duplicate point
        douglas_peucker_recursive(&points[max_idx..], epsilon, result);
    } else {
        // All points are close enough, just keep endpoints
        result.push(start);
        result.push(end);
    }
}

/// Calculate perpendicular distance from point to line segment
fn perpendicular_distance(point: (f32, f32), line_start: (f32, f32), line_end: (f32, f32)) -> f32 {
    let dx = line_end.0 - line_start.0;
    let dy = line_end.1 - line_start.1;

    let norm = (dx * dx + dy * dy).sqrt();
    if norm < 1e-6 {
        // Line segment is actually a point
        let pdx = point.0 - line_start.0;
        let pdy = point.1 - line_start.1;
        return (pdx * pdx + pdy * pdy).sqrt();
    }

    // Calculate perpendicular distance using cross product
    let cross = (point.0 - line_start.0) * dy - (point.1 - line_start.1) * dx;
    cross.abs() / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rle_decode() {
        // Simple 3x3 mask with center pixel set
        let rle = CocoRLE {
            size: vec![3, 3],
            counts: vec![4, 1, 4], // 4 zeros, 1 one, 4 zeros
        };

        let mask = decode_rle(&rle);
        assert_eq!(mask.len(), 9);
        assert_eq!(mask[4], 1); // Center pixel should be set
        assert_eq!(mask[0], 0);
        assert_eq!(mask[8], 0);
    }

    #[test]
    fn test_perpendicular_distance() {
        let point = (1.0, 1.0);
        let line_start = (0.0, 0.0);
        let line_end = (2.0, 0.0);

        let dist = perpendicular_distance(point, line_start, line_end);
        assert!((dist - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_simplify_polygon() {
        // Collinear points should be simplified
        let points = vec![
            (0.0, 0.0),
            (1.0, 0.0),
            (2.0, 0.0),
            (3.0, 0.0),
        ];

        let simplified = simplify_polygon(&points, 0.1);
        assert_eq!(simplified.len(), 2); // Should only keep endpoints
    }
}
