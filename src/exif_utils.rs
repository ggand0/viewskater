//! EXIF orientation utilities for automatic image rotation correction.
//!
//! This module provides EXIF-aware image decoding that automatically applies
//! orientation corrections based on EXIF metadata embedded in images (primarily JPEG).

use image::{DynamicImage, ImageDecoder, ImageReader};
use std::io::Cursor;

#[allow(unused_imports)]
use log::{debug, warn, error};

/// Decodes image from bytes with EXIF orientation applied.
///
/// Uses image crate v0.25+ built-in orientation support:
/// 1. Creates decoder from bytes
/// 2. Reads EXIF orientation (if present)
/// 3. Decodes to DynamicImage
/// 4. Applies orientation transformation
///
/// Falls back to simple decode if decoder creation fails (some formats
/// may not support the decoder interface).
pub fn decode_with_exif_orientation(bytes: &[u8]) -> Result<DynamicImage, std::io::ErrorKind> {
    let cursor = Cursor::new(bytes);

    let reader = ImageReader::new(cursor)
        .with_guessed_format()
        .map_err(|e| {
            error!("Failed to guess image format: {}", e);
            std::io::ErrorKind::InvalidData
        })?;

    // Try to get orientation from decoder
    match reader.into_decoder() {
        Ok(mut decoder) => {
            // Get orientation (defaults to NoTransforms if not present or unsupported format)
            let orientation = decoder.orientation()
                .unwrap_or(image::metadata::Orientation::NoTransforms);

            if orientation != image::metadata::Orientation::NoTransforms {
                debug!("EXIF orientation detected: {:?}", orientation);
            }

            // Decode the image
            let mut img = DynamicImage::from_decoder(decoder)
                .map_err(|e| {
                    error!("Failed to decode image: {}", e);
                    std::io::ErrorKind::InvalidData
                })?;

            // Apply orientation if not NoTransforms
            if orientation != image::metadata::Orientation::NoTransforms {
                debug!("Applying EXIF orientation transformation");
                img.apply_orientation(orientation);
            }

            Ok(img)
        }
        Err(e) => {
            // Fallback: some formats may not support decoder interface
            // Fall back to simple decode without orientation
            warn!("Decoder creation failed, falling back to simple decode: {}", e);
            let cursor = Cursor::new(bytes);
            ImageReader::new(cursor)
                .with_guessed_format()
                .map_err(|_| std::io::ErrorKind::InvalidData)?
                .decode()
                .map_err(|e| {
                    error!("Failed to decode image: {}", e);
                    std::io::ErrorKind::InvalidData
                })
        }
    }
}

/// Get orientation-aware dimensions from image bytes.
///
/// For 90/270 degree rotations (and their flip variants), the width and height
/// are swapped to reflect the final displayed dimensions after EXIF orientation is applied.
pub fn get_orientation_aware_dimensions(bytes: &[u8]) -> (u32, u32) {
    use image::metadata::Orientation;

    let cursor = Cursor::new(bytes);

    if let Ok(reader) = ImageReader::new(cursor).with_guessed_format() {
        if let Ok(mut decoder) = reader.into_decoder() {
            let orientation = decoder.orientation()
                .unwrap_or(Orientation::NoTransforms);

            let (w, h) = decoder.dimensions();

            // Swap dimensions for orientations that include 90/270 degree rotations
            return match orientation {
                Orientation::Rotate90
                | Orientation::Rotate270
                | Orientation::Rotate90FlipH   // EXIF 5: 90 CCW + flip = swaps dimensions
                | Orientation::Rotate270FlipH  // EXIF 7: 270 CCW + flip = swaps dimensions
                => (h, w),
                _ => (w, h),
            };
        }
    }

    // Fallback: try header-only read without orientation
    let cursor = Cursor::new(bytes);
    ImageReader::new(cursor)
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .unwrap_or((0, 0))
}
