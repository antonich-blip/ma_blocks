mod block;
mod block_manager;
mod constants;
mod image_loader;
mod paths;

use block::{
    block_control_rects, handle_blocks_resizing, BlockControlHover, BlockRenderConfig, ImageBlock,
    InteractionState, ResizeHandle,
};
use block_manager::{BlockManager, ChainedIds};
use constants::{
    CANVAS_PADDING, CANVAS_WORKING_WIDTH, COLOR_GROUP_PLACEHOLDER, COLOR_TOOLBAR_BG,
    INITIAL_WINDOW_HEIGHT, INITIAL_WINDOW_WIDTH, MAX_BLOCK_DIMENSION, MIN_CANVAS_INNER_WIDTH,
    TOOLBAR_BUTTON_SIZE, TOOLBAR_ICON_SIZE, TOOLBAR_START_SPACING,
};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, UiBuilder, Vec2};
use egui::{pos2, vec2};
use paths::AppPaths;
use serde::{Deserialize, Serialize};

use std::path::PathBuf;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use uuid::Uuid;

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([INITIAL_WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT])
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

/// Represents a saved application session, containing blocks and their relationships.
#[derive(Serialize, Deserialize)]
struct Session {
    blocks: Vec<BlockData>,
    #[serde(default)]
    remembered_chains: Vec<Vec<String>>,
    #[serde(default)]
    last_unboxed_ids: Vec<Uuid>,
    #[serde(default)]
    last_boxed_id: Option<Uuid>,
    #[serde(default = "default_zoom")]
    zoom: f32,
    #[serde(default)]
    show_file_names: bool,
}

fn default_zoom() -> f32 {
    1.0
}

/// Serialized form of an ImageBlock for persistence.
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

/// Captures pointer and modifier state for a single frame.
struct InputSnapshot {
    hover_pos: Option<Pos2>,
    interact_pos: Option<Pos2>,
    primary_clicked: bool,
    secondary_clicked: bool,
    secondary_pressed: bool,
    secondary_released: bool,
    middle_down: bool,
    pointer_delta: Vec2,
    zoom_delta: f32,
    ctrl: bool,
    shift: bool,
}

impl InputSnapshot {
    fn from_ui(ui: &egui::Ui) -> Self {
        ui.input(|i| Self {
            hover_pos: i.pointer.hover_pos(),
            interact_pos: i.pointer.interact_pos(),
            primary_clicked: i.pointer.button_clicked(egui::PointerButton::Primary),
            secondary_clicked: i.pointer.button_clicked(egui::PointerButton::Secondary),
            secondary_pressed: i.pointer.button_pressed(egui::PointerButton::Secondary),
            secondary_released: i.pointer.button_released(egui::PointerButton::Secondary),
            middle_down: i.pointer.button_down(egui::PointerButton::Middle),
            pointer_delta: i.pointer.delta(),
            zoom_delta: i.zoom_delta(),
            ctrl: i.modifiers.ctrl,
            shift: i.modifiers.shift,
        })
    }
}

/// The main application state holding all blocks, UI interaction states, and resource management.
struct MaBlocksApp {
    block_manager: BlockManager,
    resizing_state: Option<InteractionState>,
    skip_chain_cancel: bool,
    working_inner_width: f32,
    session_file: Option<PathBuf>,
    zoom: f32,
    last_unboxed_ids: Vec<Uuid>,
    last_boxed_id: Option<Uuid>,
    show_file_names: bool,
    hovered_box_id: Option<Uuid>,
    image_rx: Option<Receiver<image_loader::ImageLoadResponse>>,
    image_tx: Sender<image_loader::ImageLoadResponse>,
    paths: Option<AppPaths>,
    last_auto_save_time: f64,
}

impl MaBlocksApp {
    // ─────────────────────────────────────────────────────────────────────────────
    // Block Lookup Helpers (delegate to BlockManager)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns the index of a block by its ID, or None if not found.
    fn block_index(&self, id: Uuid) -> Option<usize> {
        self.block_manager.index_of(id)
    }

    /// Returns an immutable reference to a block by its ID, or None if not found.
    fn block_by_id(&self, id: Uuid) -> Option<&ImageBlock> {
        self.block_manager.get(id)
    }

    /// Returns a mutable reference to a block by its ID, or None if not found.
    fn block_by_id_mut(&mut self, id: Uuid) -> Option<&mut ImageBlock> {
        self.block_manager.get_mut(id)
    }

