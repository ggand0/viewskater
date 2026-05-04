<img src="https://github.com/user-attachments/assets/4c410f1b-1103-4b84-87f1-7278aa3a46f9" alt="Alt text" width="600"/>

# ViewSkater
ViewSkater is a fast, cross-platform image viewer written in Rust & Iced.
It aims to alleviate the challenges of exploring and comparing numerous images. Linux, macOS and Windows are currently supported.

> **Note:** This (iced) version is in maintenance mode. Active development is moving to the [egui version](https://github.com/ggand0/viewskater-egui), which offers better performance and a simpler codebase.

## Features
- GPU-based image rendering powered by wgpu
- Dynamic image caching on CPU or GPU memory
- Continuous image rendering via key presses and the slider UI
- Dual pane view for side-by-side image comparison
- Supports image formats supported by the image crate (JPG, PNG, GIF, BMP, TIFF, WebP, QOI, TGA, etc.)
- **JPEG 2000 support** (optional feature): View JP2, J2K, and J2C files
- Supports viewing images inside ZIP, RAR, and 7z (LZMA2 codec) files
- Renders images up to 8192×8192 px (larger images are resized to fit)
- **COCO annotation support** (optional feature): Display bounding boxes and segmentation masks with dual rendering modes (polygon/pixel)
- **Selection feature** (optional feature): Select and export subsets of images from large datasets

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

**Building with optional features:**
```sh
# Build with COCO annotation support
cargo build --release --features coco

# Build with selection feature
cargo build --release --features selection

# Build with JPEG 2000 support
cargo build --release --features jp2

# Build with multiple features
cargo build --release --features coco,selection,jp2
```

See [docs/bundling.md](./docs/bundling.md) for full packaging instructions.

### Linux icon setup

On GNOME 46+ (Ubuntu 24.04+), the taskbar icon requires installing a `.desktop` file and icon:

```bash
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
cp assets/icon_256.png ~/.local/share/icons/hicolor/256x256/apps/viewskater.png
gtk-update-icon-cache -f ~/.local/share/icons/hicolor/
cp resources/linux/viewskater.desktop ~/.local/share/applications/
```

Edit the `Exec=` line in the installed `.desktop` file to point to your binary:
```bash
sed -i "s|Exec=.*|Exec=/path/to/viewskater %f|" \
    ~/.local/share/applications/viewskater.desktop
```

For the AppImage, use the AppImage path instead:
```bash
sed -i "s|Exec=.*|Exec=/path/to/ViewSkater.AppImage %f|" \
    ~/.local/share/applications/viewskater.desktop
```

## Usage
Drag and drop an image or a directory of images onto a pane, and navigate through the images using the **A / D** keys or the slider UI.
Use the mouse wheel to zoom in/out of an image.

In dual-pane mode (**Ctrl + 2**), the slider syncs images in both panes by default.
You can switch to per-pane sliders by selecting the "Controls -> Controls -> Toggle Slider" menu item or pressing the **Space** bar.

**COCO Annotations** (when built with `--features coco`):
Drag and drop a COCO-format JSON annotation file onto the app. The app will automatically search for the image directory in common locations:
- Same directory as the JSON file
- `images/`, `img/`, `val2017/`, or `train2017/` subdirectories
- Single subdirectory if only one exists in the JSON's parent directory

If the image directory is not found automatically, a folder picker will prompt you to select the image directory manually.

**Image Selection** (when built with `--features selection`):
Mark images for dataset curation while browsing. Press **S** to mark an image as selected (green badge), **X** to exclude it (red badge), or **U** to clear the mark. Export your selections to JSON using **Cmd+E** (macOS) or **Ctrl+E** (Windows/Linux). Selection states are automatically saved and persist across sessions.

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
| Toggle fullscreen mode             | F11                  | F11                    |
| Close all panes                    | Cmd + W              | Ctrl + W               |
| Exit                               | Cmd + Q              | Ctrl + Q               |


## Documentation

- [Bundling & Packaging](./docs/bundling.md) - Build for Linux, macOS, Windows
- [Replay Mode](./docs/replay.md) - Automated benchmarking with CLI options

## Resources
- [Website](https://viewskater.com/)
- [egui version](https://github.com/ggand0/viewskater-egui)

## Acknowledgments
ViewSkater's slider UI was inspired by the open-source project [emulsion](https://github.com/ArturKovacs/emulsion).

## License
ViewSkater is licensed under either of
- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

