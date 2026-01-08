mod block;
mod image_loader;

use block::{ImageBlock, BLOCK_PADDING};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, UiBuilder, Vec2};
use egui::{pos2, vec2};
use image_loader::load_image_frames;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

const CANVAS_PADDING: f32 = 32.0;
const CANVAS_WORKING_WIDTH: f32 = 1400.0;
const ALIGN_SPACING: f32 = 24.0;
const MAX_BLOCK_DIMENSION: f32 = 420.0;
const MIN_BLOCK_SIZE: f32 = 50.0;
const MIN_CANVAS_INNER_WIDTH: f32 = MIN_BLOCK_SIZE + BLOCK_PADDING * 2.0;

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "MaBlocks2",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(MaBlocksApp::new(cc)))
        }),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum ResizeHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone)]
struct InteractionState {
    id: Uuid,
    handle: ResizeHandle,
    initial_mouse_pos: Pos2,
    initial_block_rect: Rect,
}

#[derive(Serialize, Deserialize)]
struct Session {
    blocks: Vec<BlockData>,
    #[serde(default)]
    remembered_chains: Vec<Vec<String>>, // Vec of chain groups, each containing block UUIDs as strings
}

#[derive(Serialize, Deserialize)]
struct BlockData {
    id: Uuid,
    position: [f32; 2],
    size: [f32; 2],
    path: String,
    chained: bool,
    animation_enabled: bool,
    counter: i32,
    #[serde(default)]
    is_group: bool,
    #[serde(default)]
    group_name: String,
    #[serde(default)]
    color: [u8; 4],
    #[serde(default)]
    children: Vec<BlockData>,
}

struct MaBlocksApp {
    blocks: Vec<ImageBlock>,
    next_block_id: usize,
    resizing_state: Option<InteractionState>,
    skip_chain_cancel: bool,
    working_inner_width: f32,
    session_file: Option<PathBuf>,
    zoom: f32,
    last_unboxed_ids: Vec<Uuid>,
    last_boxed_id: Option<Uuid>,
    show_file_names: bool,
    hovered_box_id: Option<Uuid>,
    /// Remembered chain groups - selecting one member auto-selects others
    remembered_chains: Vec<HashSet<Uuid>>,
}

#[derive(Default, Clone, Copy)]
struct BlockControlHover {
    close_hovered: bool,
    chain_hovered: bool,
    counter_hovered: bool,
}

fn block_control_rects(rect: Rect, _block: &ImageBlock, zoom: f32) -> (Rect, Rect, Rect) {
    let btn_size = 16.0 * zoom;
    let btn_spacing = 4.0 * zoom;
    let btn_hit_size = btn_size * 1.2;

    let close_rect = Rect::from_center_size(
        rect.right_top()
            + Vec2::new(
                -btn_hit_size / 2.0 - 4.0 * zoom,
                btn_hit_size / 2.0 + 4.0 * zoom,
            ),
        Vec2::splat(btn_hit_size),
    );

    let chain_rect = Rect::from_center_size(
        close_rect.center() - Vec2::new(btn_hit_size + btn_spacing, 0.0),
        Vec2::splat(btn_hit_size),
    );

    let counter_rect = Rect::from_center_size(
        chain_rect.center() - Vec2::new(btn_hit_size + btn_spacing, 0.0),
        Vec2::splat(btn_hit_size),
    );

    (close_rect, chain_rect, counter_rect)
}

