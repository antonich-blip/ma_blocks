//! Centralized constants for UI sizing, spacing, and colors.
//!
//! This module consolidates all magic numbers and colors used throughout the application
//! to improve maintainability and provide semantic meaning to values.

use eframe::egui::Color32;

// =============================================================================
// BLOCK LAYOUT CONSTANTS
// =============================================================================

/// Internal padding within a block to provide visual separation between the image and its border.
pub const BLOCK_PADDING: f32 = 4.0;

/// Minimum size for a block's dimension to ensure it remains interactable and visible.
pub const MIN_BLOCK_SIZE: f32 = 50.0;

/// The vertical height used to quantize block positions into rows for sorting and alignment purposes.
pub const ROW_QUANTIZATION_HEIGHT: f32 = 100.0;

/// Default size for new group blocks.
pub const DEFAULT_GROUP_SIZE: f32 = 160.0;

// =============================================================================
// CANVAS CONSTANTS
// =============================================================================

/// Spacing between the canvas edges and the blocks.
pub const CANVAS_PADDING: f32 = 32.0;

/// The target width for the inner canvas content, used as a reference for layout.
pub const CANVAS_WORKING_WIDTH: f32 = 1400.0;

/// Horizontal and vertical spacing between blocks to maintain a clean grid-like appearance.
pub const ALIGN_SPACING: f32 = 24.0;

/// Maximum dimension (width or height) for any single block to prevent oversized images.
pub const MAX_BLOCK_DIMENSION: f32 = 420.0;

/// Minimum width the canvas can take, ensuring at least one block plus padding can be displayed.
pub const MIN_CANVAS_INNER_WIDTH: f32 = MIN_BLOCK_SIZE + BLOCK_PADDING * 2.0;

/// Maximum number of animations to keep in memory simultaneously.
pub const MAX_CACHED_ANIMATIONS: usize = 20;

// =============================================================================
// WINDOW CONSTANTS
// =============================================================================

/// Initial window width when the application starts.
pub const INITIAL_WINDOW_WIDTH: f32 = 800.0;

/// Initial window height when the application starts.
pub const INITIAL_WINDOW_HEIGHT: f32 = 600.0;

// =============================================================================
// CONTROL BUTTON CONSTANTS
// =============================================================================

/// Base size for control buttons (close, chain, counter) before zoom scaling.
pub const BUTTON_BASE_SIZE: f32 = 16.0;

/// Spacing between control buttons before zoom scaling.
pub const BUTTON_SPACING: f32 = 4.0;

/// Multiplier for hit area of buttons (makes them easier to click).
pub const BUTTON_HIT_AREA_MULTIPLIER: f32 = 1.2;

/// Font size for control button icons (x, o, #).
pub const BUTTON_ICON_FONT_SIZE: f32 = 12.0;

// =============================================================================
// BLOCK RENDERING CONSTANTS
// =============================================================================

/// Corner radius for block rectangles.
pub const BLOCK_CORNER_RADIUS: f32 = 6.0;

/// Corner radius for folder preview inside groups.
pub const FOLDER_CORNER_RADIUS: f32 = 2.0;

/// Corner radius for folder tab.
pub const FOLDER_TAB_CORNER_RADIUS: f32 = 1.0;

/// Scale factor for folder preview relative to image rect.
pub const FOLDER_PREVIEW_SCALE: f32 = 0.9;

/// Scale factor for representative texture inside groups.
pub const GROUP_TEXTURE_SCALE: f32 = 0.8;

/// Width ratio for folder tab relative to folder width.
pub const FOLDER_TAB_WIDTH_RATIO: f32 = 0.4;

/// Height of folder tab above the folder body.
pub const FOLDER_TAB_HEIGHT: f32 = 5.0;

// =============================================================================
// COUNTER BADGE CONSTANTS
// =============================================================================

/// Radius of the counter badge circle.
pub const COUNTER_BADGE_RADIUS: f32 = 15.0;

