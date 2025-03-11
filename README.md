<img src="https://github.com/user-attachments/assets/4c410f1b-1103-4b84-87f1-7278aa3a46f9" alt="Alt text" width="600"/>

# ViewSkater
ViewSkater is a fast, cross-platform image viewer written in Rust & Iced.
It aims to alleviate the challenges of exploring and comparing numerous images. Linux, macOS and Windows are currently supported.

## Features
- GPU-based image rendering powered by wgpu
- Dynamic image caching on CPU or GPU memory
- Continuous image rendering via key presses and the slider UI
- Dual pane view for side-by-side image comparison
- Supports JPG and PNG images up to 8192×8192 px

## Installation
Download the pre-built binaries from the [releases page](https://github.com/ggand0/viewskater/releases), or build it locally:
```
cargo run --release
```
If you'd like to see debug logs while running, specify the `RUST_LOG` flag:
```
RUST_LOG=view_skater=debug cargo run --release
```

## Usage
Drag and drop an image or a directory of images onto a pane, and navigate through the images using the **A / D** keys or the slider UI.
Use the mouse wheel to zoom in/out of an image.

In dual-pane mode (**Ctrl + 2**), the slider syncs images in both panes by default.
You can switch to per-pane sliders by selecting the "Controls -> Controls -> Toggle Slider" menu item or pressing the **Space** bar.

## Shortcuts
- **Arrow keys (Left / Right) or A / D**: Show previous / next image
- **Shift + arrow keys (Left / Right) or Shift + A / D**: Render previous / next images continuously ("skate" mode)
- **Tab**: Show / hide the slider and footer UI
- **Space**: Toggle between single slider and dual slider
- **`1` and `2` keys**: Select Pane 1 or Pane 2
- **Ctrl + 1 or 2**: Toggle between single pane and dual pane mode
- **Ctrl + W**: Close all panes
- **Ctrl + Q**: Exit

## Resources
- [Website](https://viewskater.com/)

## Acknowledgments
ViewSkater's slider UI was inspired by the open-source project [emulsion](https://github.com/ArturKovacs/emulsion).

## License
ViewSkater is licensed under either of
- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