impl MaBlocksApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            blocks: Vec::new(),
            next_block_id: 0,
            resizing_state: None,
            skip_chain_cancel: false,
            working_inner_width: CANVAS_WORKING_WIDTH,
            session_file: None,
            zoom: 1.0,
            last_unboxed_ids: Vec::new(),
            last_boxed_id: None,
            show_file_names: false,
            remembered_chains: Vec::new(),
            hovered_box_id: None,
        }
    }

    fn load_images(&mut self, ctx: &egui::Context) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "avif"])
            .pick_files()
        {
            for path in paths {
                if let Err(err) = self.insert_block_from_path(ctx, path.clone()) {
                    log::error!("{}", err);
                }
            }
            self.reflow_blocks();
        }
    }

    fn insert_block_from_path(&mut self, ctx: &egui::Context, path: PathBuf) -> Result<(), String> {
        let loaded = load_image_frames(&path)?;
        if loaded.frames.is_empty() {
            return Err(format!(
                "{} did not contain renderable frames",
                path.display()
            ));
        }

        let texture_label = format!("block-texture-{}", self.next_block_id);
        let texture = ctx.load_texture(
            texture_label,
            loaded.frames[0].image.clone(),
            egui::TextureOptions::LINEAR,
        );

        let image_size = scaled_size(loaded.original_size);
        let mut block = ImageBlock::new(
            path.to_string_lossy().into_owned(),
            texture,
            loaded.frames,
            image_size,
        );
        self.next_block_id += 1;
        block.position = pos2(CANVAS_PADDING, CANVAS_PADDING);
        self.blocks.push(block);
        Ok(())
    }

    fn advance_animations(&mut self, dt: f32, ctx: &egui::Context) {
        let mut changed = false;
        let mut next_frame_in: Option<Duration> = None;
        for block in &mut self.blocks {
            if block.update_animation(dt) {
                changed = true;
            }
            if let Some(remaining) = block.time_until_next_frame() {
                next_frame_in = Some(match next_frame_in {
                    Some(current) => current.min(remaining),
                    None => remaining,
                });
            }
        }
        if changed {
            ctx.request_repaint();
        }
        if let Some(wait) = next_frame_in {
            ctx.request_repaint_after(wait);
        }
    }

    fn reflow_blocks(&mut self) {
        let inner_width = self.working_inner_width.max(MIN_CANVAS_INNER_WIDTH);
        let row_limit = CANVAS_PADDING + inner_width;
        let max_image_width = (inner_width - BLOCK_PADDING * 2.0).max(1.0);

        for block in &mut self.blocks {
            block.reset_to_preferred_size();
            block.constrain_to_width(max_image_width);
        }

        let mut cursor = vec2(CANVAS_PADDING, CANVAS_PADDING);
        let mut row_height = 0.0;

        for block in &mut self.blocks {
            let size = block.outer_size();
            if cursor.x + size.x > row_limit {
                cursor.x = CANVAS_PADDING;
                cursor.y += row_height + ALIGN_SPACING;
                row_height = 0.0;
            }

            block.position = pos2(cursor.x, cursor.y);
            cursor.x += size.x + ALIGN_SPACING;
            row_height = row_height.max(size.y);
        }
    }

    fn can_chain(&self) -> bool {
        !self.blocks.is_empty()
    }

    fn enforce_chain_constraints(&mut self) {
        if self.blocks.is_empty() {
            self.clear_chain_group();
        }
    }

    fn clear_chain_group(&mut self) {
        // Collect IDs of currently chained blocks to remember
        let chained_ids: HashSet<Uuid> = self
            .blocks
            .iter()
            .filter(|b| b.chained)
            .map(|b| b.id)
            .collect();

        // Only save if there are at least 2 chained blocks (a real group)
        if chained_ids.len() >= 2 {
            // Remove any existing remembered chains that overlap with this one
            self.remembered_chains
                .retain(|chain| chain.is_disjoint(&chained_ids));
            // Add the new remembered chain
            self.remembered_chains.push(chained_ids);
        }

        // Clear the chain
        for block in &mut self.blocks {
            block.chained = false;
        }
        self.skip_chain_cancel = false;
    }

    fn toggle_chain_for_block(&mut self, index: usize) {
        if !self.can_chain() && !self.blocks[index].is_group {
            return;
        }

        let block_id = self.blocks[index].id;
        let was_chained = self.blocks[index].chained;

        // If turning ON chain, check if this block belongs to a remembered chain
        if !was_chained {
            // Find a remembered chain containing this block
            let remembered_chain = self
                .remembered_chains
                .iter()
                .find(|chain| chain.contains(&block_id))
                .cloned();

            if let Some(chain_ids) = remembered_chain {
                // Auto-chain all members of the remembered group
                for block in &mut self.blocks {
                    if chain_ids.contains(&block.id) {
                        block.chained = true;
                    }
                }
            } else {
                // No remembered chain, just toggle this block
                self.blocks[index].chained = true;
            }
        } else {
            // Turning OFF - just toggle this block
            self.blocks[index].chained = false;
        }

        self.skip_chain_cancel = true;
    }

    fn box_group(&mut self, ctx: &egui::Context) -> Uuid {
        let mut chained_indices: Vec<usize> = self
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.chained)
            .map(|(i, _)| i)
            .collect();

        if chained_indices.is_empty() {
            return Uuid::nil();
        }

        // Sort indices in descending order to remove from blocks safely
        chained_indices.sort_by(|a, b| b.cmp(a));

        let mut children = Vec::new();
        let mut min_pos = pos2(f32::MAX, f32::MAX);
        for &idx in &chained_indices {
            let block = self.blocks.remove(idx);
            min_pos.x = min_pos.x.min(block.position.x);
            min_pos.y = min_pos.y.min(block.position.y);
            children.push(block);
        }
        children.reverse(); // Restore original order

        let group_name = if children.len() > 1 {
            format!("Group of {}", children.len())
        } else if children.len() == 1 {
            format!(
                "Box: {}",
                Path::new(&children[0].path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            )
        } else {
            "Empty Group".to_string()
        };

        // Create a placeholder texture for the group (folder icon)
        // For now, we use a simple color image
        let texture = ctx.load_texture(
            format!("group-texture-{}", self.next_block_id),
            egui::ColorImage::new([1, 1], Color32::from_rgb(200, 180, 100)),
            egui::TextureOptions::LINEAR,
        );
        self.next_block_id += 1;

        let representative_texture = children.first().map(|c| c.texture.clone());

        let mut group_block =
            ImageBlock::new_group(group_name, children, texture, representative_texture);
        group_block.position = min_pos;
        let new_id = group_block.id;
        self.blocks.insert(0, group_block);
        self.reflow_blocks();
        new_id
    }

    fn unbox_group(&mut self, index: usize) {
        let group = self.blocks.remove(index);
        if group.is_group {
            // Find the index after all currently existing groups
            let insert_idx = self
                .blocks
                .iter()
                .position(|b| !b.is_group)
                .unwrap_or(self.blocks.len());
            for (i, mut child) in group.children.into_iter().enumerate() {
                child.chained = false;
                self.blocks.insert(insert_idx + i, child);
            }
        }
        self.reflow_blocks();
    }

    fn drop_block_into_box(&mut self, block_idx: usize, box_idx: usize) {
        let is_chained = self.blocks[block_idx].chained;
        let box_id = self.blocks[box_idx].id;

        if is_chained {
            let chained_ids: Vec<Uuid> = self
                .blocks
                .iter()
                .filter(|b| b.chained)
                .map(|b| b.id)
                .collect();
            for id in chained_ids {
                if let Some(b_idx) = self.blocks.iter().position(|b| b.id == id) {
                    if let Some(t_idx) = self.blocks.iter().position(|b| b.id == box_id) {
                        self.move_single_block_into_box(b_idx, t_idx);
                    }
                }
            }
        } else {
            self.move_single_block_into_box(block_idx, box_idx);
        }
    }

    fn move_single_block_into_box(&mut self, block_idx: usize, box_idx: usize) {
        let mut block = self.blocks.remove(block_idx);
        block.is_dragging = false;
        block.chained = false;

        let target_box_idx = if box_idx > block_idx {
            box_idx - 1
        } else {
            box_idx
        };
        let box_block = &mut self.blocks[target_box_idx];

        if box_block.representative_texture.is_none() {
            box_block.representative_texture = Some(block.texture.clone());
        }

        box_block.children.push(block);

        // Update group name
        if box_block.children.len() > 1 {
            box_block.group_name = format!("Group of {}", box_block.children.len());
        } else if box_block.children.len() == 1 {
            box_block.group_name = format!(
                "Box: {}",
                Path::new(&box_block.children[0].path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            );
        }
    }

    fn toggle_compact_group(&mut self, ctx: &egui::Context) {
        let chained_count = self.blocks.iter().filter(|b| b.chained).count();

        // If nothing is chained, try to toggle the last state
        if chained_count == 0 {
            if !self.last_unboxed_ids.is_empty() {
                let mut found_any = false;
                for block in &mut self.blocks {
                    if self.last_unboxed_ids.contains(&block.id) {
                        block.chained = true;
                        found_any = true;
                    }
                }
                if found_any {
                    self.last_boxed_id = Some(self.box_group(ctx));
                    self.last_unboxed_ids.clear();
                    return;
                }
            } else if let Some(last_id) = self.last_boxed_id {
                if let Some(idx) = self.blocks.iter().position(|b| b.id == last_id) {
                    self.last_unboxed_ids =
                        self.blocks[idx].children.iter().map(|c| c.id).collect();
                    self.unbox_group(idx);
                    self.last_boxed_id = None;
                    return;
                }
            }
        }

        let chained: Vec<&ImageBlock> = self.blocks.iter().filter(|b| b.chained).collect();

        if chained.len() == 1 && chained[0].is_group {
            let idx = self.blocks.iter().position(|b| b.chained).unwrap();
            // Store children IDs before unboxing
            self.last_unboxed_ids = self.blocks[idx].children.iter().map(|c| c.id).collect();
            self.unbox_group(idx);
            self.last_boxed_id = None;
        } else if !chained.is_empty() {
            self.last_boxed_id = Some(self.box_group(ctx));
            self.last_unboxed_ids.clear();
        }
    }

    fn reorder_and_reflow(&mut self, leader_id: Option<Uuid>) {
        if let Some(leader_id) = leader_id {
            // Identify moved group
            let is_leader_chained = self
                .blocks
                .iter()
                .find(|b| b.id == leader_id)
                .map(|b| b.chained)
                .unwrap_or(false);

            let mut moved_group = Vec::new();
            let mut remaining = Vec::new();

            // We must preserve the relative order in self.blocks
            let leader_idx_in_group = self
                .blocks
                .iter()
                .enumerate()
                .find(|(_, b)| b.id == leader_id)
                .map(|(i, _)| i);

            if leader_idx_in_group.is_none() {
                return;
            }

            for block in self.blocks.drain(..) {
                let is_moved = if is_leader_chained {
                    block.chained
                } else {
                    block.id == leader_id
                };

                if is_moved {
                    moved_group.push(block);
                } else {
                    remaining.push(block);
                }
            }

            if moved_group.is_empty() {
                self.blocks = remaining;
                self.reflow_blocks();
                return;
            }

            // Find where the leader ended up
            let leader_pos = moved_group
                .iter()
                .find(|b| b.id == leader_id)
                .unwrap()
                .position;

            // Sort remaining by current position to find insertion point correctly
            remaining.sort_by(|a, b| {
                let a_y_q = (a.position.y / 100.0) as i32;
                let b_y_q = (b.position.y / 100.0) as i32;
                match a_y_q.cmp(&b_y_q) {
                    Ordering::Equal => a
                        .position
                        .x
                        .partial_cmp(&b.position.x)
                        .unwrap_or(Ordering::Equal),
                    ord => ord,
                }
            });

            let mut insert_idx = remaining.len();
            for (i, b) in remaining.iter().enumerate() {
                let b_y_q = (b.position.y / 100.0) as i32;
                let leader_y_q = (leader_pos.y / 100.0) as i32;

                if leader_y_q < b_y_q || (leader_y_q == b_y_q && leader_pos.x < b.position.x) {
                    insert_idx = i;
                    break;
                }
            }

            // Re-assemble
            self.blocks = remaining;
            for (i, block) in moved_group.into_iter().enumerate() {
                self.blocks.insert(insert_idx + i, block);
            }
        } else {
            // Old sorting logic as fallback
            self.blocks.sort_by(|a, b| {
                let a_y_q = (a.position.y / 100.0) as i32;
                let b_y_q = (b.position.y / 100.0) as i32;
                match a_y_q.cmp(&b_y_q) {
                    Ordering::Equal => a
                        .position
                        .x
                        .partial_cmp(&b.position.x)
                        .unwrap_or(Ordering::Equal),
                    ord => ord,
                }
            });
        }
        self.reflow_blocks();
    }

    fn render_block(
        &self,
        ui: &mut egui::Ui,
        block: &ImageBlock,
        rect: Rect,
        _hovered: bool,
        show_controls: bool,
        hover_state: BlockControlHover,
        is_drop_target: bool,
    ) {
        let painter = ui.painter_at(rect);
        let zoom = self.zoom;

        // Draw only the image, no background or border for minimalistic look
        let image_rect = Rect::from_min_size(
            pos2(
                rect.min.x + BLOCK_PADDING * zoom,
                rect.min.y + BLOCK_PADDING * zoom,
            ),
            block.image_size * zoom,
        );

        // Slightly rounded corners for the image
        let rounding = egui::Rounding::same(6.0 * zoom);

        if block.is_group {
            let fill_color = if block.chained || is_drop_target {
                Color32::from_rgb(100, 100, 150)
            } else {
                Color32::from_rgb(60, 60, 60)
            };
            painter.rect_filled(image_rect, rounding, fill_color);

            // Folder-like icon (simplified)
            let folder_rect = Rect::from_center_size(image_rect.center(), image_rect.size() * 0.9);
            painter.rect_filled(folder_rect, egui::Rounding::same(2.0 * zoom), block.color);
            painter.rect_filled(
                Rect::from_min_max(
                    folder_rect.left_top() - vec2(0.0, 5.0 * zoom),
                    folder_rect.left_top() + vec2(folder_rect.width() * 0.4, 0.0),
                ),
                egui::Rounding::same(1.0 * zoom),
                block.color,
            );

            // Representative image tag - centered
            if let Some(rep_texture) = &block.representative_texture {
                let tag_size = image_rect.size() * 0.8;
                let tag_rect = Rect::from_center_size(image_rect.center(), tag_size);
                let mut tag_shape = egui::epaint::RectShape::filled(
                    tag_rect,
                    egui::Rounding::same(2.0 * zoom),
                    Color32::WHITE,
                );
                tag_shape.fill_texture_id = rep_texture.id();
                tag_shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.add(tag_shape);
            }
        } else {
            let mut rect_shape =
                egui::epaint::RectShape::filled(image_rect, rounding, Color32::WHITE);
            rect_shape.fill_texture_id = block.texture.id();
            rect_shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.add(rect_shape);
        }

        if show_controls {
            let (close_rect, chain_rect, counter_rect) = block_control_rects(rect, block, zoom);
            // block_control_rects already uses the passed rect which is scaled

            let btn_size = 16.0 * zoom; // Use the same size for drawing
            let chain_enabled = self.can_chain();

            painter.circle_filled(
                close_rect.center(),
                btn_size / 2.0,
                if hover_state.close_hovered {
                    Color32::from_rgb(255, 100, 100)
                } else {
                    Color32::RED
                },
            );
            painter.text(
                close_rect.center(),
                Align2::CENTER_CENTER,
                "x",
                FontId::monospace(12.0 * zoom),
                Color32::WHITE,
            );

            let chain_color = if block.chained {
                Color32::GREEN
            } else if !chain_enabled {
                Color32::from_gray(80)
            } else if hover_state.chain_hovered {
                Color32::LIGHT_GRAY
            } else {
                Color32::GRAY
            };
            painter.circle_filled(chain_rect.center(), btn_size / 2.0, chain_color);
            painter.text(
                chain_rect.center(),
                Align2::CENTER_CENTER,
                "o",
                FontId::monospace(12.0 * zoom),
                Color32::WHITE,
            );

            if !block.is_group {
                painter.circle_filled(
                    counter_rect.center(),
                    btn_size / 2.0,
                    if hover_state.counter_hovered {
                        Color32::from_rgb(0, 150, 0)
                    } else {
                        Color32::from_rgb(0, 100, 0)
                    },
                );
                painter.text(
                    counter_rect.center(),
                    Align2::CENTER_CENTER,
                    "#",
                    FontId::monospace(12.0 * zoom),
                    Color32::WHITE,
                );
            }
        }

        // Draw counter if > 0
        if !block.is_group && block.counter > 0 {
            let circle_radius = 15.0 * zoom;
            let circle_center = pos2(
                rect.min.x + circle_radius + 5.0 * zoom,
                rect.min.y + circle_radius + 5.0 * zoom,
            );
            painter.circle_filled(
                circle_center,
                circle_radius,
                Color32::from_rgba_unmultiplied(0, 100, 0, 170),
            );
            painter.text(
                circle_center,
                Align2::CENTER_CENTER,
                block.counter.to_string(),
                FontId::proportional(20.0 * zoom),
                Color32::WHITE,
            );
        }

        if self.show_file_names {
            let name = if block.is_group {
                &block.group_name
            } else {
                Path::new(&block.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            };

            let font_id = FontId::proportional(12.0 * zoom);
            let galley = ui
                .painter()
                .layout_no_wrap(name.to_string(), font_id, Color32::WHITE);

            // Position at top-left of the image with a small margin
            let text_pos = image_rect.left_top() + vec2(4.0 * zoom, 4.0 * zoom);
            let text_rect = Rect::from_min_size(text_pos, galley.size());

            painter.rect_filled(
                text_rect.expand(2.0 * zoom),
                egui::Rounding::same(2.0 * zoom),
                Color32::from_black_alpha(180),
            );
            painter.galley(text_pos, galley, Color32::WHITE);
        }
    }

    fn handle_resizing(&mut self, curr_mouse_pos: Pos2) {
        if let Some(state) = &self.resizing_state {
            if let Some(idx) = self.blocks.iter().position(|b| b.id == state.id) {
                let delta_world = (curr_mouse_pos - state.initial_mouse_pos) / self.zoom;
                let original_center = state.initial_block_rect.center();
                let min_size = MIN_BLOCK_SIZE;

                let initial_image_width = state.initial_block_rect.width() - BLOCK_PADDING * 2.0;
                let initial_image_height = state.initial_block_rect.height() - BLOCK_PADDING * 2.0;
                let half_width = initial_image_width * 0.5;
                let half_height = initial_image_height * 0.5;

                let x_sign = match state.handle {
                    ResizeHandle::TopLeft | ResizeHandle::BottomLeft => -1.0,
                    _ => 1.0,
                };
                let y_sign = match state.handle {
                    ResizeHandle::TopLeft | ResizeHandle::TopRight => -1.0,
                    _ => 1.0,
                };

                let target_offset_x = half_width * x_sign + delta_world.x;
                let width_from_x = (2.0 * target_offset_x.abs()).max(min_size);

                let target_offset_y = half_height * y_sign + delta_world.y;
                let height_from_y = 2.0 * target_offset_y.abs();
                let width_from_y = (height_from_y * self.blocks[idx].aspect_ratio).max(min_size);

                let mut new_width = if delta_world.x.abs() >= delta_world.y.abs() {
                    width_from_x
                } else {
                    width_from_y
                };

                if !new_width.is_finite() || new_width.is_nan() {
                    new_width = min_size;
                }
                new_width = new_width.max(min_size);

                let new_height = new_width / self.blocks[idx].aspect_ratio;
                let new_size = vec2(new_width, new_height);
                let new_outer_size = new_size + Vec2::splat(BLOCK_PADDING * 2.0);
                let new_rect = Rect::from_center_size(original_center, new_outer_size);

                self.blocks[idx].position = new_rect.min;
                self.blocks[idx].set_preferred_size(new_size);

                // Handle chained blocks group resizing
                if self.blocks[idx].chained {
                    let chained_count = self.blocks.iter().filter(|b| b.chained).count();
                    if chained_count > 1 {
                        for i in 0..self.blocks.len() {
                            if self.blocks[i].chained && i != idx {
                                let aspect_ratio = self.blocks[i].aspect_ratio;
                                let chained_width = (new_height * aspect_ratio).max(MIN_BLOCK_SIZE);
                                let chained_size = vec2(chained_width, new_height);
                                let chained_outer_size =
                                    chained_size + Vec2::splat(BLOCK_PADDING * 2.0);

                                let original_center = self.blocks[i].rect().center();
                                let chained_rect =
                                    Rect::from_center_size(original_center, chained_outer_size);

                                self.blocks[i].position = chained_rect.min;
                                self.blocks[i].set_preferred_size(chained_size);
                            }
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for MaBlocksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.show_file_names = !self.show_file_names;
        }

        let dt = ctx.input(|i| i.unstable_dt).max(0.0);
        self.advance_animations(dt, ctx);
        self.enforce_chain_constraints();

        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::default()
                    .fill(Color32::from_rgb(30, 30, 30))
                    .inner_margin(0.0)
                    .outer_margin(0.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new(RichText::new("💾").size(24.0))
                                .min_size(Vec2::new(32.0, 32.0))
                                .frame(false),
                        )
                        .on_hover_text("Save Session")
                        .clicked()
                    {
                        self.save_session();
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("📂").size(24.0))
                                .min_size(Vec2::new(32.0, 32.0))
                                .frame(false),
                        )
                        .on_hover_text("Load Session")
                        .clicked()
                    {
                        self.load_session(ctx);
                    }

                    if ui
                        .add(
                            egui::Button::new(RichText::new("🖼").size(24.0))
                                .min_size(Vec2::new(32.0, 32.0))
                                .frame(false),
                        )
                        .on_hover_text("Add Image")
                        .clicked()
                    {
                        self.load_images(ctx);
                    }

                    if ui
                        .add(
                            egui::Button::new(RichText::new("🔄").size(24.0))
                                .min_size(Vec2::new(32.0, 32.0))
                                .frame(false),
                        )
                        .on_hover_text("Reset All Counters")
                        .clicked()
                    {
                        self.reset_all_counters();
                    }

                    if ui
                        .add(
                            egui::Button::new(RichText::new("📦").size(24.0))
                                .min_size(Vec2::new(32.0, 32.0))
                                .frame(false),
                        )
                        .on_hover_text("Compact/Unbox Group")
                        .clicked()
                    {
                        self.toggle_compact_group(ctx);
                    }
                });
            });

        let mut dropped_leader_id = None;
        let mut should_reflow = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            // Zoom handling: Ctrl + Scroll
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if zoom_delta != 1.0 {
                self.zoom = (self.zoom * zoom_delta).clamp(0.1, 10.0);
            }

            let available_width = ui.available_width();
            let mut target_inner_width = if available_width.is_finite() {
                (available_width / self.zoom - CANVAS_PADDING * 2.0).max(MIN_CANVAS_INNER_WIDTH)
            } else {
                CANVAS_WORKING_WIDTH / self.zoom
            };
            if target_inner_width.is_nan() {
                target_inner_width = CANVAS_WORKING_WIDTH / self.zoom;
            }
            if (target_inner_width - self.working_inner_width).abs() > 0.5 {
                self.working_inner_width = target_inner_width;
                self.reflow_blocks();
            }

            let input = ui.input(|i| i.clone());
            let mouse_pos = input.pointer.hover_pos();
            let secondary_pressed = input.pointer.button_pressed(egui::PointerButton::Secondary);
            let secondary_released = input
                .pointer
                .button_released(egui::PointerButton::Secondary);

            if secondary_released {
                self.resizing_state = None;
                should_reflow = true;
            }

            if let Some(curr_mouse_pos) = mouse_pos {
                self.handle_resizing(curr_mouse_pos);
            }

            egui::ScrollArea::both()
                .id_salt("main_canvas")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // MMB Panning: Pan the canvas even over blocks
                    if ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle)) {
                        let delta = ui.input(|i| i.pointer.delta());
                        ui.scroll_with_delta(delta);
                    }

                    let zoom = self.zoom;
                    let content_height = self
                        .blocks
                        .iter()
                        .map(|b| b.position.y + b.outer_size().y)
                        .fold(0.0, |a: f32, b| a.max(b));
                    let min_height = ui.available_height() / zoom;
                    let canvas_height = (content_height + CANVAS_PADDING).max(min_height);

                    let canvas_size = vec2(
                        (self.working_inner_width + CANVAS_PADDING * 2.0) * zoom,
                        canvas_height * zoom,
                    );
                    let (canvas_rect, _) = ui.allocate_exact_size(canvas_size, Sense::hover());

                    let mut canvas_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(canvas_rect)
                            .layout(egui::Layout::default()),
                    );
                    let canvas_origin = canvas_rect.min;

                    // Detect which box is being hovered by a dragged block
                    self.hovered_box_id = None;
                    if let Some(dragging_idx) = self.blocks.iter().position(|b| b.is_dragging) {
                        if !self.blocks[dragging_idx].is_group {
                            if let Some(m_pos) = ui.input(|i| i.pointer.interact_pos()) {
                                let world_mouse = (m_pos - canvas_origin) / zoom;
                                if let Some(target_idx) = self.blocks.iter().position(|other| {
                                    other.id != self.blocks[dragging_idx].id
                                        && other.is_group
                                        && other.rect().contains(world_mouse.to_pos2())
                                }) {
                                    self.hovered_box_id = Some(self.blocks[target_idx].id);
                                }
                            }
                        }
                    }

                    let mut index = 0;
                    let mut hovered_box_to_render = None;
                    let mut dragging_blocks_to_render = Vec::new();

                    while index < self.blocks.len() {
                        let block_rect = Rect::from_min_size(
                            self.blocks[index].position * zoom,
                            self.blocks[index].outer_size() * zoom,
                        )
                        .translate(canvas_origin.to_vec2());

                        let block = &self.blocks[index];
                        let (close_rect, chain_rect, counter_rect) =
                            block_control_rects(block_rect, block, zoom);

                        let block_id = canvas_ui.id().with(block.id);

                        // We use a custom hit test to see if we should show controls
                        // and to handle button clicks before the block gets them.
                        let mouse_pos = ui.input(|i| i.pointer.hover_pos());
                        let is_hovering_block = mouse_pos.is_some_and(|p| block_rect.contains(p));

                        let hover_state = BlockControlHover {
                            close_hovered: mouse_pos.is_some_and(|p| close_rect.contains(p)),
                            chain_hovered: mouse_pos.is_some_and(|p| chain_rect.contains(p)),
                            counter_hovered: !block.is_group
                                && mouse_pos.is_some_and(|p| counter_rect.contains(p)),
                        };

                        let any_button_hovered = hover_state.close_hovered
                            || hover_state.chain_hovered
                            || hover_state.counter_hovered;

                        // Sense clicks manually to avoid egui widget capture issues
                        let primary_clicked =
                            ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
                        let secondary_clicked =
                            ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));

                        let mut remove = false;
                        if primary_clicked {
                            if hover_state.close_hovered {
                                remove = true;
                                self.skip_chain_cancel = true;
                            } else if hover_state.chain_hovered {
                                self.toggle_chain_for_block(index);
                                // toggle_chain_for_block already sets skip_chain_cancel
                            } else if hover_state.counter_hovered {
                                self.blocks[index].counter += 1;
                                self.skip_chain_cancel = true;
                            }
                        }

                        if secondary_clicked && hover_state.counter_hovered {
                            self.blocks[index].counter = (self.blocks[index].counter - 1).max(0);
                            self.skip_chain_cancel = true;
                        }

                        // Resizing start: RMB + Drag
                        if secondary_pressed && is_hovering_block && !any_button_hovered {
                            if let Some(m_pos) = mouse_pos {
                                let world_mouse = (m_pos - canvas_origin) / zoom;
                                let center = self.blocks[index].rect().center();
                                let handle =
                                    match (world_mouse.x < center.x, world_mouse.y < center.y) {
                                        (true, true) => ResizeHandle::TopLeft,
                                        (false, true) => ResizeHandle::TopRight,
                                        (true, false) => ResizeHandle::BottomLeft,
                                        (false, false) => ResizeHandle::BottomRight,
                                    };
                                self.resizing_state = Some(InteractionState {
                                    id: self.blocks[index].id,
                                    handle,
                                    initial_mouse_pos: m_pos,
                                    initial_block_rect: self.blocks[index].rect(),
                                });
                            }
                        }

                        // Now handle the block itself
                        let block_sense = Sense::click_and_drag();

                        let response = canvas_ui.interact(block_rect, block_id, block_sense);

                        if response.drag_started_by(egui::PointerButton::Primary)
                            && !any_button_hovered
                        {
                            if let Some(pointer) = response.interact_pointer_pos() {
                                let block = &mut self.blocks[index];
                                block.drag_offset = (pointer - canvas_origin) / zoom
                                    - vec2(block.position.x, block.position.y);
                                block.is_dragging = true;
                            }
                        }

                        if self.blocks[index].is_dragging
                            && response.dragged_by(egui::PointerButton::Primary)
                        {
                            if let Some(pointer) = response.interact_pointer_pos() {
                                // Autoscroll logic
                                let viewport = ui.clip_rect();
                                let mut scroll_delta = 0.0;
                                if pointer.y < viewport.min.y {
                                    scroll_delta = viewport.min.y - pointer.y;
                                } else if pointer.y > viewport.max.y {
                                    scroll_delta = viewport.max.y - pointer.y;
                                }

                                if scroll_delta != 0.0 {
                                    ui.scroll_with_delta(vec2(0.0, scroll_delta));
                                    ui.ctx().request_repaint();
                                }

                                let old_pos = self.blocks[index].position;
                                let current_canvas_origin = canvas_origin + vec2(0.0, scroll_delta);
                                let new_pos = (pointer - current_canvas_origin) / zoom
                                    - self.blocks[index].drag_offset;
                                let delta = pos2(new_pos.x, new_pos.y) - old_pos;

                                let is_chained = self.blocks[index].chained;
                                let leader_id = self.blocks[index].id;

                                self.blocks[index].position = pos2(new_pos.x, new_pos.y);

                                if is_chained {
                                    for other in &mut self.blocks {
                                        if other.chained && other.id != leader_id {
                                            other.position += delta;
                                        }
                                    }
                                }
                            }
                        }

                        if self.blocks[index].is_dragging && response.drag_stopped() {
                            self.blocks[index].is_dragging = false;

                            let mut dropped_into_box = false;
                            if !self.blocks[index].is_group {
                                if let Some(m_pos) = ui.input(|i| i.pointer.interact_pos()) {
                                    let world_mouse = (m_pos - canvas_origin) / zoom;
                                    let target_idx = self.blocks.iter().position(|other| {
                                        other.id != self.blocks[index].id
                                            && other.is_group
                                            && other.rect().contains(world_mouse.to_pos2())
                                    });

                                    if let Some(t_idx) = target_idx {
                                        self.drop_block_into_box(index, t_idx);
                                        dropped_into_box = true;
                                        should_reflow = true;
                                    }
                                }
                            }

                            if dropped_into_box {
                                continue;
                            }

                            dropped_leader_id = Some(self.blocks[index].id);
                        }

                        let show_controls = is_hovering_block
                            || self.blocks[index].is_dragging
                            || self.blocks[index].chained;

                        let is_any_dragging = self.blocks.iter().any(|b| b.is_dragging);
                        let is_hovered_box = Some(self.blocks[index].id) == self.hovered_box_id;
                        let should_render_on_top = is_hovered_box
                            || self.blocks[index].is_dragging
                            || (is_any_dragging && self.blocks[index].chained);

                        if !should_render_on_top {
                            self.render_block(
                                &mut canvas_ui,
                                &self.blocks[index],
                                block_rect,
                                is_hovering_block,
                                show_controls,
                                hover_state,
                                false,
                            );
                        }

                        if is_hovered_box {
                            hovered_box_to_render = Some((
                                self.blocks[index].id,
                                block_rect,
                                is_hovering_block,
                                show_controls,
                                hover_state,
                            ));
                        } else if self.blocks[index].is_dragging
                            || (is_any_dragging && self.blocks[index].chained)
                        {
                            dragging_blocks_to_render.push((
                                self.blocks[index].id,
                                block_rect,
                                is_hovering_block,
                                show_controls,
                                hover_state,
                            ));
                        }

                        if primary_clicked && response.clicked() && !any_button_hovered {
                            if ui.input(|i| i.modifiers.ctrl) {
                                self.toggle_chain_for_block(index);
                            } else if self.blocks[index].frames.len() > 1 {
                                self.blocks[index].toggle_animation();
                            }
                        }

                        if remove {
                            self.blocks.remove(index);
                            should_reflow = true;
                        } else {
                            index += 1;
                        }
                    }

                    // Render dragging blocks and then hovered box on top
                    for (id, rect, h, s, state) in dragging_blocks_to_render {
                        if let Some(block) = self.blocks.iter().find(|b| b.id == id) {
                            self.render_block(&mut canvas_ui, block, rect, h, s, state, false);
                        }
                    }
                    if let Some((id, rect, h, s, state)) = hovered_box_to_render {
                        if let Some(block) = self.blocks.iter().find(|b| b.id == id) {
                            self.render_block(&mut canvas_ui, block, rect, h, s, state, true);
                        }
                    }

                    // Chaining cancellation
                    if !std::mem::take(&mut self.skip_chain_cancel) {
                        if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)) {
                            if let Some(click_pos) = ui.input(|i| i.pointer.interact_pos()) {
                                let world_click = (click_pos - canvas_origin) / zoom;
                                let hit_block = self
                                    .blocks
                                    .iter()
                                    .any(|b| b.rect().contains(world_click.to_pos2()));
                                if !hit_block {
                                    self.clear_chain_group();
                                }
                            }
                        }
                    }
                });
        });

        if let Some(leader_id) = dropped_leader_id {
            self.reorder_and_reflow(Some(leader_id));
        } else if should_reflow {
            self.reflow_blocks();
        }
    }
}

