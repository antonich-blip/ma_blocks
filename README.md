## Compilation and Running

This app supports Linux (Wayland/X11) and macOS.

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
The runnable binary will be located at `target/release/ma_blocks2`.

### Wayland Support (Linux)
The app is configured to support Wayland. If you encounter issues, you can force Wayland or X11 using environment variables:
```bash
# Force Wayland
WINIT_UNIX_BACKEND=wayland cargo run

# Force X11
WINIT_UNIX_BACKEND=x11 cargo run
```

## User Requirements

1. **Platform:** Whiteboard-type GUI desktop app for Linux (Wayland/X11) and macOS, written in Rust using `eframe`/`egui`.
2. **Canvas:** 
    *   Unlimited vertical scrolling and 2D panning (MMB + Drag).
    *   Dynamic horizontal width that reflows content based on window size or zoom level.
    *   Zoom support (Ctrl + Scroll) affecting all canvas elements and layout.
3. **Toolbar:** Positioned at the top with actions for:
    *   💾 Save Session: Saves current blocks and their states to a JSON file.
    *   📂 Load Session: Restores a previous session from a JSON file.
    *   🖼 Add Image: Bulk load images (PNG, JPG, GIF, WebP, AVIF).
    *   🔄 Reset Counters: Resets all block counters to zero.
    *   📦 Compact/Unbox: Stateful toggle that packs chained blocks into a single "Box" block or unpacks a selected box. Remembers the last group unboxed for easy re-boxing if no new selection is made.
4. **Block Support:** 
    *   Currently supports Image blocks with full transparency and animation support (GIF, animated WebP, animated AVIF).
    *   **Box Blocks:** Specialized blocks that contain other blocks.
    *   Images spawn with original aspect ratio and maintain it during all operations.
5. **Alignment & Layout:** 
    *   Automatic row-based reflow logic with wrapping (similar to text).
    *   Blocks are automatically reordered and reflowed after manual repositioning (drag stop) to maintain a clean grid.
    After block (or a group of chained blocks) is dropped, it is treated as a single unit. The entire group is extracted from the block list, preserving its internal relative order, and then inserted into the new position as a continuous sequence
6. **Resizing:**
    *   Symmetrical resizing around the block's center using RMB + Drag.
    *   Real-time synchronization with mouse movement.
    *   Minimum size constraints to prevent UI artifacts.
7. **Chaining (Grouping):**
    *   Toggle 'chain' mode for individual blocks via the 'o' button or Ctrl+Click.
    *   Chained blocks move together when any member of the group is dragged.
    *   **Uniform Height:** Chained blocks maintain a synchronized height during resizing while preserving their individual aspect ratios.
    *   Chaining is cancelled by clicking on the empty canvas.
    *   **Remembered Chains:** When a chain of 2+ blocks is cleared, it is automatically remembered. Selecting any member of a previously chained group will auto-select all other members of that group. This feature is session-persistent.
8. **Boxing & Containers:**
    *   Chained blocks can be packed into a "Box" block using the 📦 toolbar button.
    *   **Drag-to-Box:** Single blocks can be dragged and dropped directly into a "Box" block to add them to the container.
    *   Boxing an existing "Box" block is not permitted (no nested containers).
    *   Unboxing restores all contained blocks to the main canvas.
    *   **Smart Toggle:** The 📦 toolbar button acts as a toggle. If no blocks are selected, clicking it will either re-box the most recently unboxed group or unbox the most recently created group, allowing for quick "previewing" of unboxed content.
9. **Counter Feature:**
    *   Each normal block has an optional counter (visible when > 0).
    *   Interact via the '#' button: LMB click to increment, RMB click to decrement (normal blocks only).
10. **Controls & Mappings:**
    *   **LMB + Drag:** Move blocks (triggers reflow on release or drop into box).
    *   **RMB + Drag:** Resize block(s) symmetrically.
    *   **MMB + Drag:** Pan the canvas.
    *   **Ctrl + Scroll:** Zoom in/out.
    *   **Mouse Scroll:** Vertical scrolling.
    *   **LMB Click (Image):** Toggle animation for supported formats.
11. **Block UI (Hover/Interaction):**
    *   'x': Delete/Close block.
    *   'o': Toggle chaining.
    *   '#': Increment/Decrement counter (normal blocks only).

## ToDos
- [x]  proper realignment with keeping order on  drag + drop of a group of blocks
- [x]  make 'reset' button to reset counter bubbles inside boxes too
- [x]  remember  group/'chain'. selecting one of the remembered member triggers auto selecting other members (this feature is session persistent)
- [ ]  drop group of blocks into a box with proper visual effects
- [ ]  default images/sessions folder
- [ ]  sound options
- [ ]  text over blocks options
- [ ]  Windows support
- [ ]  Mobile support


