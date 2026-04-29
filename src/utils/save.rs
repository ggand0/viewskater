use std::sync::Arc;

use iced_wgpu::wgpu::{self, util::align_to, Texture};

use crate::app::DataViewer;

pub(crate) fn extract_gpu_image(app: &mut DataViewer, texture: &Arc<Texture>) -> Vec<u8> {
    let width = texture.width();
    let height = texture.height();

    let bytes_per_row = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = app.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tmp"),
        size: (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = app
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );

    app.queue.submit([encoder.finish()]);

    let (sender, receiver) = std::sync::mpsc::channel();
    let buffer_slice = buffer.slice(..);

    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });

    app.device.poll(wgpu::Maintain::Wait);

    receiver.recv().unwrap().unwrap();

    let padded_bytes_per_row = bytes_per_row as usize;
    let unpadded_bytes_per_row = (width * 4) as usize;

    let pixels: Vec<u8> = buffer_slice
        .get_mapped_range()
        .chunks(padded_bytes_per_row)
        .flat_map(|row| &row[..unpadded_bytes_per_row])
        .copied()
        .collect();

    buffer.unmap();

    pixels
}
