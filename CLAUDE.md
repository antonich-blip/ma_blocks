# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**MaBlocks2** is a whiteboard-style desktop app for organizing images on an infinite canvas, built in Rust with the egui/eframe immediate-mode GUI framework.

## Commands

Rust is provided via the Nix flake. direnv with `use flake` (`.envrc`) auto-loads the dev environment on `cd`. If not yet allowed: `direnv allow`.

```bash
cargo run                  # Run in development mode
cargo build --release      # Build release binary → target/release/ma_blocks2
cargo check                # Fast type-check without building
cargo clippy               # Lint
nix build .#default        # Build via Nix → result/bin/ma_blocks2
```

**Environment variables for Linux:**
```bash
WINIT_UNIX_BACKEND=wayland  # Force Wayland
WINIT_UNIX_BACKEND=x11      # Force X11
```

There are no automated tests currently.

## Architecture

Six modules in `src/`:

- **`main.rs`** — `MaBlocksApp` central state, event loop, input handling, canvas pan/zoom, session save/restore (auto-saves every 5 min to `~/.local/share/ma_blocks2/session.json`)
- **`block.rs`** — `ImageBlock` struct: position, size, animation state, group membership, UI rendering (close button, chain toggle, counter badge, resize handles)
- **`block_manager.rs`** — `BlockManager`: CRUD on blocks, chaining/grouping logic, LRU animation cache (max 20 animated blocks in memory), layout/reflow
- **`image_loader.rs`** — Multi-format async decoding (GIF, WebP, AVIF, PNG, JPG). Parallel frame downsampling via rayon, SIMD YUV→RGB via `yuv` crate. Max frame dimension: 420px, max frames: 1024.
- **`constants.rs`** — All magic numbers: sizes, spacing, colors, UI dimensions
- **`paths.rs`** — Cross-platform data directory paths

**Data flow:** `MaBlocksApp` owns a `BlockManager` which owns `Vec<ImageBlock>`. Images load asynchronously; a skeleton placeholder is shown until the first frame arrives. `Session` (serde) serializes block positions/paths for persistence.

**Chaining:** Blocks can be linked ("chained") for synchronized movement and resize. Chains are persisted across sessions. Boxing packs a chain into a container `ImageBlock` (no nested boxes).

**Layout:** Horizontal reflow based on window width and zoom, row-quantized to ~100px rows.

## Key Implementation Notes

- AVIF decoding uses `libavif-sys` with dav1d backend. `LIBCLANG_PATH` may need to be set for compilation.
- The `yuv` crate provides SIMD-accelerated YUV→RGB conversion (~3-7x faster than naive); `rayon` parallelizes per-frame downsampling (~6x faster).
- `render_canvas()` in `main.rs` is a large ~230-line method — pending extraction per `refactoring.md`.
- All UI constants live in `constants.rs`; add new magic numbers there rather than inline.
