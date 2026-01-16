use crate::image_loader::AnimationFrame;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Vec2};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

pub const BLOCK_PADDING: f32 = 4.0;
pub const MIN_BLOCK_SIZE: f32 = 50.0;

#[derive(Clone, Copy, PartialEq)]
pub enum ResizeHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone)]
pub struct InteractionState {
    pub id: Uuid,
    pub handle: ResizeHandle,
    pub initial_mouse_pos: Pos2,
    pub initial_block_rect: Rect,
}

#[derive(Default, Clone, Copy)]
pub struct BlockControlHover {
    pub close_hovered: bool,
    pub chain_hovered: bool,
    pub counter_hovered: bool,
}

pub fn block_control_rects(rect: Rect, _block: &ImageBlock, zoom: f32) -> (Rect, Rect, Rect) {
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

pub struct ImageBlock {
    pub id: Uuid,
    pub path: String,
    pub texture: egui::TextureHandle,
    pub frames: Vec<AnimationFrame>,
    pub current_frame: usize,
    pub frame_elapsed: Duration,
    pub animation_enabled: bool,
    pub position: Pos2,
    pub drag_offset: Vec2,
    pub is_dragging: bool,
    pub image_size: Vec2,
    pub preferred_image_size: Vec2,
    pub aspect_ratio: f32,
    pub color: egui::Color32,
    pub chained: bool,
    pub counter: i32,
    pub is_group: bool,
    pub group_name: String,
    pub children: Vec<ImageBlock>,
    pub representative_texture: Option<egui::TextureHandle>,
    pub has_animation: bool,
    pub is_full_sequence: bool,
}

#[derive(Clone, Copy)]
pub struct BlockRenderConfig {
    pub zoom: f32,
    pub show_controls: bool,
    pub show_file_names: bool,
    pub can_chain: bool,
    pub is_drop_target: bool,
    pub hover_state: BlockControlHover,
}

impl ImageBlock {
    pub fn new(
        path: String,
        texture: egui::TextureHandle,
        frames: Vec<AnimationFrame>,
        image_size: Vec2,
        has_animation: bool,
        is_full_sequence: bool,
    ) -> Self {
        let aspect_ratio = if image_size.y > 0.0 {
            image_size.x / image_size.y
        } else {
            1.0
        };
        let id = Uuid::new_v4();
        let color = color_from_uuid(id);
        Self {
            id,
            path,
            texture,
            frames,
            current_frame: 0,
            frame_elapsed: Duration::ZERO,
            animation_enabled: false,
            position: egui::pos2(0.0, 0.0),
            drag_offset: Vec2::ZERO,
            is_dragging: false,
            image_size,
            preferred_image_size: image_size,
            aspect_ratio,
            color,
            chained: false,
            counter: 0,
            is_group: false,
            group_name: String::new(),
            children: Vec::new(),
            representative_texture: None,
            has_animation,
            is_full_sequence,
        }
    }

    pub fn new_group(
        name: String,
        children: Vec<ImageBlock>,
        texture: egui::TextureHandle,
        representative_texture: Option<egui::TextureHandle>,
    ) -> Self {
        let image_size = egui::vec2(160.0, 160.0);
        let id = Uuid::new_v4();
        let color = color_from_uuid(id);
        Self {
            id,
            path: String::new(),
            texture,
            frames: Vec::new(),
            current_frame: 0,
            frame_elapsed: Duration::ZERO,
            animation_enabled: false,
            position: egui::pos2(0.0, 0.0),
            drag_offset: Vec2::ZERO,
            is_dragging: false,
            image_size,
            preferred_image_size: image_size,
            aspect_ratio: 1.0,
            color,
            chained: false,
            counter: 0,
            is_group: true,
            group_name: name,
            children,
            representative_texture,
            has_animation: false,
            is_full_sequence: true,
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.position, self.outer_size())
    }

    pub fn outer_size(&self) -> Vec2 {
        egui::vec2(
            self.image_size.x + BLOCK_PADDING * 2.0,
            self.image_size.y + BLOCK_PADDING * 2.0,
        )
    }

    pub fn set_preferred_size(&mut self, size: Vec2) {
        self.preferred_image_size = size;
        self.image_size = size;
    }

    pub fn reset_to_preferred_size(&mut self) {
        self.image_size = self.preferred_image_size;
    }

    pub fn constrain_to_width(&mut self, max_width: f32) {
        if self.image_size.x <= max_width + f32::EPSILON {
            return;
        }

        let constrained_width = max_width.max(1.0);
        let constrained_height = constrained_width / self.aspect_ratio;
        self.image_size = egui::vec2(constrained_width, constrained_height);
    }

    pub fn update_animation(&mut self, dt: f32) -> bool {
        if !self.animation_enabled || self.frames.len() <= 1 {
            return false;
        }

        self.frame_elapsed += Duration::from_secs_f32(dt.max(0.0));
        let mut updated = false;
        while self.frame_elapsed >= self.frames[self.current_frame].duration {
            self.frame_elapsed -= self.frames[self.current_frame].duration;
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            let frame_image = self.frames[self.current_frame].image.clone();
            self.texture.set(frame_image, egui::TextureOptions::LINEAR);
            updated = true;
        }
        updated
    }

    pub fn time_until_next_frame(&self) -> Option<Duration> {
        if !self.animation_enabled || self.frames.len() <= 1 {
            return None;
        }

        let frame_duration = self.frames[self.current_frame].duration;
        let remaining = frame_duration.saturating_sub(self.frame_elapsed);
        if remaining.is_zero() {
            Some(Duration::from_millis(1))
        } else {
            Some(remaining)
        }
    }

    pub fn toggle_animation(&mut self) {
        if self.frames.len() <= 1 {
            return;
        }
        self.animation_enabled = !self.animation_enabled;
        if !self.animation_enabled {
            self.current_frame = 0;
            self.frame_elapsed = Duration::ZERO;
            let first = self.frames.first().cloned().unwrap();
            self.texture.set(first.image, egui::TextureOptions::LINEAR);
        }
    }

    pub fn reset_counters_recursive(&mut self) {
        self.counter = 0;
        for child in &mut self.children {
            child.reset_counters_recursive();
        }
    }

    pub fn render(&self, ui: &mut egui::Ui, rect: Rect, config: BlockRenderConfig) {
        let painter = ui.painter_at(rect);

        let image_rect = Rect::from_min_size(
            pos2(
                rect.min.x + BLOCK_PADDING * config.zoom,
                rect.min.y + BLOCK_PADDING * config.zoom,
            ),
            self.image_size * config.zoom,
        );

        let rounding = egui::Rounding::same(6.0 * config.zoom);

        if self.is_group {
            let fill_color = if self.chained || config.is_drop_target {
                Color32::from_rgb(100, 100, 150)
            } else {
                Color32::from_rgb(60, 60, 60)
            };
            painter.rect_filled(image_rect, rounding, fill_color);

            let folder_rect = Rect::from_center_size(image_rect.center(), image_rect.size() * 0.9);
            painter.rect_filled(
                folder_rect,
                egui::Rounding::same(2.0 * config.zoom),
                self.color,
            );
            painter.rect_filled(
                Rect::from_min_max(
                    folder_rect.left_top() - vec2(0.0, 5.0 * config.zoom),
                    folder_rect.left_top() + vec2(folder_rect.width() * 0.4, 0.0),
                ),
                egui::Rounding::same(1.0 * config.zoom),
                self.color,
            );

            if let Some(rep_texture) = &self.representative_texture {
                let tag_size = image_rect.size() * 0.8;
                let tag_rect = Rect::from_center_size(image_rect.center(), tag_size);
                let mut tag_shape = egui::epaint::RectShape::filled(
                    tag_rect,
                    egui::Rounding::same(2.0 * config.zoom),
                    Color32::WHITE,
                );
                tag_shape.fill_texture_id = rep_texture.id();
                tag_shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.add(tag_shape);
            }
        } else {
            let mut rect_shape =
                egui::epaint::RectShape::filled(image_rect, rounding, Color32::WHITE);
            rect_shape.fill_texture_id = self.texture.id();
            rect_shape.uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.add(rect_shape);
        }

        if config.show_controls {
            let (close_rect, chain_rect, counter_rect) =
                block_control_rects(rect, self, config.zoom);
            let btn_size = 16.0 * config.zoom;

            painter.circle_filled(
                close_rect.center(),
                btn_size / 2.0,
                if config.hover_state.close_hovered {
                    Color32::from_rgb(255, 100, 100)
                } else {
                    Color32::RED
                },
            );
            painter.text(
                close_rect.center(),
                Align2::CENTER_CENTER,
                "x",
                FontId::monospace(12.0 * config.zoom),
                Color32::WHITE,
            );

            let chain_color = if self.chained {
                Color32::GREEN
            } else if !config.can_chain {
                Color32::from_gray(80)
            } else if config.hover_state.chain_hovered {
                Color32::LIGHT_GRAY
            } else {
                Color32::GRAY
            };
            painter.circle_filled(chain_rect.center(), btn_size / 2.0, chain_color);
            painter.text(
                chain_rect.center(),
                Align2::CENTER_CENTER,
                "o",
                FontId::monospace(12.0 * config.zoom),
                Color32::WHITE,
            );

            if !self.is_group {
                painter.circle_filled(
                    counter_rect.center(),
                    btn_size / 2.0,
                    if config.hover_state.counter_hovered {
                        Color32::from_rgb(0, 150, 0)
                    } else {
                        Color32::from_rgb(0, 100, 0)
                    },
                );
                painter.text(
                    counter_rect.center(),
                    Align2::CENTER_CENTER,
                    "#",
                    FontId::monospace(12.0 * config.zoom),
                    Color32::WHITE,
                );
            }
        }

        if !self.is_group && self.counter > 0 {
            let circle_radius = 15.0 * config.zoom;
            let circle_center = pos2(
                rect.min.x + circle_radius + 5.0 * config.zoom,
                rect.min.y + circle_radius + 5.0 * config.zoom,
            );
            painter.circle_filled(
                circle_center,
                circle_radius,
                Color32::from_rgba_unmultiplied(0, 100, 0, 170),
            );
            painter.text(
                circle_center,
                Align2::CENTER_CENTER,
                self.counter.to_string(),
                FontId::proportional(20.0 * config.zoom),
                Color32::WHITE,
            );
        }

        if config.show_file_names {
            let name = if self.is_group {
                &self.group_name
            } else {
                Path::new(&self.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            };

            let font_id = FontId::proportional(12.0 * config.zoom);
            let galley = ui
                .painter()
                .layout_no_wrap(name.to_string(), font_id, Color32::WHITE);

            let text_pos = image_rect.left_top() + vec2(4.0 * config.zoom, 4.0 * config.zoom);
            let text_rect = Rect::from_min_size(text_pos, galley.size());

            painter.rect_filled(
                text_rect.expand(2.0 * config.zoom),
                egui::Rounding::same(2.0 * config.zoom),
                Color32::from_black_alpha(180),
            );
            painter.galley(text_pos, galley, Color32::WHITE);
        }
    }
}

pub fn handle_blocks_resizing(
    blocks: &mut [ImageBlock],
    resizing_state: &InteractionState,
    curr_mouse_pos: Pos2,
    zoom: f32,
) {
    if let Some(idx) = blocks.iter().position(|b| b.id == resizing_state.id) {
        let delta_world = (curr_mouse_pos - resizing_state.initial_mouse_pos) / zoom;
        let original_center = resizing_state.initial_block_rect.center();
        let min_size = MIN_BLOCK_SIZE;

        let initial_image_width = resizing_state.initial_block_rect.width() - BLOCK_PADDING * 2.0;
        let initial_image_height = resizing_state.initial_block_rect.height() - BLOCK_PADDING * 2.0;
        let half_width = initial_image_width * 0.5;
        let half_height = initial_image_height * 0.5;

        let x_sign = match resizing_state.handle {
            ResizeHandle::TopLeft | ResizeHandle::BottomLeft => -1.0,
            _ => 1.0,
        };
        let y_sign = match resizing_state.handle {
            ResizeHandle::TopLeft | ResizeHandle::TopRight => -1.0,
            _ => 1.0,
        };

        let target_offset_x = half_width * x_sign + delta_world.x;
        let width_from_x = (2.0 * target_offset_x.abs()).max(min_size);

        let target_offset_y = half_height * y_sign + delta_world.y;
        let height_from_y = 2.0 * target_offset_y.abs();
        let width_from_y = (height_from_y * blocks[idx].aspect_ratio).max(min_size);

        let mut new_width = if delta_world.x.abs() >= delta_world.y.abs() {
            width_from_x
        } else {
            width_from_y
        };

        if !new_width.is_finite() || new_width.is_nan() {
            new_width = min_size;
        }
        new_width = new_width.max(min_size);

        let new_height = new_width / blocks[idx].aspect_ratio;
        let new_size = vec2(new_width, new_height);
        let new_outer_size = new_size + Vec2::splat(BLOCK_PADDING * 2.0);
        let new_rect = Rect::from_center_size(original_center, new_outer_size);

        blocks[idx].position = new_rect.min;
        blocks[idx].set_preferred_size(new_size);

        if blocks[idx].chained {
            let chained_count = blocks.iter().filter(|b| b.chained).count();
            if chained_count > 1 {
                for i in 0..blocks.len() {
                    if blocks[i].chained && i != idx {
                        let aspect_ratio = blocks[i].aspect_ratio;
                        let chained_width = (new_height * aspect_ratio).max(MIN_BLOCK_SIZE);
                        let chained_size = vec2(chained_width, new_height);
                        let chained_outer_size = chained_size + Vec2::splat(BLOCK_PADDING * 2.0);

                        let original_center = blocks[i].rect().center();
                        let chained_rect =
                            Rect::from_center_size(original_center, chained_outer_size);

                        blocks[i].position = chained_rect.min;
                        blocks[i].set_preferred_size(chained_size);
                    }
                }
            }
        }
    }
}

fn color_from_uuid(id: Uuid) -> egui::Color32 {
    let b = id.as_bytes();
    // Generate a vibrant color from the UUID bytes
    let h = (b[0] as f32 + b[1] as f32 * 256.0) / 65535.0;
    let s = 0.6 + (b[2] as f32 / 255.0) * 0.4;
    let l = 0.5 + (b[3] as f32 / 255.0) * 0.2;
    egui::Color32::from(egui::epaint::Hsva::new(h, s, l, 1.0))
}
