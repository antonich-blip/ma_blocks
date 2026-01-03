use crate::image_loader::AnimationFrame;
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::time::Duration;
use uuid::Uuid;

pub const BLOCK_PADDING: f32 = 4.0;

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
}

impl ImageBlock {
    pub fn new(
        path: String,
        texture: egui::TextureHandle,
        frames: Vec<AnimationFrame>,
        image_size: Vec2,
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
}

fn color_from_uuid(id: Uuid) -> egui::Color32 {
    let b = id.as_bytes();
    // Generate a vibrant color from the UUID bytes
    let h = (b[0] as f32 + b[1] as f32 * 256.0) / 65535.0;
    let s = 0.6 + (b[2] as f32 / 255.0) * 0.4;
    let l = 0.5 + (b[3] as f32 / 255.0) * 0.2;
    egui::Color32::from(egui::epaint::Hsva::new(h, s, l, 1.0))
}
