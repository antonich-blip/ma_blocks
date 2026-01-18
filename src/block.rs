use crate::constants::{
    BLOCK_CORNER_RADIUS, BLOCK_PADDING, BUTTON_BASE_SIZE, BUTTON_HIT_AREA_MULTIPLIER,
    BUTTON_ICON_FONT_SIZE, BUTTON_SPACING, COLOR_CHAINED_GROUP_BG, COLOR_CHAIN_ACTIVE,
    COLOR_CHAIN_DISABLED, COLOR_CHAIN_HOVER, COLOR_CHAIN_NORMAL, COLOR_CLOSE_BUTTON,
    COLOR_CLOSE_BUTTON_HOVER, COLOR_COUNTER_BADGE, COLOR_COUNTER_BUTTON,
    COLOR_COUNTER_BUTTON_HOVER, COLOR_LABEL_BG_ALPHA, COLOR_NORMAL_GROUP_BG, COUNTER_BADGE_OFFSET,
    COUNTER_BADGE_RADIUS, COUNTER_FONT_SIZE, DEFAULT_GROUP_SIZE, FOLDER_CORNER_RADIUS,
    FOLDER_PREVIEW_SCALE, FOLDER_TAB_CORNER_RADIUS, FOLDER_TAB_HEIGHT, FOLDER_TAB_WIDTH_RATIO,
    GROUP_TEXTURE_SCALE, LABEL_BG_EXPANSION, LABEL_FONT_SIZE, LABEL_PADDING, MIN_BLOCK_SIZE,
    ROW_QUANTIZATION_HEIGHT, UUID_COLOR_LIGHTNESS_MIN, UUID_COLOR_LIGHTNESS_RANGE,
    UUID_COLOR_SATURATION_MIN, UUID_COLOR_SATURATION_RANGE,
};
use crate::image_loader::AnimationFrame;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Vec2};
use std::cmp::Ordering;
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

/// Defines the four corners of a block that can be used for resizing.
#[derive(Clone, Copy, PartialEq)]
pub enum ResizeHandle {
    /// Top-left corner handle.
    TopLeft,
    /// Top-right corner handle.
    TopRight,
    /// Bottom-left corner handle.
    BottomLeft,
    /// Bottom-right corner handle.
    BottomRight,
}

/// Tracks the active state of a block resizing operation, including initial positions for delta calculation.
#[derive(Clone)]
pub struct InteractionState {
    pub id: Uuid,
    pub handle: ResizeHandle,
    pub initial_mouse_pos: Pos2,
    pub initial_block_rect: Rect,
}

/// Manages the spatial properties and dragging state of a block.
pub struct BlockPosition {
    pub position: Pos2,
    pub drag_offset: Vec2,
    pub is_dragging: bool,
}

/// Manages the animation sequence and playback state for a block.
pub struct AnimationState {
    pub frames: Vec<AnimationFrame>,
    pub current_frame: usize,
    pub frame_elapsed: Duration,
    pub animation_enabled: bool,
    pub has_animation: bool,
}

/// Manages group-related data when multiple blocks are combined.
pub struct GroupData {
    pub is_group: bool,
    pub group_name: String,
    pub children: Vec<ImageBlock>,
    pub representative_texture: Option<egui::TextureHandle>,
}

/// Tracks which control elements of a block are currently hovered by the pointer.
#[derive(Default, Clone, Copy)]
pub struct BlockControlHover {
    pub close_hovered: bool,
    pub chain_hovered: bool,
    pub counter_hovered: bool,
}

impl BlockControlHover {
    pub fn from_mouse_pos(
        mouse_pos: Option<Pos2>,
        rects: &(Rect, Rect, Rect),
        is_group: bool,
    ) -> Self {
        let (close_rect, chain_rect, counter_rect) = rects;
        Self {
            close_hovered: mouse_pos.is_some_and(|p| close_rect.contains(p)),
            chain_hovered: mouse_pos.is_some_and(|p| chain_rect.contains(p)),
            counter_hovered: !is_group && mouse_pos.is_some_and(|p| counter_rect.contains(p)),
        }
    }
}