    /// Returns a reference to the blocks slice.
    fn blocks(&self) -> &[ImageBlock] {
        self.block_manager.blocks()
    }

    /// Returns a mutable reference to the blocks slice.
    fn blocks_mut(&mut self) -> &mut [ImageBlock] {
        self.block_manager.blocks_mut()
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Initialization
    // ─────────────────────────────────────────────────────────────────────────────

    /// Initializes the application state, sets up channels, and discovers project directories.
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        let paths = AppPaths::from_project_dirs();
        if let Some(ref p) = paths {
            if let Err(err) = p.ensure_dirs_exist() {
                log::error!("Failed to create default directories: {err}");
            }
        }

        let mut app = Self {
            block_manager: BlockManager::new(),
            resizing_state: None,
            skip_chain_cancel: false,
            working_inner_width: CANVAS_WORKING_WIDTH,
            session_file: None,
            zoom: 1.0,
            last_unboxed_ids: Vec::new(),
            last_boxed_id: None,
            show_file_names: false,
            hovered_box_id: None,
            image_rx: Some(rx),
            image_tx: tx,
            paths,
            last_auto_save_time: 0.0,
        };

        if let Some(storage) = cc.storage {
            if let Some(session) = eframe::get_value::<Session>(storage, eframe::APP_KEY) {
                app.apply_session_data(&cc.egui_ctx, session);
            }
        }

        app
    }

    fn apply_session_data(&mut self, ctx: &egui::Context, session: Session) {
        self.block_manager.clear();
        for block_data in session.blocks {
            if let Some(block) = self.data_to_block_skeleton(ctx, block_data) {
                self.block_manager.push(block);
            }
        }
        self.block_manager
            .set_remembered_chains(Self::parse_remembered_chains(session.remembered_chains));
        self.last_unboxed_ids = session.last_unboxed_ids;
        self.last_boxed_id = session.last_boxed_id;
        self.zoom = session.zoom;
        self.show_file_names = session.show_file_names;
        self.reorder_and_reflow(None);
    }

