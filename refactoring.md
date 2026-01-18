Codebase Overview
Project: MaBlocks2 - A Rust/egui desktop application for organizing images on an infinite canvas (whiteboard-style image board)
Structure: 4 source files (~2,460 lines total)
- main.rs (1,381 lines) - Application state and core logic
- block.rs (621 lines) - Block rendering and manipulation
- image_loader.rs (428 lines) - Multi-format image decoding
- paths.rs (30 lines) - Path management
---
Redundancy Analysis
High Priority Duplications
| Issue | Description | Locations |
|-------|-------------|-----------|
| Block lookup by ID | blocks.iter().position(\|b\| b.id == id) pattern | 5+ occurrences in main.rs, block.rs |
| Group texture creation | Identical texture creation code | box_group(), data_to_block() |
| World position calculation | (m_pos - canvas_origin) / zoom formula | 3 locations in main.rs |
Medium Priority Duplications
| Issue | Description |
|-------|-------------|
| Y-quantization sorting logic | Identical comparison logic for block positioning appears twice |
| Color restoration from array | Same Color32::from_rgba_unmultiplied pattern duplicated |
| Block property restoration | Position/size assignment duplicated for groups and images |
| Chained blocks filtering | blocks.iter().filter(\|b\| b.chained) appears 3 times |
---
Refactoring Opportunities
High Priority
1. MaBlocksApp is a God class (main.rs:102-1381)
   - Handles UI, persistence, input, animations, chaining, zoom all in one struct
   - Extract: SessionManager, CanvasRenderer, BlockManager, AnimationController
2. render_canvas method is 232 lines (main.rs:795-1027)
   - Extract: process_input_snapshot, detect_drop_target, render_blocks
3. ImageBlock::render is 172 lines (block.rs:363-535)
   - Extract: render_group_background, render_control_buttons, render_counter_badge
4. Magic numbers scattered throughout (block.rs)
   - 16.0, 4.0, 1.2, 6.0, 0.9, etc. used without constants
   - Colors like Color32::from_rgb(100, 100, 150) need semantic names
5. No block lookup abstraction
   - Create block_index(&self, id: Uuid) -> Option<usize> helper
   - Consider HashMap<Uuid, usize> for O(1) lookup
Medium Priority
| Issue | File | Recommendation |
|-------|------|----------------|
| Complex conditionals in toggle_compact_group | main.rs:550-598 | Extract helper methods, use early returns |
| Toolbar button creation repeated 5x | main.rs:733-791 | Create toolbar_button() helper |
| Inconsistent error handling | main.rs | Mix of unwrap(), logging, silent failures |
| data_to_block duplication | main.rs:1247-1310 | Extract deserialize_group_block, deserialize_image_block |
| Tightly coupled ImageBlock to egui | block.rs | Separate domain model from view model |
Low Priority
- Missing documentation on public items
- No unit tests in the project
- session_file field is set but never read
- Some type annotations could be more explicit
---
Recommended Action Plan
1. Add BlockManager abstraction - Consolidate block CRUD and chaining operations
5. Extract session management - Separate persistence from UI logic
6. Add tests - Required before major refactoring
