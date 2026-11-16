<img src="https://github.com/user-attachments/assets/e48601c5-705a-44c4-8cc7-36f6a6fc422a" alt="Alt text" width="600"/>

# ViewSkater

A fast, cross-platform image viewer built with Rust and egui. Designed for exploring and comparing large sets of images. Linux, macOS and Windows are supported.

This is the egui port of the original [iced version](https://github.com/ggand0/viewskater-iced).

## Features

- Dynamic image caching with background preloading
- Continuous image rendering via key presses and slider
- Dual pane view for side-by-side image comparison
- Scroll-to-zoom centered on cursor, click-drag to pan
- Rotate the current image view left or right
- Fullscreen mode with cursor proximity UI reveal
- Supports jpg, jxl, png, bmp, webp, gif, tiff, qoi, tga
- Opens camera RAW files (arw, cr2, cr3, dng, nef, raf, rw2 and more) by showing the JPEG the camera stored inside them

## Download

Builds for Windows, macOS and Linux are on [GitHub Releases](https://github.com/ggand0/viewskater-egui/releases). The official build on [viewskater.com](https://viewskater.com) is the same app with a few extra features.

Or build from source:

```bash
cargo run --release --features official
```

To see debug logs:
```bash
RUST_LOG=viewskater_egui=debug cargo run --release
```

### Linux icon setup

On GNOME 46+ (Ubuntu 24.04+), the taskbar icon requires installing a `.desktop` file and icon:

```bash
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
cp assets/icon_256.png ~/.local/share/icons/hicolor/256x256/apps/viewskater-egui.png
gtk-update-icon-cache -f ~/.local/share/icons/hicolor/
cp assets/viewskater-egui.desktop ~/.local/share/applications/
```

For the AppImage, update the `Exec=` line to point to the AppImage path:
```bash
sed -i "s|Exec=.*|Exec=/path/to/viewskater-egui.AppImage %f|" \
    ~/.local/share/applications/viewskater-egui.desktop
```

## Usage

Drag and drop an image or a directory of images onto a pane, and navigate through the images using the **A / D** keys or the slider.
Use the mouse wheel to zoom in/out of an image.

In dual-pane mode (**Ctrl+2** / **Cmd+2**), the slider syncs images in both panes by default.

## Shortcuts

On macOS, use **Cmd** instead of **Ctrl**.

| Input | Action |
|---|---|
| A / D or Left / Right arrow | Previous / next image |
| Hold A / D or arrows | Skate mode (continuous scroll) |
| Home / End | First / last image |
| Ctrl+1 / Ctrl+2 | Single / dual pane |
| Ctrl+O | Open file |
| Ctrl+Shift+O | Open folder |
| Delete (also Cmd+Backspace on macOS) | Move the current image to the Trash |
| Ctrl+W | Close images |
| Ctrl+Q | Quit |
| Scroll wheel | Zoom (centered on cursor) |
| Click + drag | Pan |
| Double-click | Reset zoom and pan |
| R / Shift+R | Rotate right / left |
| I | Show or hide the metadata panel |
| F11 | Toggle fullscreen |
| Escape | Exit fullscreen |

## Caching and memory usage

ViewSkater caches decoded images in RAM for smooth rendering. Both caches are adjustable in Preferences.

| Cache | Purpose | Default | Setting |
|---|---|---|---|
| Sliding window | Preloads neighbors for instant keyboard navigation | 5 images ahead/behind | Cache Size |
| LRU decode | Stores visited images for smooth slider scrubbing | 1024 MB budget | LRU Budget (MB) |

Decoded pixels are much larger than compressed files on disk (e.g. a 10 MB PNG at 3840×2160 becomes ~32 MB as raw RGBA), so higher-resolution images use more cache per entry. Current memory usage is shown in the FPS overlay.

## Contributing

Contributions are accepted under MIT or Apache 2.0 and ship in every build. A few features are behind the `official` cargo feature. If your PR improves one of them, that change ships only in the official build, though official features may move to the GitHub Releases builds over time. The source is in this repo, so you can build it yourself. Anyone whose PR is merged gets the official build (email viewskater@ggando.me).

## License

ViewSkater is licensed under either of
- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
