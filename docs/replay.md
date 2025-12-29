# Replay Mode

Replay mode automates image navigation for performance benchmarking. It loads test directories, navigates through images, and records FPS metrics.

## Quick Start

```bash
# Basic benchmark (keyboard mode)
cargo run --profile opt-dev -- --replay --test-dir /path/to/images --duration 5 --auto-exit

# Slider mode benchmark
cargo run --profile opt-dev -- --replay --test-dir /path/to/images --duration 5 --nav-mode slider --auto-exit

# Multiple directories with JSON output
cargo run --profile opt-dev -- --replay \
    --test-dir /path/to/small_images \
    --test-dir /path/to/large_images \
    --duration 10 \
    --output results.json \
    --output-format json \
    --auto-exit
```

## CLI Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `--replay` | - | Enable replay mode |
| `--test-dir` | - | Directory to benchmark (can specify multiple) |
| `--duration` | `10` | Seconds to navigate each direction |
| `--directions` | `right` | Navigation direction: `right`, `left`, or `both` |
| `--iterations` | `1` | Number of times to repeat the benchmark |
| `--nav-mode` | `keyboard` | Navigation mechanism: `keyboard` or `slider` |
| `--nav-interval` | `50` | Milliseconds between navigation actions |
| `--slider-step` | `1` | Images to skip per navigation (slider mode only) |
| `--skip-initial` | `0` | Skip first N images from metrics (excludes cache warmup) |
| `--output` | - | Output file path |
| `--output-format` | `markdown` | Output format: `json` or `markdown` |
| `--auto-exit` | `false` | Exit automatically when benchmark completes |
| `--verbose` | `false` | Print detailed metrics during execution |

## Navigation Modes

### Keyboard Mode (default)

Simulates holding down navigation keys. Sets `skate_right`/`skate_left` flags for continuous frame-by-frame navigation.

- Movement: Continuous (every frame)
- Image loading: Incremental, cache-aware
- Typical Image FPS: 200+

### Slider Mode

Simulates dragging the navigation slider. Sends `SliderChanged` messages at regular intervals.

- Movement: Stepped (one position per interval)
- Image loading: Direct jump with async preview loading
- Typical Image FPS: 20-50 (depends on image size)

## Speed Control

Navigation speed is controlled by two parameters:

### Navigation Interval (`--nav-interval`)

Controls how frequently navigation actions are triggered.

| nav-interval | Actions/sec |
|--------------|-------------|
| 50ms (default) | 20 |
| 25ms | 40 |
| 20ms | 50 |
| 100ms | 10 |

### Slider Step (`--slider-step`, slider mode only)

Controls how many images to skip per navigation action.

| nav-interval | slider-step | Images/sec |
|--------------|-------------|------------|
| 50ms | 1 | 20 |
| 50ms | 5 | 100 |
| 20ms | 1 | 50 |

**Formula:** `images_per_second = (1000 / nav_interval) * slider_step`

### Matching Mouse Speed

For MX Master 3 (1000 DPI) or similar mice with typical drag speed:

```bash
# ~50 images/sec to match typical mouse drag
--nav-mode slider --nav-interval 20
```

## Output Formats

### JSON

```json
{
  "results": [
    {
      "directory": "/path/to/images",
      "direction": "right",
      "duration_secs": 5.02,
      "total_frames": 251,
      "ui_fps": { "avg": 60.1, "min": 58.0, "max": 62.0 },
      "image_fps": { "avg": 45.2, "min": 40.0, "max": 50.0, "last": 48.0 },
      "memory_mb": { "avg": 512.0, "min": 400.0, "max": 600.0 }
    }
  ],
  "iterations": 2
}
```

### Markdown

Generates a formatted table with per-directory results and summary statistics.

## Metrics Collected

| Metric | Description |
|--------|-------------|
| UI FPS | Main application frame rate |
| Image FPS | Rate of new images displayed |
| Memory | Process memory usage (Linux/macOS) |
| Duration | Actual time spent navigating |
| Total Frames | Number of UI frames rendered |

### Image FPS Sources

- **Keyboard mode**: Uses `IMAGE_RENDER_FPS` (incremental cache loading)
- **Slider mode**: Uses `iced_wgpu::get_image_fps()` (async preview loading)

## Tips

1. **Use `--skip-initial`** to exclude warmup frames where cache is cold
2. **Use `--directions right`** to avoid cache advantage on reverse navigation
3. **Run multiple `--iterations`** for more reliable averages
4. **Use `--profile opt-dev`** for optimized builds with debug symbols
5. **Set `--auto-exit`** for scripted benchmarks
