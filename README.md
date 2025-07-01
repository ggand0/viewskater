<img src="https://github.com/user-attachments/assets/4c410f1b-1103-4b84-87f1-7278aa3a46f9" alt="Alt text" width="600"/>

# ViewSkater
ViewSkater is a fast, cross-platform image viewer written in Rust & Iced.
It aims to alleviate the challenges of exploring and comparing numerous images. Linux, macOS and Windows are currently supported.

## Features
- GPU-based image rendering powered by wgpu
- Dynamic image caching on CPU or GPU memory
- Continuous image rendering via key presses and the slider UI
- Dual pane view for side-by-side image comparison
- Supports image formats supported by the image crate (JPG, PNG, GIF, BMP, TIFF, WebP, QOI, TGA, etc.)
- Renders images up to 8192×8192 px (larger images are resized to fit)

## Installation
Download the pre-built binaries from the [releases page](https://github.com/ggand0/viewskater/releases), or build it locally:

```sh
cargo run
```

To see debug logs while running, set the `RUST_LOG` environment variable:
```sh
RUST_LOG=viewskater=debug cargo run
```

To build a full release binary for packaging or distribution:
```sh
cargo build --release
```

See [`BUNDLING.md`](./BUNDLING.md) for full packaging instructions.


## Usage
Drag and drop an image or a directory of images onto a pane, and navigate through the images using the **A / D** keys or the slider UI.
Use the mouse wheel to zoom in/out of an image.

In dual-pane mode (**Ctrl + 2**), the slider syncs images in both panes by default.
You can switch to per-pane sliders by selecting the "Controls -> Controls -> Toggle Slider" menu item or pressing the **Space** bar.

## Shortcuts
| Action                             | macOS Shortcut      | Windows/Linux Shortcut |
|------------------------------------|----------------------|-------------------------|
| Show previous / next image         | Left / Right or A / D | Left / Right or A / D  |
| Continuous scroll ("skate" mode)   | Shift + Left / Right or Shift + A / D | Shift + Left / Right or Shift + A / D |
| Jump to first / last image         | Cmd + Left / Right   | Ctrl + Left / Right    |
| Toggle UI (slider + footer)        | Tab                  | Tab                    |
| Toggle single / dual slider        | Space                | Space                  |
| Select Pane 1 / 2 (Dual slider)    | 1 / 2                | 1 / 2                  |
| Open folder in Pane 1 / 2          | Alt + 1 / 2          | Alt + 1 / 2            |
| Open file in Pane 1 / 2            | Shift + Alt + 1 / 2  | Shift + Alt + 1 / 2    |
| Open file (Single pane)            | Cmd + O              | Ctrl + O               |
| Open folder (Single pane)          | Cmd + Shift + O      | Ctrl + Shift + O       |
| Toggle single / dual pane mode     | Cmd + 1 / 2          | Ctrl + 1 / 2           |
| Close all panes                    | Cmd + W              | Ctrl + W               |
| Exit                               | Cmd + Q              | Ctrl + Q               |


## Resources
- [Website](https://viewskater.com/)
- [macOS App Store](https://apps.apple.com/us/app/viewskater/id6745068907)

## Acknowledgments
ViewSkater's slider UI was inspired by the open-source project [emulsion](https://github.com/ArturKovacs/emulsion).

## License
ViewSkater is licensed under either of
- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