    /// Opens a file dialog to pick images and triggers background loading for each.
    fn load_images(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "avif"]);

        if let Some(ref p) = self.paths {
            dialog = dialog.set_directory(&p.images);
        }

        if let Some(paths) = dialog.pick_files() {
            for path in paths {
                self.trigger_image_load(path, true);
            }
        }
    }

    /// Spawns a background thread to load and decode an image from the specified path.
    fn trigger_image_load(&self, path: PathBuf, first_frame_only: bool) {
        let tx = self.image_tx.clone();
        std::thread::spawn(move || {
            let result = image_loader::load_image_frames_scaled(
                &path,
                Some(MAX_BLOCK_DIMENSION as u32),
                first_frame_only,
            )
            .map(|loaded| (path, loaded, !first_frame_only));
            let _ = tx.send(result);
        });
    }

    /// Polls the image loading channel for completed tasks and integrates them into the application state.
    fn poll_image_rx(&mut self, ctx: &egui::Context) {
        if let Some(rx) = self.image_rx.take() {
            let mut got_any = false;
            let mut added_ids = Vec::new();

            // Calculate max height of EXISTING blocks before adding new ones
            let current_max_h = self.block_manager.max_block_height();

            while let Ok(result) = rx.try_recv() {
                match result {
                    Ok((path, loaded, is_full)) => {
                        let mut loaded = loaded;
                        let path_str = path.to_string_lossy().into_owned();

                        // Check if any block (including group children) needs this image
                        let needs_update = self
                            .blocks()
                            .iter()
                            .any(|b| b.needs_skeleton_for_path(&path_str, is_full));

                        if needs_update {
                            // Update all matching blocks recursively (including group children)
                            for block in self.blocks_mut() {
                                let (updated, _) = block.populate_skeletons_by_path(
                                    &path_str,
                                    &mut loaded.frames,
                                    loaded.has_animation,
                                    is_full,
                                );
                                if updated && is_full {
                                    // Note: We can't call mark_animation_used here for children
                                    // as they're not in the main block list. This is acceptable
                                    // since group children don't play animations independently.
                                }
                            }
                        } else {
                            // New block being added (not a skeleton restore)
                            match self.insert_loaded_image(ctx, path, loaded, is_full) {
                                Ok(id) => added_ids.push(id),
                                Err(err) => log::error!("{err}"),
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("Failed to load image: {err}");
                    }
                }
                got_any = true;
            }
            if got_any {
                if !added_ids.is_empty() && current_max_h > 0.0 {
                    for id in added_ids {
                        if let Some(block) = self.block_by_id_mut(id) {
                            let aspect_ratio = block.aspect_ratio;
                            block.set_preferred_size(vec2(
                                current_max_h * aspect_ratio,
                                current_max_h,
                            ));
                        }
                    }
                }
                self.reorder_and_reflow(None);
            }
            self.image_rx = Some(rx);
        }
    }

    fn data_to_block_skeleton(
        &mut self,
        ctx: &egui::Context,
        data: BlockData,
    ) -> Option<ImageBlock> {
        if data.is_group {
            let children: Vec<ImageBlock> = data
                .children
                .into_iter()
                .filter_map(|c| self.data_to_block_skeleton(ctx, c))
                .collect();

            let texture = ctx.load_texture(
                format!("group-texture-{}", self.block_manager.allocate_block_id()),
                egui::ColorImage::new([1, 1], COLOR_GROUP_PLACEHOLDER),
                egui::TextureOptions::LINEAR,
            );

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
            group.pos.position = pos2(data.position[0], data.position[1]);
            group.set_preferred_size(vec2(data.size[0], data.size[1]));
            group.chained = data.chained;
            Some(group)
        } else {
            if data.path.is_empty() {
                return None;
            }

            // Create a skeleton block with a neutral placeholder
            let texture = ctx.load_texture(
                format!("skeleton-texture-{}", data.id),
                egui::ColorImage::new([1, 1], Color32::from_gray(40)),
                egui::TextureOptions::LINEAR,
            );

            let mut block = ImageBlock::new(
                data.path.clone(),
                texture,
                Vec::new(),
                vec2(data.size[0], data.size[1]),
                false, // Will be updated when image loads
                false,
            );
            block.id = data.id;
            block.color = Color32::from_rgba_unmultiplied(
                data.color[0],
                data.color[1],
                data.color[2],
                data.color[3],
            );
            block.pos.position = pos2(data.position[0], data.position[1]);
            block.set_preferred_size(vec2(data.size[0], data.size[1]));
            block.chained = data.chained;
            block.counter = data.counter;
            // Note: we don't restore animation_enabled here - it will be set to false
            // and the user will need to click to load the full animation sequence on demand

            // Always load first frame only on session restore
            // Full sequence will be loaded on-demand when user clicks
            let path_buf = PathBuf::from(&data.path);
            self.trigger_image_load(path_buf, true);

            Some(block)
        }
    }

    fn create_block_from_loaded(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        loaded: image_loader::LoadedImage,
        is_full: bool,
    ) -> Result<ImageBlock, String> {
        if loaded.frames.is_empty() {
            return Err(format!(
                "{} did not contain renderable frames",
                path.display()
            ));
        }

        let texture_label = format!("block-texture-{}", self.block_manager.allocate_block_id());
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
            loaded.has_animation,
            is_full,
        );
        block.pos.position = pos2(CANVAS_PADDING, CANVAS_PADDING);
        Ok(block)
    }

    fn insert_loaded_image(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        loaded: image_loader::LoadedImage,
        is_full: bool,
    ) -> Result<Uuid, String> {
        let block = self.create_block_from_loaded(ctx, path, loaded, is_full)?;
        let id = block.id;
        self.block_manager.push(block);

        if is_full {
            self.block_manager.mark_animation_used(id);
        }

        Ok(id)
    }

    fn advance_animations(&mut self, dt: f32, ctx: &egui::Context) {
        let mut changed = false;
        let mut next_frame_in: Option<Duration> = None;
        for block in self.blocks_mut() {
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

    /// Recalculates the positions of all blocks to fit within the current canvas width.
    fn reflow_blocks(&mut self) {
        self.block_manager.reflow(self.working_inner_width);
    }

    fn can_chain(&self) -> bool {
        self.block_manager.can_chain()
    }

    fn clear_chain_group(&mut self) {
        self.block_manager.clear_chain_group();
        self.skip_chain_cancel = false;
    }

    fn toggle_chain_for_block(&mut self, index: usize) {
        self.block_manager.toggle_chain(index);
        self.skip_chain_cancel = true;
    }

    /// Combines all currently chained blocks into a single group block.
    fn box_group(&mut self, ctx: &egui::Context) -> Uuid {
        let new_id = self.block_manager.box_chained(ctx);
        self.reflow_blocks();
        new_id
    }

    fn unbox_group(&mut self, index: usize) {
        self.block_manager.unbox_group(index);
        self.reflow_blocks();
    }

    fn drop_block_into_box(&mut self, block_idx: usize, box_idx: usize) {
        self.block_manager.drop_into_group(block_idx, box_idx);
    }

    /// Attempts to re-box previously unboxed blocks. Returns true if action was taken.
    fn try_rebox_last_unboxed(&mut self, ctx: &egui::Context) -> bool {
        if self.last_unboxed_ids.is_empty() {
            return false;
        }

        let mut found_any = false;
        let last_unboxed_ids = self.last_unboxed_ids.clone();
        for block in self.blocks_mut() {
            if last_unboxed_ids.contains(&block.id) {
                block.chained = true;
                found_any = true;
            }
        }

        if found_any {
            self.last_boxed_id = Some(self.box_group(ctx));
            self.last_unboxed_ids.clear();
        }
        found_any
    }

    /// Attempts to unbox the last boxed group. Returns true if action was taken.
    fn try_unbox_last_boxed(&mut self) -> bool {
        let Some(last_id) = self.last_boxed_id else {
            return false;
        };

        let Some(idx) = self.block_index(last_id) else {
            return false;
        };

        self.last_unboxed_ids = self
            .block_manager
            .get_by_index(idx)
            .map(|b| b.group.children.iter().map(|c| c.id).collect())
            .unwrap_or_default();
        self.unbox_group(idx);
        self.last_boxed_id = None;
        true
    }

    /// Unboxes a single chained group block.
    fn unbox_single_chained_group(&mut self) -> bool {
        let chained_groups: Vec<_> = self
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.chained && b.group.is_group)
            .collect();

        if chained_groups.len() != 1 {
            return false;
        }

        let idx = chained_groups[0].0;
        self.last_unboxed_ids = self
            .block_manager
            .get_by_index(idx)
            .map(|b| b.group.children.iter().map(|c| c.id).collect())
            .unwrap_or_default();
        self.unbox_group(idx);
        self.last_boxed_id = None;
        true
    }

    fn toggle_compact_group(&mut self, ctx: &egui::Context) {
        let chained_count = self.block_manager.chained_count();

        // No blocks chained - try to restore previous state
        if chained_count == 0 {
            if self.try_rebox_last_unboxed(ctx) {
                return;
            }
            if self.try_unbox_last_boxed() {
                return;
            }
            return;
        }

        // Single chained group - unbox it
        if self.unbox_single_chained_group() {
            return;
        }

        // Multiple chained blocks - box them
        if chained_count > 0 {
            self.last_boxed_id = Some(self.box_group(ctx));
            self.last_unboxed_ids.clear();
        }
    }

    fn reorder_and_reflow(&mut self, leader_id: Option<Uuid>) {
        self.block_manager
            .reorder_and_reflow(leader_id, self.working_inner_width);
    }
}

impl eframe::App for MaBlocksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current_time = ctx.input(|i| i.time);
        if current_time - self.last_auto_save_time > 300.0 {
            if let Some(storage) = _frame.storage_mut() {
                self.save(storage);
                self.last_auto_save_time = current_time;
                log::info!("Auto-saved session");
            }
        }

        self.poll_image_rx(ctx);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.show_file_names = !self.show_file_names;
        }

        let dt = ctx.input(|i| i.unstable_dt).max(0.0);
        self.advance_animations(dt, ctx);
        self.block_manager.enforce_chain_constraints();

        self.render_toolbar(ctx);

        let (dropped_leader_id, should_reflow) = self.render_canvas(ctx);

        if let Some(leader_id) = dropped_leader_id {
            self.reorder_and_reflow(Some(leader_id));
        } else if should_reflow {
            self.reflow_blocks();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let session = Session {
            blocks: self
                .blocks()
                .iter()
                .map(|b| Self::block_to_data(b))
                .collect(),
            remembered_chains: self.serialize_remembered_chains(),
            last_unboxed_ids: self.last_unboxed_ids.clone(),
            last_boxed_id: self.last_boxed_id,
            zoom: self.zoom,
            show_file_names: self.show_file_names,
        };
        eframe::set_value(storage, eframe::APP_KEY, &session);
    }
}