impl MaBlocksApp {
    fn save_session(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Session", &["json"])
            .set_file_name("ma_blocks_session.json")
            .save_file()
        {
            let session = Session {
                blocks: self.blocks.iter().map(|b| self.block_to_data(b)).collect(),
                remembered_chains: self
                    .remembered_chains
                    .iter()
                    .map(|chain| chain.iter().map(|id| id.to_string()).collect())
                    .collect(),
            };

            if let Ok(file) = std::fs::File::create(&path) {
                let _ = serde_json::to_writer_pretty(file, &session);
                self.session_file = Some(path);
            }
        }
    }

    fn block_to_data(&self, b: &ImageBlock) -> BlockData {
        BlockData {
            id: b.id,
            position: [b.position.x, b.position.y],
            size: [b.image_size.x, b.image_size.y],
            path: b.path.clone(),
            chained: b.chained,
            animation_enabled: b.animation_enabled,
            counter: b.counter,
            is_group: b.is_group,
            group_name: b.group_name.clone(),
            color: b.color.to_array(),
            children: b.children.iter().map(|c| self.block_to_data(c)).collect(),
        }
    }

    fn data_to_block(&mut self, ctx: &egui::Context, data: BlockData) -> Option<ImageBlock> {
        if data.is_group {
            let children: Vec<ImageBlock> = data
                .children
                .into_iter()
                .filter_map(|c| self.data_to_block(ctx, c))
                .collect();

            let texture = ctx.load_texture(
                format!("group-texture-{}", self.next_block_id),
                egui::ColorImage::new([1, 1], Color32::from_rgb(200, 180, 100)),
                egui::TextureOptions::LINEAR,
            );
            self.next_block_id += 1;

            let representative_texture = children.first().map(|c| c.texture.clone());

            let mut group =
                ImageBlock::new_group(data.group_name, children, texture, representative_texture);
            group.id = data.id;
            group.color = Color32::from_rgba_unmultiplied(
                data.color[0],
                data.color[1],
                data.color[2],
                data.color[3],
            );
            group.position = pos2(data.position[0], data.position[1]);
            group.set_preferred_size(vec2(data.size[0], data.size[1]));
            group.chained = data.chained;
            Some(group)
        } else {
            if data.path.is_empty() {
                return None;
            }
            if let Ok(path_buf) = Path::new(&data.path).canonicalize() {
                if let Ok(_) = self.insert_block_from_path(ctx, path_buf) {
                    if let Some(mut block) = self.blocks.pop() {
                        block.id = data.id;
                        block.color = Color32::from_rgba_unmultiplied(
                            data.color[0],
                            data.color[1],
                            data.color[2],
                            data.color[3],
                        );
                        block.position = pos2(data.position[0], data.position[1]);
                        block.set_preferred_size(vec2(data.size[0], data.size[1]));
                        block.chained = data.chained;
                        block.counter = data.counter;
                        if data.animation_enabled && block.frames.len() > 1 {
                            block.animation_enabled = true;
                        }
                        return Some(block);
                    }
                }
            }
            None
        }
    }