/// Calculates the hit-rects for block control buttons (close, chain, counter) based on the block's current rect and zoom.
pub fn block_control_rects(rect: Rect, zoom: f32) -> (Rect, Rect, Rect) {
    let btn_size = BUTTON_BASE_SIZE * zoom;
    let btn_spacing = BUTTON_SPACING * zoom;
    let btn_hit_size = btn_size * BUTTON_HIT_AREA_MULTIPLIER;

    let close_rect = Rect::from_center_size(
        rect.right_top()
            + Vec2::new(
                -btn_hit_size / 2.0 - BUTTON_SPACING * zoom,
                btn_hit_size / 2.0 + BUTTON_SPACING * zoom,
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

/// The core entity of the application, representing an image or a group of images.
/// It handles its own rendering, animation state, and interaction properties.
pub struct ImageBlock {
    pub id: Uuid,
    pub path: String,
    pub texture: egui::TextureHandle,
    pub pos: BlockPosition,
    pub anim: AnimationState,
    pub group: GroupData,
    pub image_size: Vec2,
    pub preferred_image_size: Vec2,
    pub aspect_ratio: f32,
    pub color: egui::Color32,
    pub chained: bool,
    pub counter: i32,
    pub is_full_sequence: bool,
}

/// Contextual configuration passed during the rendering phase of a block.
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
    /// Creates a new ImageBlock for a single image or the first frame of an animation.
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
            pos: BlockPosition {
                position: egui::pos2(0.0, 0.0),
                drag_offset: Vec2::ZERO,
                is_dragging: false,
            },
            anim: AnimationState {
                frames,
                current_frame: 0,
                frame_elapsed: Duration::ZERO,
                animation_enabled: false,
                has_animation,
            },
            group: GroupData {
                is_group: false,
                group_name: String::new(),
                children: Vec::new(),
                representative_texture: None,
            },
            image_size,
            preferred_image_size: image_size,
            aspect_ratio,
            color,
            chained: false,
            counter: 0,
            is_full_sequence,
        }
    }

    /// Creates a new group block that contains multiple child blocks.
    pub fn new_group(
        name: String,
        children: Vec<ImageBlock>,
        texture: egui::TextureHandle,
        representative_texture: Option<egui::TextureHandle>,
    ) -> Self {
        let image_size = egui::vec2(DEFAULT_GROUP_SIZE, DEFAULT_GROUP_SIZE);
        let id = Uuid::new_v4();
        let color = color_from_uuid(id);
        Self {
            id,
            path: String::new(),
            texture,
            pos: BlockPosition {
                position: egui::pos2(0.0, 0.0),
                drag_offset: Vec2::ZERO,
                is_dragging: false,
            },
            anim: AnimationState {
                frames: Vec::new(),
                current_frame: 0,
                frame_elapsed: Duration::ZERO,
                animation_enabled: false,
                has_animation: false,
            },
            group: GroupData {
                is_group: true,
                group_name: name,
                children,
                representative_texture,
            },
            image_size,
            preferred_image_size: image_size,
            aspect_ratio: 1.0,
            color,
            chained: false,
            counter: 0,
            is_full_sequence: true,
        }
    }

    /// Returns the bounding rectangle of the block in its current position.
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.pos.position, self.outer_size())
    }

    /// Returns the drag offset for this block.
    pub fn drag_offset(&self) -> Vec2 {
        self.pos.drag_offset
    }

    /// Returns the total size of the block including internal padding.
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

    /// Advances the animation state based on elapsed time. Returns true if the frame changed.
    pub fn update_animation(&mut self, dt: f32) -> bool {
        if !self.anim.animation_enabled || self.anim.frames.len() <= 1 {
            return false;
        }

        self.anim.frame_elapsed += Duration::from_secs_f32(dt.max(0.0));
        let mut updated = false;
        while self.anim.frame_elapsed >= self.anim.frames[self.anim.current_frame].duration {
            self.anim.frame_elapsed -= self.anim.frames[self.anim.current_frame].duration;
            self.anim.current_frame = (self.anim.current_frame + 1) % self.anim.frames.len();
            let frame_image = self.anim.frames[self.anim.current_frame].image.clone();
            self.texture.set(frame_image, egui::TextureOptions::LINEAR);
            updated = true;
        }
        updated
    }

    pub fn time_until_next_frame(&self) -> Option<Duration> {
        if !self.anim.animation_enabled || self.anim.frames.len() <= 1 {
            return None;
        }

        let frame_duration = self.anim.frames[self.anim.current_frame].duration;
        let remaining = frame_duration.saturating_sub(self.anim.frame_elapsed);
        if remaining.is_zero() {
            Some(Duration::from_millis(1))
        } else {
            Some(remaining)
        }
    }

    pub fn toggle_animation(&mut self) {
        if self.anim.frames.len() <= 1 {
            return;
        }
        self.anim.animation_enabled = !self.anim.animation_enabled;
        if !self.anim.animation_enabled {
            self.stop_animation();
        }
    }

    pub fn stop_animation(&mut self) {
        self.anim.animation_enabled = false;
        self.anim.current_frame = 0;
        self.anim.frame_elapsed = Duration::ZERO;
        if let Some(first) = self.anim.frames.first() {
            self.texture
                .set(first.image.clone(), egui::TextureOptions::LINEAR);
        }
    }

    pub fn reset_counters_recursive(&mut self) {
        self.counter = 0;
        for child in &mut self.group.children {
            child.reset_counters_recursive();
        }
    }

    pub fn cmp_layout(&self, other: &Self) -> Ordering {
        match (self.group.is_group, other.group.is_group) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                let a_y_q = (self.pos.position.y / ROW_QUANTIZATION_HEIGHT) as i32;
                let b_y_q = (other.pos.position.y / ROW_QUANTIZATION_HEIGHT) as i32;
                match a_y_q.cmp(&b_y_q) {
                    Ordering::Equal => self
                        .pos
                        .position
                        .x
                        .partial_cmp(&other.pos.position.x)
                        .unwrap_or(Ordering::Equal),
                    ord => ord,
                }
            }
        }
    }

    pub fn update_group_name(&mut self) {
        if !self.group.is_group {
            return;
        }
        self.group.group_name = if self.group.children.len() > 1 {
            format!("Group of {}", self.group.children.len())
        } else if self.group.children.len() == 1 {
            format!(
                "Box: {}",
                Path::new(&self.group.children[0].path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            )
        } else {
            "Empty Group".to_string()
        };
    }

    /// Renders the block and its controls to the UI.
    pub fn render(&self, ui: &mut egui::Ui, rect: Rect, config: BlockRenderConfig) {
        let painter = ui.painter_at(rect);

        let image_rect = Rect::from_min_size(
            pos2(
                rect.min.x + BLOCK_PADDING * config.zoom,
                rect.min.y + BLOCK_PADDING * config.zoom,
            ),
            self.image_size * config.zoom,
        );

        let rounding = egui::Rounding::same(BLOCK_CORNER_RADIUS * config.zoom);

        if self.group.is_group {
            let fill_color = if self.chained || config.is_drop_target {
                COLOR_CHAINED_GROUP_BG
            } else {
                COLOR_NORMAL_GROUP_BG
            };
            painter.rect_filled(image_rect, rounding, fill_color);

            let folder_rect = Rect::from_center_size(
                image_rect.center(),
                image_rect.size() * FOLDER_PREVIEW_SCALE,
            );
            painter.rect_filled(
                folder_rect,
                egui::Rounding::same(FOLDER_CORNER_RADIUS * config.zoom),
                self.color,
            );
            painter.rect_filled(
                Rect::from_min_max(
                    folder_rect.left_top() - vec2(0.0, FOLDER_TAB_HEIGHT * config.zoom),
                    folder_rect.left_top()
                        + vec2(folder_rect.width() * FOLDER_TAB_WIDTH_RATIO, 0.0),
                ),
                egui::Rounding::same(FOLDER_TAB_CORNER_RADIUS * config.zoom),
                self.color,
            );

            if let Some(rep_texture) = &self.group.representative_texture {
                let available_size = image_rect.size() * GROUP_TEXTURE_SCALE;
                let tex_size = rep_texture.size_vec2();
                let tex_aspect = tex_size.x / tex_size.y;
                let available_aspect = available_size.x / available_size.y;

                let tag_size = if tex_aspect > available_aspect {
                    // Texture is wider than available space - fit to width
                    vec2(available_size.x, available_size.x / tex_aspect)
                } else {
                    // Texture is taller than available space - fit to height
                    vec2(available_size.y * tex_aspect, available_size.y)
                };

                let tag_rect = Rect::from_center_size(image_rect.center(), tag_size);
                let mut tag_shape = egui::epaint::RectShape::filled(
                    tag_rect,
                    egui::Rounding::same(FOLDER_CORNER_RADIUS * config.zoom),
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
            let (close_rect, chain_rect, counter_rect) = block_control_rects(rect, config.zoom);
            let btn_size = BUTTON_BASE_SIZE * config.zoom;

            painter.circle_filled(
                close_rect.center(),
                btn_size / 2.0,
                if config.hover_state.close_hovered {
                    COLOR_CLOSE_BUTTON_HOVER
                } else {
                    COLOR_CLOSE_BUTTON
                },
            );
            painter.text(
                close_rect.center(),
                Align2::CENTER_CENTER,
                "x",
                FontId::monospace(BUTTON_ICON_FONT_SIZE * config.zoom),
                Color32::WHITE,
            );

            let chain_color = if self.chained {
                COLOR_CHAIN_ACTIVE
            } else if !config.can_chain {
                COLOR_CHAIN_DISABLED
            } else if config.hover_state.chain_hovered {
                COLOR_CHAIN_HOVER
            } else {
                COLOR_CHAIN_NORMAL
            };
            painter.circle_filled(chain_rect.center(), btn_size / 2.0, chain_color);
            painter.text(
                chain_rect.center(),
                Align2::CENTER_CENTER,
                "o",
                FontId::monospace(BUTTON_ICON_FONT_SIZE * config.zoom),
                Color32::WHITE,
            );

            if !self.group.is_group {
                painter.circle_filled(
                    counter_rect.center(),
                    btn_size / 2.0,
                    if config.hover_state.counter_hovered {
                        COLOR_COUNTER_BUTTON_HOVER
                    } else {
                        COLOR_COUNTER_BUTTON
                    },
                );
                painter.text(
                    counter_rect.center(),
                    Align2::CENTER_CENTER,
                    "#",
                    FontId::monospace(BUTTON_ICON_FONT_SIZE * config.zoom),
                    Color32::WHITE,
                );
            }
        }

        if !self.group.is_group && self.counter > 0 {
            let circle_radius = COUNTER_BADGE_RADIUS * config.zoom;
            let circle_center = pos2(
                rect.min.x + circle_radius + COUNTER_BADGE_OFFSET * config.zoom,
                rect.min.y + circle_radius + COUNTER_BADGE_OFFSET * config.zoom,
            );
            painter.circle_filled(circle_center, circle_radius, COLOR_COUNTER_BADGE);
            painter.text(
                circle_center,
                Align2::CENTER_CENTER,
                self.counter.to_string(),
                FontId::proportional(COUNTER_FONT_SIZE * config.zoom),
                Color32::WHITE,
            );
        }

        if config.show_file_names {
            let name = if self.group.is_group {
                &self.group.group_name
            } else {
                Path::new(&self.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
            };

            let font_id = FontId::proportional(LABEL_FONT_SIZE * config.zoom);
            let galley = ui
                .painter()
                .layout_no_wrap(name.to_string(), font_id, Color32::WHITE);

            let text_pos = image_rect.left_top()
                + vec2(LABEL_PADDING * config.zoom, LABEL_PADDING * config.zoom);
            let text_rect = Rect::from_min_size(text_pos, galley.size());

            painter.rect_filled(
                text_rect.expand(LABEL_BG_EXPANSION * config.zoom),
                egui::Rounding::same(FOLDER_CORNER_RADIUS * config.zoom),
                Color32::from_black_alpha(COLOR_LABEL_BG_ALPHA),
            );
            painter.galley(text_pos, galley, Color32::WHITE);
        }
    }
}

/// Returns the index of a block by its ID within a slice, or None if not found.
pub fn block_index_in_slice(blocks: &[ImageBlock], id: Uuid) -> Option<usize> {
    blocks.iter().position(|b| b.id == id)
}

/// Handles the resizing logic for a set of blocks based on pointer movement and the current resizing state.
pub fn handle_blocks_resizing(
    blocks: &mut [ImageBlock],
    resizing_state: &InteractionState,
    curr_mouse_pos: Pos2,
    zoom: f32,
) {
    if let Some(idx) = block_index_in_slice(blocks, resizing_state.id) {
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

        blocks[idx].pos.position = new_rect.min;
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

                        blocks[i].pos.position = chained_rect.min;
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
    let s = UUID_COLOR_SATURATION_MIN + (b[2] as f32 / 255.0) * UUID_COLOR_SATURATION_RANGE;
    let l = UUID_COLOR_LIGHTNESS_MIN + (b[3] as f32 / 255.0) * UUID_COLOR_LIGHTNESS_RANGE;
    egui::Color32::from(egui::epaint::Hsva::new(h, s, l, 1.0))
}
