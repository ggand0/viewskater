<img src="https://github.com/user-attachments/assets/bd210448-09c4-48c3-96c4-b49772d3f01b" alt="Alt text" width="600"/>

# ViewSkater
ViewSkater is a fast, cross-platform image viewer written in Rust & Iced.
It aims to alleviate the challenges of exploring and comparing numerous images. Linux, macOS and Windows are currently supported.

## Features
- Dynamic image caching on memory
- Continuous image rendering via key presses and the slider UI
- Dual pane view for side-by-side image comparison

## Installation
Download the pre-built binaries from the releases page, or build it locally:
```
cargo run --release
```

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