/// Creates a toolbar button with consistent styling.
fn toolbar_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(icon).size(TOOLBAR_ICON_SIZE))
            .min_size(Vec2::splat(TOOLBAR_BUTTON_SIZE))
            .frame(false),
    )
    .on_hover_text(tooltip)
    .clicked()
}

impl MaBlocksApp {
    fn render_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::default()
                    .fill(COLOR_TOOLBAR_BG)
                    .inner_margin(0.0)
                    .outer_margin(0.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(TOOLBAR_START_SPACING);

                    if toolbar_button(ui, "💾", "Save Session") {
                        self.save_session();
                    }
                    if toolbar_button(ui, "📂", "Load Session") {
                        self.load_session(ctx);
                    }
                    if toolbar_button(ui, "🖼", "Add Image") {
                        self.load_images();
                    }
                    if toolbar_button(ui, "🔄", "Reset All Counters") {
                        self.reset_all_counters();
                    }
                    if toolbar_button(ui, "📦", "Compact/Unbox Group") {
                        self.toggle_compact_group(ctx);
                    }
                });
            });
    }

    fn render_canvas(&mut self, ctx: &egui::Context) -> (Option<Uuid>, bool) {
        let mut dropped_leader_id = None;
        let mut should_reflow = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            let input = InputSnapshot::from_ui(ui);

            self.handle_zoom_input(ui, &input);

            if input.secondary_released {
                self.resizing_state = None;
                should_reflow = true;
            }

            if let Some(curr_mouse_pos) = input.hover_pos {
                if let Some(state) = self.resizing_state.clone() {
                    let zoom = self.zoom;
                    handle_blocks_resizing(self.blocks_mut(), &state, curr_mouse_pos, zoom);
                }
            }

            egui::ScrollArea::both()
                .id_salt("main_canvas")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if input.middle_down {
                        ui.scroll_with_delta(input.pointer_delta);
                    }

                    let zoom = self.zoom;
                    let canvas_size = self.calculate_canvas_size(ui.available_height());
                    let (canvas_rect, _) = ui.allocate_exact_size(canvas_size, Sense::hover());

                    let mut canvas_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(canvas_rect)
                            .layout(egui::Layout::default()),
                    );
                    let canvas_origin = canvas_rect.min;

                    self.update_drop_target(&input, canvas_origin, zoom);

                    let mut hovered_box_to_render = None;
                    let mut dragging_blocks_to_render = Vec::new();

                    let is_any_dragging = self.block_manager.any_dragging();
                    let block_ids: Vec<_> = self.block_manager.block_ids().collect();

                    for id in block_ids {
                        let Some(index) = self.block_index(id) else {
                            continue;
                        };

                        let block = self.block_manager.get_by_index(index).unwrap();
                        let block_rect = Rect::from_min_size(
                            block.pos.position * zoom,
                            block.outer_size() * zoom,
                        )
                        .translate(canvas_origin.to_vec2());

                        let rects = block_control_rects(block_rect, zoom);
                        let block_id = canvas_ui.id().with(id);
                        let is_hovering_block =
                            input.hover_pos.is_some_and(|p| block_rect.contains(p));
                        let hover_state = BlockControlHover::from_mouse_pos(
                            input.hover_pos,
                            &rects,
                            block.group.is_group,
                        );
                        let any_button_hovered = hover_state.close_hovered
                            || hover_state.chain_hovered
                            || hover_state.counter_hovered;

                        let mut remove_single = false;
                        let mut remove_cascade = false;
                        if input.primary_clicked && hover_state.close_hovered {
                            if input.shift {
                                remove_cascade = true;
                            } else {
                                remove_single = true;
                            }
                            self.skip_chain_cancel = true;
                        }

                        if !remove_single
                            && !remove_cascade
                            && self.handle_block_interaction(
                                index,
                                &input,
                                canvas_origin,
                                zoom,
                                &hover_state,
                                is_hovering_block,
                            )
                        {
                            self.skip_chain_cancel = true;
                        }

                        let response =
                            canvas_ui.interact(block_rect, block_id, Sense::click_and_drag());

                        if !remove_single && !remove_cascade {
                            let (d_id, s_reflow, removed) = self.process_block_drag(
                                index,
                                &response,
                                canvas_origin,
                                zoom,
                                &input,
                                ui,
                            );
                            if let Some(id) = d_id {
                                dropped_leader_id = Some(id);
                            }
                            if s_reflow {
                                should_reflow = true;
                            }
                            if removed {
                                continue;
                            }
                        }

                        let block = self.block_manager.get_by_index(index).unwrap();
                        let show_controls =
                            is_hovering_block || block.pos.is_dragging || block.chained;

                        let is_hovered_box = Some(id) == self.hovered_box_id;
                        let should_render_on_top = is_hovered_box
                            || block.pos.is_dragging
                            || (is_any_dragging && block.chained);

                        let config = BlockRenderConfig {
                            zoom,
                            show_controls,
                            show_file_names: self.show_file_names,
                            can_chain: self.can_chain(),
                            is_drop_target: false,
                            hover_state,
                        };

                        if !should_render_on_top {
                            block.render(&mut canvas_ui, block_rect, config);
                        }

                        if is_hovered_box {
                            hovered_box_to_render = Some((id, block_rect, config));
                        } else if block.pos.is_dragging || (is_any_dragging && block.chained) {
                            dragging_blocks_to_render.push((id, block_rect, config));
                        }

                        if !remove_single
                            && !remove_cascade
                            && input.primary_clicked
                            && response.clicked()
                            && !any_button_hovered
                        {
                            self.handle_block_click(index, input.ctrl);
                        }

                        if remove_cascade {
                            self.block_manager.remove_cascade(index);
                            should_reflow = true;
                        } else if remove_single {
                            self.block_manager.remove_with_children(index);
                            should_reflow = true;
                        }
                    }

                    self.render_block_layer(
                        &mut canvas_ui,
                        &dragging_blocks_to_render,
                        hovered_box_to_render,
                    );

                    self.handle_canvas_background_click(&input, canvas_origin, zoom);
                });
        });

        (dropped_leader_id, should_reflow)
    }

    fn handle_zoom_input(&mut self, ui: &egui::Ui, input: &InputSnapshot) {
        if input.zoom_delta != 1.0 {
            self.zoom = (self.zoom * input.zoom_delta).clamp(0.1, 10.0);
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
    }

    /// Calculates the canvas size based on block positions and available viewport height.
    fn calculate_canvas_size(&self, available_height: f32) -> Vec2 {
        let zoom = self.zoom;
        let content_height = self
            .blocks()
            .iter()
            .map(|b| b.pos.position.y + b.outer_size().y)
            .fold(0.0, |a: f32, b| a.max(b));
        let min_height = available_height / zoom;
        let canvas_height = (content_height + CANVAS_PADDING).max(min_height);

        vec2(
            (self.working_inner_width + CANVAS_PADDING * 2.0) * zoom,
            canvas_height * zoom,
        )
    }

    /// Updates the hovered drop target when dragging a non-group block over groups.
    fn update_drop_target(&mut self, input: &InputSnapshot, canvas_origin: Pos2, zoom: f32) {
        self.hovered_box_id = None;

        let Some(dragging_idx) = self.blocks().iter().position(|b| b.pos.is_dragging) else {
            return;
        };

        let dragging_block = self.block_manager.get_by_index(dragging_idx).unwrap();
        if dragging_block.group.is_group {
            return;
        }
        let dragging_id = dragging_block.id;

        let Some(m_pos) = input.interact_pos else {
            return;
        };

        let world_mouse = (m_pos - canvas_origin) / zoom;
        if let Some(target_idx) = self
            .block_manager
            .find_group_at_pos(world_mouse.to_pos2(), dragging_id)
        {
            self.hovered_box_id = self.block_manager.get_by_index(target_idx).map(|b| b.id);
        }
    }

    /// Clears chain selection when clicking on empty canvas area.
    fn handle_canvas_background_click(
        &mut self,
        input: &InputSnapshot,
        canvas_origin: Pos2,
        zoom: f32,
    ) {
        if std::mem::take(&mut self.skip_chain_cancel) {
            return;
        }

        if !input.primary_clicked {
            return;
        }

        let Some(click_pos) = input.interact_pos else {
            return;
        };

        let world_click = (click_pos - canvas_origin) / zoom;
        let hit_block = self
            .blocks()
            .iter()
            .any(|b| b.rect().contains(world_click.to_pos2()));

        if !hit_block {
            self.clear_chain_group();
        }
    }

    /// Handles a click on a block - toggles chain or animation.
    fn handle_block_click(&mut self, index: usize, ctrl_held: bool) {
        if ctrl_held {
            self.toggle_chain_for_block(index);
            return;
        }

        let block = self.block_manager.get_by_index(index).unwrap();
        if !block.anim.has_animation {
            return;
        }

        if !block.is_full_sequence {
            let path = PathBuf::from(&block.path);
            self.trigger_image_load(path, false);
        } else {
            let block = self.block_manager.get_by_index_mut(index).unwrap();
            block.toggle_animation();
            if block.anim.animation_enabled {
                let id = block.id;
                self.block_manager.mark_animation_used(id);
            }
        }
    }

    fn handle_block_interaction(
        &mut self,
        index: usize,
        input: &InputSnapshot,
        canvas_origin: Pos2,
        zoom: f32,
        hover_state: &BlockControlHover,
        is_hovering_block: bool,
    ) -> bool {
        let mut skip_chain_cancel = false;
        let any_button_hovered =
            hover_state.close_hovered || hover_state.chain_hovered || hover_state.counter_hovered;

        if input.primary_clicked {
            if hover_state.chain_hovered {
                self.toggle_chain_for_block(index);
            } else if hover_state.counter_hovered {
                self.block_manager.get_by_index_mut(index).unwrap().counter += 1;
                skip_chain_cancel = true;
            }
        }

        if input.secondary_clicked && hover_state.counter_hovered {
            let block = self.block_manager.get_by_index_mut(index).unwrap();
            block.counter = (block.counter - 1).max(0);
            skip_chain_cancel = true;
        }

        if input.secondary_pressed && is_hovering_block && !any_button_hovered {
            if let Some(m_pos) = input.hover_pos {
                let block = self.block_manager.get_by_index(index).unwrap();
                let world_mouse = (m_pos - canvas_origin) / zoom;
                let center = block.rect().center();
                let handle = match (world_mouse.x < center.x, world_mouse.y < center.y) {
                    (true, true) => ResizeHandle::TopLeft,
                    (false, true) => ResizeHandle::TopRight,
                    (true, false) => ResizeHandle::BottomLeft,
                    (false, false) => ResizeHandle::BottomRight,
                };
                self.resizing_state = Some(InteractionState {
                    id: block.id,
                    handle,
                    initial_mouse_pos: m_pos,
                    initial_block_rect: block.rect(),
                });
            }
        }
        skip_chain_cancel
    }

    fn process_block_drag(
        &mut self,
        index: usize,
        response: &egui::Response,
        canvas_origin: Pos2,
        zoom: f32,
        input: &InputSnapshot,
        ui: &egui::Ui,
    ) -> (Option<Uuid>, bool, bool) {
        let mut dropped_leader_id = None;
        let mut should_reflow = false;

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let block = self.block_manager.get_by_index_mut(index).unwrap();
                block.pos.drag_offset = (pointer - canvas_origin) / zoom
                    - vec2(block.pos.position.x, block.pos.position.y);
                block.pos.is_dragging = true;
            }
        }

        let block = self.block_manager.get_by_index(index).unwrap();
        let is_dragging = block.pos.is_dragging;

        if is_dragging && response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
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

                let block = self.block_manager.get_by_index(index).unwrap();
                let old_pos = block.pos.position;
                let drag_offset = block.drag_offset();
                let is_chained = block.chained;
                let leader_id = block.id;

                let current_canvas_origin = canvas_origin + vec2(0.0, scroll_delta);
                let new_pos = (pointer - current_canvas_origin) / zoom - drag_offset;
                let delta = pos2(new_pos.x, new_pos.y) - old_pos;

                self.block_manager
                    .get_by_index_mut(index)
                    .unwrap()
                    .pos
                    .position = pos2(new_pos.x, new_pos.y);

                if is_chained {
                    for other in self.blocks_mut() {
                        if other.chained && other.id != leader_id {
                            other.pos.position += delta;
                        }
                    }
                }
            }
        }

        let block = self.block_manager.get_by_index(index).unwrap();
        if block.pos.is_dragging && response.drag_stopped() {
            let block_id = block.id;
            let is_group = block.group.is_group;
            self.block_manager
                .get_by_index_mut(index)
                .unwrap()
                .pos
                .is_dragging = false;

            let mut dropped_into_box = false;
            if !is_group {
                if let Some(m_pos) = input.interact_pos {
                    let world_mouse = (m_pos - canvas_origin) / zoom;
                    let target_idx = self
                        .block_manager
                        .find_group_at_pos(world_mouse.to_pos2(), block_id);

                    if let Some(t_idx) = target_idx {
                        self.drop_block_into_box(index, t_idx);
                        dropped_into_box = true;
                        should_reflow = true;
                    }
                }
            }

            if dropped_into_box {
                return (None, true, true);
            }

            dropped_leader_id = Some(block_id);
        }

        (dropped_leader_id, should_reflow, false)
    }

    fn render_block_layer(
        &self,
        ui: &mut egui::Ui,
        dragging_blocks: &[(Uuid, Rect, BlockRenderConfig)],
        hovered_box: Option<(Uuid, Rect, BlockRenderConfig)>,
    ) {
        for (id, rect, config) in dragging_blocks {
            if let Some(block) = self.block_by_id(*id) {
                block.render(ui, *rect, *config);
            }
        }
        if let Some((id, rect, mut config)) = hovered_box {
            if let Some(block) = self.block_by_id(id) {
                config.is_drop_target = true;
                block.render(ui, rect, config);
            }
        }
    }

    /// Saves the current session state, including blocks and chains, to a JSON file.
    fn save_session(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Session", &["json"])
            .set_file_name("ma_blocks_session.json");

        if let Some(ref p) = self.paths {
            dialog = dialog.set_directory(&p.sessions);
        }

        if let Some(path) = dialog.save_file() {
            let session = Session {
                blocks: self
                    .blocks()
                    .iter()
                    .map(|b| Self::block_to_data(b))
                    .collect(),
                remembered_chains: self.serialize_remembered_chains(),
                last_unboxed_ids: self.last_unboxed_ids.clone(),
                last_boxed_id: self.last_boxed_id,
                zoom: self.zoom,
                show_file_names: self.show_file_names,
            };

            if let Ok(file) = std::fs::File::create(&path) {
                let _ = serde_json::to_writer_pretty(file, &session);
                self.session_file = Some(path);
            }
        }
    }

    fn block_to_data(b: &ImageBlock) -> BlockData {
        BlockData {
            id: b.id,
            position: [b.pos.position.x, b.pos.position.y],
            size: [b.image_size.x, b.image_size.y],
            path: b.path.clone(),
            chained: b.chained,
            animation_enabled: b.anim.animation_enabled,
            counter: b.counter,
            is_group: b.group.is_group,
            group_name: b.group.group_name.clone(),
            color: b.color.to_array(),
            children: b
                .group
                .children
                .iter()
                .map(|c| Self::block_to_data(c))
                .collect(),
        }
    }

    fn parse_remembered_chains(chains: Vec<Vec<String>>) -> Vec<ChainedIds> {
        chains
            .into_iter()
            .map(|chain| {
                chain
                    .into_iter()
                    .filter_map(|s| Uuid::parse_str(&s).ok())
                    .collect()
            })
            .filter(|chain: &ChainedIds| chain.len() >= 2)
            .collect()
    }

    fn serialize_remembered_chains(&self) -> Vec<Vec<String>> {
        self.block_manager
            .remembered_chains()
            .iter()
            .map(|chain| chain.iter().map(|id| id.to_string()).collect())
            .collect()
    }

    /// Loads a previously saved session state from a JSON file.
    fn load_session(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().add_filter("Session", &["json"]);

        if let Some(ref p) = self.paths {
            dialog = dialog.set_directory(&p.sessions);
        }

        if let Some(path) = dialog.pick_file() {
            if let Ok(file) = std::fs::File::open(&path) {
                if let Ok(session) =
                    serde_json::from_reader::<_, Session>(std::io::BufReader::new(file))
                {
                    self.apply_session_data(ctx, session);
                    self.session_file = Some(path);
                }
            }
        }
    }

    fn reset_all_counters(&mut self) {
        self.block_manager.reset_all_counters();
    }
}

fn scaled_size(original: Vec2) -> Vec2 {
    let scale = (MAX_BLOCK_DIMENSION / original.x.max(1.0))
        .min(MAX_BLOCK_DIMENSION / original.y.max(1.0))
        .min(1.0);
    original * scale
}