    fn load_session(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Session", &["json"])
            .pick_file()
        {
            if let Ok(file) = std::fs::File::open(&path) {
                if let Ok(session) =
                    serde_json::from_reader::<_, Session>(std::io::BufReader::new(file))
                {
                    self.blocks.clear();
                    for block_data in session.blocks {
                        if let Some(block) = self.data_to_block(ctx, block_data) {
                            self.blocks.push(block);
                        }
                    }
                    // Load remembered chains
                    self.remembered_chains = session
                        .remembered_chains
                        .into_iter()
                        .map(|chain| {
                            chain
                                .into_iter()
                                .filter_map(|s| Uuid::parse_str(&s).ok())
                                .collect()
                        })
                        .filter(|chain: &HashSet<Uuid>| chain.len() >= 2)
                        .collect();
                    self.session_file = Some(path);
                    self.reflow_blocks();
                }
            }
        }
    }

    fn reset_all_counters(&mut self) {
        for block in &mut self.blocks {
            block.reset_counters_recursive();
        }
    }
}

fn scaled_size(original: Vec2) -> Vec2 {
    let scale = (MAX_BLOCK_DIMENSION / original.x.max(1.0))
        .min(MAX_BLOCK_DIMENSION / original.y.max(1.0))
        .min(1.0);
    original * scale
}
