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

1. Whiteboard-type GUI desktop app (Linux/Pop!_Os/Cosmic-latest/Wayland) written in Rust.
20. Keep code base well commented and structured for better readability and understanding.
18. Development should be in small steps with user testing for each change and git commits.
21. All temporary files/data created by app (if any) should be deleted/cleaned on app/session/block closure to prevent pollution.
2. Canvas: Unlimited-vertical scrolling, dragging and resizing, canvas width is dynamic horizontally (it can change its width with main window resizing). 2d canvas with some reasonable boundaries to prevent edge cases.
3. App has toolbar positioned on top of the canvas with icons that represent 
actions (open/load session, save sesion, load image, create text block, allign blocks and others)
3. Support for text and image blocks (gif, avif, webp with animation support). Image support: Static and animation formats.
10. Animations: Click left mouse button (LMB) to toggle image animation.
11. Image blocks spawn with original aspect ratio and maintain it during resize.
4. Collision: Blocks cannot overlap (including new spawned blocks, grouped/'chained' blocks, and resized blocks), except while being moved, dragged or resized. 
5. Align logic: rows grid alignment with wraping (simular to text words wraping) that resize blocks in each row to highest member value. 
Alignment could be called on demand (toolbar icon click), on load (session load or images bulk load) and after manual resize/reposition of block(s).
7. When two or more blocks are chained they have colored borders and can be moved or resized together.
8. 'chained' mode dissapeares after 10 seconds of inactivity (no group dragging of resising) or on click outside of group, excluding 'chain' buttons (note that there is an edge case here: clicking outside current group but on 'chain' button shouldn't cancel current group 'chaning'. Therefore 'button clicked' event should be checked first.
13. Right Mouse Button (RMB) hold and move anywhere inside a block resizes it (defaulting to the closest corner). The blocks should resize symmetrically around their center without moving across the window during the process.
14. Resizing/moving is synchronized with mouse movement in real-time. 
During group resizing blocks centeres should maintain/keep their's positions (user test edge case).
15. 'Chained'/group resize mode allows to resize several blocks to same hight 
(using hight of the block with mouse howered over) while preserving original ratio.
16. **Controls & Mappings:**
    *   **LMB + Drag:** Move blocks on canvas.
    *   **RMB + Drag:** Resize block(s).
    *   **MMB + Drag:** Pan the canvas.
    *   **Cntrl + Scroll:** Zoom in/out (zooms background and all objects, text size). Notice that this should trigger auto alignment and resizing if block gets bigger than window size (same as with single/group block(s) resizing and window resizing.
    *   **Mouse scroll:**  vertical srolling
    *   **LMB + Click (animated formats only):** Toggle animation.
    *   **Double Click (Text):** Edit text. (later)
17. **Block UI:**
    *   Top-right corner buttons (visible on hover/interaction):
    *   'x': Close/Delete block.
    *   'o': Chain/Unchain block.
19. **Chaining (Grouping):**
    *   Blocks with the 'chain' ('o') toggled on (green border) move and resize together as a group.
    *   **Auto-Unchain:** If a chained group is inactive for 10 seconds, it automatically unchains (reverts to individual blocks).
22.  Implemented uniform height for chained blocks - All chained blocks now maintain the same height during resizing
4. Proper group resizing: When any chained block is resized, 
all chained blocks adjust to match the new height while preserving their individual aspect ratios.





