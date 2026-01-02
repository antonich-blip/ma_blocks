mod block;
mod image_loader;

use block::{ImageBlock, BLOCK_PADDING};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, UiBuilder, Vec2};
use egui::{pos2, vec2};
use image_loader::load_image_frames;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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
}

struct MaBlocksApp {
    blocks: Vec<ImageBlock>,
    next_block_id: usize,
    resizing_state: Option<InteractionState>,
    skip_chain_cancel: bool,
    working_inner_width: f32,
    session_file: Option<PathBuf>,
    zoom: f32,
}

#[derive(Default, Clone, Copy)]
struct BlockControlHover {
    close_hovered: bool,
    chain_hovered: bool,
    counter_hovered: bool,
}

fn block_control_rects(rect: Rect, block: &ImageBlock, zoom: f32) -> (Rect, Rect, Rect) {
    let image_rect = Rect::from_min_size(
        pos2(
            rect.min.x + BLOCK_PADDING * zoom,
            rect.min.y + BLOCK_PADDING * zoom,
        ),
        block.image_size * zoom,
    );

    let btn_size = 16.0 * zoom;
    let margin_top = 12.0 * zoom;
    let margin_right = 12.0 * zoom;
    let btn_spacing = 6.0 * zoom;
    let top_right = image_rect.right_top();

    let close_rect = Rect::from_center_size(
        top_right + Vec2::new(-margin_right - btn_size / 2.0, margin_top + btn_size / 2.0),
        Vec2::splat(btn_size),
    );

    let chain_rect = Rect::from_center_size(
        close_rect.center() - Vec2::new(btn_size + btn_spacing, 0.0),
        Vec2::splat(btn_size),
    );

    let counter_rect = Rect::from_center_size(
        chain_rect.center() - Vec2::new(btn_size + btn_spacing, 0.0),
        Vec2::splat(btn_size),
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
        self.blocks.len() > 1
    }

    fn clear_chain_group(&mut self) {
        if self.blocks.iter().any(|b| b.chained) {
            for block in &mut self.blocks {
                block.chained = false;
            }
        }
        self.skip_chain_cancel = false;
    }

    fn enforce_chain_constraints(&mut self) {
        if !self.can_chain() {
            self.clear_chain_group();
        }
    }

    fn toggle_chain_for_block(&mut self, index: usize) {
        if !self.can_chain() {
            return;
        }

        self.blocks[index].chained = !self.blocks[index].chained;
        self.skip_chain_cancel = true;
    }

    fn reorder_and_reflow(&mut self) {
        self.blocks.sort_by(|a, b| {
            // Bucket Y coordinates to group blocks into "rows" for sorting.
            // This allows for some vertical drift during dragging without jumping rows.
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
        self.reflow_blocks();
    }

    fn render_block(
        &self,
        ui: &mut egui::Ui,
        block: &ImageBlock,
        rect: Rect,
        _hovered: bool,
        show_controls: bool,
    ) -> BlockControlHover {
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
        let mut rect_shape = egui::epaint::RectShape::filled(image_rect, rounding, Color32::WHITE);
        rect_shape.fill_texture_id = block.texture.id();
        rect_shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.add(rect_shape);

        let mut hover_state = BlockControlHover::default();

        if show_controls {
            let (close_rect, chain_rect, counter_rect) = block_control_rects(rect, block, zoom);
            // block_control_rects already uses the passed rect which is scaled

            let btn_size = 16.0 * zoom;
            let chain_enabled = self.can_chain();

            let mouse_pos = ui.input(|i| i.pointer.hover_pos());
            hover_state.close_hovered = mouse_pos.is_some_and(|p| close_rect.contains(p));
            hover_state.chain_hovered =
                chain_enabled && mouse_pos.is_some_and(|p| chain_rect.contains(p));
            hover_state.counter_hovered = mouse_pos.is_some_and(|p| counter_rect.contains(p));

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

        // Draw counter if > 0
        if block.counter > 0 {
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

        hover_state
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
                });
            });

        let mut should_reorder = false;
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
            let secondary_down = input.pointer.secondary_down();
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

                    let mut index = 0;
                    while index < self.blocks.len() {
                        let block_rect = Rect::from_min_size(
                            self.blocks[index].position * zoom,
                            self.blocks[index].outer_size() * zoom,
                        )
                        .translate(canvas_origin.to_vec2());

                        let response = canvas_ui.allocate_rect(block_rect, Sense::click_and_drag());
                        let mut remove = false;

                        let block = &self.blocks[index];
                        let show_controls =
                            response.hovered() || block.is_dragging || block.chained;
                        let mut hover_state = BlockControlHover::default();
                        if show_controls {
                            let (close_rect, chain_rect, counter_rect) =
                                block_control_rects(block_rect, block, zoom);
                            let mouse_pos = ui.input(|i| i.pointer.hover_pos());
                            let chain_enabled = self.can_chain();
                            hover_state.close_hovered =
                                mouse_pos.is_some_and(|p| close_rect.contains(p));
                            hover_state.chain_hovered =
                                chain_enabled && mouse_pos.is_some_and(|p| chain_rect.contains(p));
                            hover_state.counter_hovered =
                                mouse_pos.is_some_and(|p| counter_rect.contains(p));
                        }

                        // Resizing start: RMB + Drag
                        if secondary_pressed
                            && response.hovered()
                            && !hover_state.counter_hovered
                            && !hover_state.close_hovered
                            && !hover_state.chain_hovered
                        {
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

                        // Dragging logic: LMB + Drag
                        if response.drag_started_by(egui::PointerButton::Primary)
                            && !hover_state.counter_hovered
                            && !hover_state.close_hovered
                            && !hover_state.chain_hovered
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
                                // Autoscroll logic: proportional to distance beyond borders
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
                            should_reorder = true;
                        }

                        self.render_block(
                            &mut canvas_ui,
                            &self.blocks[index],
                            block_rect,
                            response.hovered(),
                            show_controls,
                        );

                        if response.clicked() && !secondary_down {
                            if hover_state.close_hovered {
                                remove = true;
                            } else if hover_state.chain_hovered {
                                self.toggle_chain_for_block(index);
                            } else if hover_state.counter_hovered {
                                self.blocks[index].counter += 1;
                            } else if self.blocks[index].frames.len() > 1 {
                                // Toggle animation only if it's an animated format
                                self.blocks[index].toggle_animation();
                            }
                        } else if response.secondary_clicked() {
                            if hover_state.counter_hovered {
                                self.blocks[index].counter =
                                    (self.blocks[index].counter - 1).max(0);
                            }
                        }

                        if remove {
                            self.blocks.remove(index);
                            should_reflow = true;
                        } else {
                            index += 1;
                        }
                    }

                    // Chaining cancellation
                    if !std::mem::take(&mut self.skip_chain_cancel) {
                        if canvas_ui
                            .input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
                        {
                            if let Some(click_pos) = canvas_ui.input(|i| i.pointer.interact_pos()) {
                                let local_click = (click_pos - canvas_origin).to_pos2();
                                let hit_block =
                                    self.blocks.iter().any(|b| b.rect().contains(local_click));
                                if !hit_block {
                                    self.clear_chain_group();
                                }
                            }
                        }
                    }
                });
        });

        if should_reorder {
            self.reorder_and_reflow();
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
                blocks: self
                    .blocks
                    .iter()
                    .map(|b| BlockData {
                        id: b.id,
                        position: [b.position.x, b.position.y],
                        size: [b.image_size.x, b.image_size.y],
                        path: b.path.clone(),
                        chained: b.chained,
                        animation_enabled: b.animation_enabled,
                        counter: b.counter,
                    })
                    .collect(),
            };

            if let Ok(file) = std::fs::File::create(&path) {
                let _ = serde_json::to_writer_pretty(file, &session);
                self.session_file = Some(path);
            }
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
                        if block_data.path.is_empty() {
                            continue;
                        }
                        if let Ok(path_buf) = Path::new(&block_data.path).canonicalize() {
                            if let Ok(_) = self.insert_block_from_path(ctx, path_buf) {
                                if let Some(block) = self.blocks.last_mut() {
                                    block.position =
                                        pos2(block_data.position[0], block_data.position[1]);
                                    block.set_preferred_size(vec2(
                                        block_data.size[0],
                                        block_data.size[1],
                                    ));
                                    block.chained = block_data.chained;
                                    block.counter = block_data.counter;
                                    if block_data.animation_enabled && block.frames.len() > 1 {
                                        block.animation_enabled = true;
                                    }
                                }
                            }
                        }
                    }
                    self.session_file = Some(path);
                    self.reflow_blocks();
                }
            }
        }
    }

    fn reset_all_counters(&mut self) {
        for block in &mut self.blocks {
            block.counter = 0;
        }
    }
}

fn scaled_size(original: Vec2) -> Vec2 {
    let scale = (MAX_BLOCK_DIMENSION / original.x.max(1.0))
        .min(MAX_BLOCK_DIMENSION / original.y.max(1.0))
        .min(1.0);
    original * scale
}
