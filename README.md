# MaBlocks2

A whiteboard-style desktop application for organizing and managing images on an infinite canvas. Built with Rust using the `eframe`/`egui` framework.

**Key Features:**
- Drag-and-drop image organization with automatic row-based layout
- Support for PNG, JPG, GIF, WebP, and AVIF (including animations)
- Block chaining for grouping related images
- Box containers for organizing groups
- Session save/load for persistent workspaces
- Efficient memory management with async loading and LRU caching

**Supported Platforms:** Linux (Wayland/X11), macOS

---

## Installation

### Prerequisites

#### Linux (Debian/Ubuntu/Pop!_OS)
You need several system libraries for the UI and file dialogs. Run the provided setup script:
```bash
./setup_linux.sh
```

#### macOS
Ensure you have Xcode Command Line Tools installed:
```bash
xcode-select --install
```

You also need several system libraries for the UI and AVIF support. Run the provided setup script:
```bash
./setup_macos.sh
```

### Build and Run
Use standard Cargo commands to build and run the application:

```bash
# Run in development mode
cargo run

# Build release binary
cargo build --release
```
The release binary will be located at `target/release/ma_blocks2`.

---

## Usage

### Controls

| Action | Input |
|--------|-------|
| Move blocks | LMB + Drag |
| Resize symmetrically | RMB + Drag |
| Pan canvas | MMB + Drag |
| Zoom | Ctrl + Scroll |
| Vertical scroll | Mouse Scroll |
| Toggle animation | LMB Click on image |
| Toggle chaining | 'o' button or Ctrl+Click |

### Toolbar Actions

- **Save Session** - Save current canvas state to JSON
- **Load Session** - Restore a previous session
- **Add Image** - Bulk load images
- **Reset Counters** - Reset all block counters to zero
- **Compact/Unbox** - Pack chained blocks into a Box or unpack

### Wayland Support (Linux)
The app is configured to support Wayland. If you encounter issues, you can force Wayland or X11 using environment variables:
```bash
# Force Wayland
WINIT_UNIX_BACKEND=wayland cargo run

# Force X11
WINIT_UNIX_BACKEND=x11 cargo run
```

### Performance & Resource Optimization

MaBlocks2 is designed to handle a large number of images efficiently:

- **Asynchronous Loading:** Images are decoded in background threads, keeping the UI responsive even when loading many files at once.
- **On-Demand Animation:** For animated images (GIF, WebP, AVIF), only the first frame is loaded initially. The full animation sequence is loaded only when you enable animation for that block.
- **Memory Capping:** 
    - **Downsampling:** Large images are automatically downsampled during loading to fit within reasonable dimensions, significantly reducing VRAM and RAM usage.
    - **Frame Limits:** Animation sequences are limited to a maximum of 1024 frames to prevent excessive memory consumption from long or high-fps animations.
    - **Animation Cache (LRU Purging):** To prevent GPU/RAM overload from many active animations, only the 20 most recently played animations are kept in memory. Older animations are automatically purged (reverting to their first frame) and will be reloaded on demand if played again.
- **Auto-Height Matching:** Newly added images automatically scale to match the tallest existing block, maintaining a uniform layout.

---

## Features

### Canvas

- Unlimited vertical scrolling and 2D panning
- Dynamic horizontal width that reflows content based on window size or zoom level
- Zoom support affecting all canvas elements and layout

### Blocks

- **Image Blocks:** Support for PNG, JPG, GIF, WebP, and AVIF with full transparency and animation
- **Box Blocks:** Container blocks that hold groups of other blocks (displayed at the top of the canvas)
- All blocks maintain their aspect ratio during operations

### Layout & Alignment

- Automatic row-based reflow with wrapping (similar to text flow)
- Blocks are automatically reordered after repositioning to maintain a clean grid
- Grouped blocks are moved as a single unit while preserving their internal order

### Chaining (Grouping)

- Toggle chain mode via the 'o' button or Ctrl+Click
- Chained blocks move and resize together
- Uniform height is maintained across chained blocks while preserving aspect ratios
- **Remembered Chains:** Previously chained groups are remembered - selecting any member auto-selects the entire group (session-persistent)

### Boxing & Containers

- Pack chained blocks into a Box using the toolbar button
- **Drag-to-Box:** Drop individual blocks directly into a Box
- Nested boxes are not permitted
- **Smart Toggle:** The box button re-boxes the last unboxed group or unboxes the most recent box when nothing is selected

### Block UI (Hover)

| Button | Action |
|--------|--------|
| 'x' | Delete block |
| 'o' | Toggle chaining |
| '#' | Increment (LMB) / Decrement (RMB) counter |

---

## Roadmap

- [ ] Sound options
- [ ] Text overlay on blocks
- [ ] Windows support
- [ ] Mobile support
