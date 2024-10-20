<img src="https://github.com/user-attachments/assets/bd210448-09c4-48c3-96c4-b49772d3f01b" alt="Alt text" width="600"/>

# ViewSkater
ViewSkater is a fast, cross-platform image viewer written in Rust & Iced.
It aims to alleviate the challenges of exploring and comparing numerous images. Linux, MacOS and Windows are currently supported.

## Features
- Dynamic image caching on memory
- Continuous image rendering via key presses and the slider UI
- Dual pane view for side-by-side image comparison

## Installation
Download the pre-build binaries from the releases page, or build it locally:
```
cargo run --release
```

## Shortcuts
- Arrow keys (Left / Right) or A / D: Show previous / next image
- Shift + arrow keys (Left / Right) or Shift + A / D: Render previous / next images continuously ("skate" mode)
- Tab: Show / hide the slider and footer UI
- Space: Toggle between a single slider and dual slider
- `1` and `2` keys: Select Pane 1 or Pane 2
- Ctrl + 1 or 2: Toggle between single pane and dual pane mode
- Ctrl + W: Close all panes
- Ctrl + Q: Exit

## Resources
- [Website](https://viewskater.com/)  

## License
ViewSkater is distributed under the Apache 2.0 license.