/// Offset from block corner for counter badge positioning.
pub const COUNTER_BADGE_OFFSET: f32 = 5.0;

/// Font size for counter badge number.
pub const COUNTER_FONT_SIZE: f32 = 20.0;

// =============================================================================
// FILE NAME LABEL CONSTANTS
// =============================================================================

/// Font size for file name labels.
pub const LABEL_FONT_SIZE: f32 = 12.0;

/// Padding around file name label text.
pub const LABEL_PADDING: f32 = 4.0;

/// Expansion for label background rectangle.
pub const LABEL_BG_EXPANSION: f32 = 2.0;

// =============================================================================
// TOOLBAR CONSTANTS
// =============================================================================

/// Spacing at the start of the toolbar.
pub const TOOLBAR_START_SPACING: f32 = 8.0;

/// Size of toolbar button icons.
pub const TOOLBAR_ICON_SIZE: f32 = 24.0;

/// Minimum size for toolbar buttons.
pub const TOOLBAR_BUTTON_SIZE: f32 = 32.0;

// =============================================================================
// COLORS - BLOCK BACKGROUNDS
// =============================================================================

/// Background color for chained or drop-target groups.
pub const COLOR_CHAINED_GROUP_BG: Color32 = Color32::from_rgb(100, 100, 150);

/// Background color for normal (non-chained) groups.
pub const COLOR_NORMAL_GROUP_BG: Color32 = Color32::from_rgb(60, 60, 60);

/// Placeholder color for group textures (used when creating group texture).
pub const COLOR_GROUP_PLACEHOLDER: Color32 = Color32::from_rgb(200, 180, 100);

// =============================================================================
// COLORS - CONTROL BUTTONS
// =============================================================================

/// Close button color when hovered.
pub const COLOR_CLOSE_BUTTON_HOVER: Color32 = Color32::from_rgb(255, 100, 100);

/// Close button color in normal state.
pub const COLOR_CLOSE_BUTTON: Color32 = Color32::RED;

/// Chain button color when block is chained.
pub const COLOR_CHAIN_ACTIVE: Color32 = Color32::GREEN;

/// Chain button color when chaining is disabled.
pub const COLOR_CHAIN_DISABLED: Color32 = Color32::from_rgb(80, 80, 80);

/// Chain button color when hovered.
pub const COLOR_CHAIN_HOVER: Color32 = Color32::LIGHT_GRAY;

/// Chain button color in normal state.
pub const COLOR_CHAIN_NORMAL: Color32 = Color32::GRAY;

/// Counter button color when hovered.
pub const COLOR_COUNTER_BUTTON_HOVER: Color32 = Color32::from_rgb(0, 150, 0);

/// Counter button color in normal state.
pub const COLOR_COUNTER_BUTTON: Color32 = Color32::from_rgb(0, 100, 0);

/// Counter badge background color (semi-transparent).
pub const COLOR_COUNTER_BADGE: Color32 = Color32::from_rgba_premultiplied(0, 100, 0, 170);

// =============================================================================
// COLORS - TEXT AND LABELS
// =============================================================================

/// Background color for file name labels (semi-transparent black).
pub const COLOR_LABEL_BG_ALPHA: u8 = 180;

// =============================================================================
// COLORS - TOOLBAR
// =============================================================================

/// Background color for the toolbar.
pub const COLOR_TOOLBAR_BG: Color32 = Color32::from_rgb(30, 30, 30);

// =============================================================================
// COLOR GENERATION CONSTANTS
// =============================================================================

/// Minimum saturation for UUID-generated colors.
pub const UUID_COLOR_SATURATION_MIN: f32 = 0.6;

/// Saturation range for UUID-generated colors.
pub const UUID_COLOR_SATURATION_RANGE: f32 = 0.4;

/// Minimum lightness for UUID-generated colors.
pub const UUID_COLOR_LIGHTNESS_MIN: f32 = 0.5;

/// Lightness range for UUID-generated colors.
pub const UUID_COLOR_LIGHTNESS_RANGE: f32 = 0.2;
