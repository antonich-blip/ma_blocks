//! Block management abstraction for handling block CRUD, chaining, and grouping operations.
//!
//! This module consolidates all block-related operations that were previously scattered
//! throughout MaBlocksApp, providing a cleaner separation of concerns.

use crate::block::ImageBlock;
use crate::constants::{
    ALIGN_SPACING, BLOCK_PADDING, CANVAS_PADDING, COLOR_GROUP_PLACEHOLDER, MAX_CACHED_ANIMATIONS,
    MIN_CANVAS_INNER_WIDTH, ROW_QUANTIZATION_HEIGHT,
};
use eframe::egui::{self, pos2, vec2, Pos2};
use std::collections::HashSet;
use uuid::Uuid;

/// A set of block IDs representing a chain group.
pub type ChainedIds = HashSet<Uuid>;

/// Manages the collection of blocks with operations for lookup, chaining, grouping, and layout.
pub struct BlockManager {
    blocks: Vec<ImageBlock>,
    next_block_id: usize,
    remembered_chains: Vec<ChainedIds>,
    animation_access_order: Vec<Uuid>,
}

#[allow(dead_code)]
impl BlockManager {
    /// Creates a new empty BlockManager.
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            next_block_id: 0,
            remembered_chains: Vec::new(),
            animation_access_order: Vec::new(),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Block Collection Access
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns a slice of all blocks.
    pub fn blocks(&self) -> &[ImageBlock] {
        &self.blocks
    }

    /// Returns a mutable slice of all blocks.
    pub fn blocks_mut(&mut self) -> &mut [ImageBlock] {
        &mut self.blocks
    }

    /// Returns the number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns true if there are no blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Returns an iterator over all block IDs.
    pub fn block_ids(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.blocks.iter().map(|b| b.id)
    }

    /// Returns a reference to the next block ID counter.
    pub fn next_block_id(&self) -> usize {
        self.next_block_id
    }

    /// Increments and returns the next block ID.
    pub fn allocate_block_id(&mut self) -> usize {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Block Lookup
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns the index of a block by its ID, or None if not found.
    pub fn index_of(&self, id: Uuid) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    /// Returns an immutable reference to a block by its ID.
    pub fn get(&self, id: Uuid) -> Option<&ImageBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Returns a mutable reference to a block by its ID.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut ImageBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    /// Returns a reference to a block by its index.
    pub fn get_by_index(&self, index: usize) -> Option<&ImageBlock> {
        self.blocks.get(index)
    }

    /// Returns a mutable reference to a block by its index.
    pub fn get_by_index_mut(&mut self, index: usize) -> Option<&mut ImageBlock> {
        self.blocks.get_mut(index)
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Block CRUD Operations
    // ─────────────────────────────────────────────────────────────────────────────

    /// Adds a block to the collection.
    pub fn push(&mut self, block: ImageBlock) {
        self.blocks.push(block);
    }

    /// Inserts a block at the specified index.
    pub fn insert(&mut self, index: usize, block: ImageBlock) {
        self.blocks.insert(index, block);
    }

    /// Removes and returns a block at the specified index.
    pub fn remove(&mut self, index: usize) -> ImageBlock {
        let block = self.blocks.remove(index);
        self.animation_access_order.retain(|&x| x != block.id);
        block
    }

    /// Removes a block by its ID. Returns the removed block if found.
    pub fn remove_by_id(&mut self, id: Uuid) -> Option<ImageBlock> {
        self.index_of(id).map(|idx| self.remove(idx))
    }

    /// Removes a block and all its children (for group blocks).
    /// For non-group blocks, just removes the single block.
    /// Returns the IDs of all removed blocks.
    pub fn remove_with_children(&mut self, index: usize) -> Vec<Uuid> {
        let block = self.blocks.remove(index);
        let mut removed_ids = vec![block.id];

        // Collect IDs of all children recursively
        fn collect_child_ids(block: &ImageBlock, ids: &mut Vec<Uuid>) {
            for child in &block.group.children {
                ids.push(child.id);
                collect_child_ids(child, ids);
            }
        }
        collect_child_ids(&block, &mut removed_ids);

        // Clean up animation access order for all removed blocks
        for id in &removed_ids {
            self.animation_access_order.retain(|&x| x != *id);
        }

        removed_ids
    }

    /// Cascade remove: removes all chained blocks AND their children.
    /// If the block is not chained, just removes it with its children.
    /// Returns the IDs of all removed blocks.
    pub fn remove_cascade(&mut self, index: usize) -> Vec<Uuid> {
        let block = &self.blocks[index];

        if !block.chained {
            // Not chained, just remove with children
            return self.remove_with_children(index);
        }

        // Collect all chained block indices
        let mut chained_indices = self.chained_indices();
        chained_indices.sort_by(|a, b| b.cmp(a)); // Reverse order for safe removal

        let mut all_removed_ids = Vec::new();

        // Remove each chained block with its children
        for idx in chained_indices {
            let block = self.blocks.remove(idx);
            all_removed_ids.push(block.id);

            // Collect child IDs recursively
            fn collect_child_ids(block: &ImageBlock, ids: &mut Vec<Uuid>) {
                for child in &block.group.children {
                    ids.push(child.id);
                    collect_child_ids(child, ids);
                }
            }
            collect_child_ids(&block, &mut all_removed_ids);
        }

        // Clean up remembered chains that contain any removed blocks
        let removed_set: HashSet<Uuid> = all_removed_ids.iter().copied().collect();
        self.remembered_chains
            .retain(|chain| chain.is_disjoint(&removed_set));

        // Clean up animation access order
        for id in &all_removed_ids {
            self.animation_access_order.retain(|&x| x != *id);
        }

        all_removed_ids
    }

    /// Clears all blocks.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.animation_access_order.clear();
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Animation Cache Management
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns a reference to the animation access order for LRU tracking.
    pub fn animation_access_order(&self) -> &[Uuid] {
        &self.animation_access_order
    }

    /// Marks an animation as recently used (for LRU cache management).
    pub fn mark_animation_used(&mut self, id: Uuid) {
        // Remove if exists and push to back (most recent)
        self.animation_access_order.retain(|&x| x != id);
        self.animation_access_order.push(id);

        // If we exceed cache size, purge the oldest (front)
        if self.animation_access_order.len() > MAX_CACHED_ANIMATIONS {
            let to_purge_id = self.animation_access_order.remove(0);
            self.purge_animation_frames(to_purge_id);
        }
    }

    /// Purges animation frames for a block, keeping only the first frame.
    fn purge_animation_frames(&mut self, id: Uuid) {
        if let Some(block) = self.get_mut(id) {
            if block.is_full_sequence && block.anim.frames.len() > 1 {
                block.anim.frames.truncate(1);
                block.is_full_sequence = false;
                block.stop_animation();
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Chaining Operations
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns true if chaining is allowed (i.e., there are blocks).
    pub fn can_chain(&self) -> bool {
        !self.blocks.is_empty()
    }

    /// Returns the number of currently chained blocks.
    pub fn chained_count(&self) -> usize {
        self.blocks.iter().filter(|b| b.chained).count()
    }

    /// Returns the IDs of all currently chained blocks.
    pub fn chained_ids(&self) -> ChainedIds {
        self.blocks
            .iter()
            .filter(|b| b.chained)
            .map(|b| b.id)
            .collect()
    }

    /// Returns indices of all currently chained blocks.
    pub fn chained_indices(&self) -> Vec<usize> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.chained)
            .map(|(i, _)| i)
            .collect()
    }

    /// Returns a reference to the remembered chains.
    pub fn remembered_chains(&self) -> &[ChainedIds] {
        &self.remembered_chains
    }

    /// Sets the remembered chains (used when loading sessions).
    pub fn set_remembered_chains(&mut self, chains: Vec<ChainedIds>) {
        self.remembered_chains = chains;
    }

    /// Clears the current chain group and remembers it if it has 2+ members.
    pub fn clear_chain_group(&mut self) {
        let chained_ids = self.chained_ids();

        if chained_ids.len() >= 2 {
            self.remembered_chains
                .retain(|chain| chain.is_disjoint(&chained_ids));
            self.remembered_chains.push(chained_ids);
        }

        for block in &mut self.blocks {
            block.chained = false;
        }
    }

    /// Toggles chain state for a block at the given index.
    /// If the block wasn't chained and belongs to a remembered chain, restores the entire chain.
    pub fn toggle_chain(&mut self, index: usize) {
        if !self.can_chain() && !self.blocks[index].group.is_group {
            return;
        }

        let block_id = self.blocks[index].id;
        let was_chained = self.blocks[index].chained;

        if !was_chained {
            // Check if this block belongs to a remembered chain
            let remembered_chain = self
                .remembered_chains
                .iter()
                .find(|chain| chain.contains(&block_id))
                .cloned();

            if let Some(chain_ids) = remembered_chain {
                // Restore the entire remembered chain
                for block in &mut self.blocks {
                    if chain_ids.contains(&block.id) {
                        block.chained = true;
                    }
                }
            } else {
                self.blocks[index].chained = true;
            }
        } else {
            self.blocks[index].chained = false;
        }
    }

    /// Enforces chain constraints (clears chain if no blocks exist).
    pub fn enforce_chain_constraints(&mut self) {
        if self.blocks.is_empty() {
            self.clear_chain_group();
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Group Operations
    // ─────────────────────────────────────────────────────────────────────────────

    /// Creates a group from all currently chained blocks. Returns the new group's ID.
    pub fn box_chained(&mut self, ctx: &egui::Context) -> Uuid {
        let mut chained_indices = self.chained_indices();

        if chained_indices.is_empty() {
            return Uuid::nil();
        }

        // Sort in reverse order to remove from the end first
        chained_indices.sort_by(|a, b| b.cmp(a));

        let mut children = Vec::new();
        let mut min_pos = pos2(f32::MAX, f32::MAX);
        for &idx in &chained_indices {
            let block = self.blocks.remove(idx);
            min_pos.x = min_pos.x.min(block.pos.position.x);
            min_pos.y = min_pos.y.min(block.pos.position.y);
            children.push(block);
        }
        children.reverse();

        let texture = ctx.load_texture(
            format!("group-texture-{}", self.next_block_id),
            egui::ColorImage::new([1, 1], COLOR_GROUP_PLACEHOLDER),
            egui::TextureOptions::LINEAR,
        );
        self.next_block_id += 1;

        let representative_texture = children.first().map(|c| c.texture.clone());

        let mut group_block =
            ImageBlock::new_group(String::new(), children, texture, representative_texture);
        group_block.update_group_name();
        group_block.pos.position = min_pos;
        let new_id = group_block.id;
        self.blocks.insert(0, group_block);
        new_id
    }

    /// Unboxes a group at the specified index, inserting its children back into the block list.
    /// Returns the IDs of the unboxed children.
    pub fn unbox_group(&mut self, index: usize) -> Vec<Uuid> {
        let group = self.blocks.remove(index);
        let mut unboxed_ids = Vec::new();

        if group.group.is_group {
            let insert_idx = self
                .blocks
                .iter()
                .position(|b| !b.group.is_group)
                .unwrap_or(self.blocks.len());

            for (i, mut child) in group.group.children.into_iter().enumerate() {
                unboxed_ids.push(child.id);
                child.chained = false;
                self.blocks.insert(insert_idx + i, child);
            }
        }

        unboxed_ids
    }

    /// Drops a block (and optionally all chained blocks) into a group.
    pub fn drop_into_group(&mut self, block_idx: usize, group_idx: usize) {
        let is_chained = self.blocks[block_idx].chained;
        let group_id = self.blocks[group_idx].id;

        if is_chained {
            let chained_ids: Vec<Uuid> = self.chained_ids().into_iter().collect();
            for id in chained_ids {
                if let Some(b_idx) = self.index_of(id) {
                    if let Some(g_idx) = self.index_of(group_id) {
                        self.move_single_into_group(b_idx, g_idx);
                    }
                }
            }
        } else {
            self.move_single_into_group(block_idx, group_idx);
        }
    }

    /// Moves a single block into a group.
    fn move_single_into_group(&mut self, block_idx: usize, group_idx: usize) {
        let mut block = self.blocks.remove(block_idx);
        block.pos.is_dragging = false;
        block.chained = false;

        let target_idx = if group_idx > block_idx {
            group_idx - 1
        } else {
            group_idx
        };
        let group = &mut self.blocks[target_idx];

        if group.group.representative_texture.is_none() {
            group.group.representative_texture = Some(block.texture.clone());
        }

        group.group.children.push(block);
        group.update_group_name();
    }

    /// Finds a group at the given position, excluding the specified block ID.
    pub fn find_group_at_pos(&self, pos: Pos2, exclude_id: Uuid) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.id != exclude_id && b.group.is_group && b.rect().contains(pos))
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layout Operations
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns the maximum height among non-group blocks.
    pub fn max_block_height(&self) -> f32 {
        self.blocks
            .iter()
            .filter(|b| !b.group.is_group)
            .map(|b| b.preferred_image_size.y)
            .fold(0.0, |a, b| a.max(b))
    }

    /// Recalculates block positions to fit within the given inner width.
    pub fn reflow(&mut self, inner_width: f32) {
        let inner_width = inner_width.max(MIN_CANVAS_INNER_WIDTH);
        let row_limit = CANVAS_PADDING + inner_width;
        let max_image_width = (inner_width - BLOCK_PADDING * 2.0).max(1.0);

        for block in &mut self.blocks {
            block.reset_to_preferred_size();
            block.constrain_to_width(max_image_width);
        }

        let mut cursor = vec2(CANVAS_PADDING, CANVAS_PADDING);
        let mut row_height = 0.0;
        let mut prev_is_group = None;

        for block in &mut self.blocks {
            // Start new row when switching between groups and non-groups
            if let Some(prev) = prev_is_group {
                if prev != block.group.is_group && cursor.x > CANVAS_PADDING {
                    cursor.x = CANVAS_PADDING;
                    cursor.y += row_height + ALIGN_SPACING;
                    row_height = 0.0;
                }
            }
            prev_is_group = Some(block.group.is_group);

            let size = block.outer_size();
            if cursor.x + size.x > row_limit {
                cursor.x = CANVAS_PADDING;
                cursor.y += row_height + ALIGN_SPACING;
                row_height = 0.0;
            }

            block.pos.position = pos2(cursor.x, cursor.y);
            cursor.x += size.x + ALIGN_SPACING;
            row_height = row_height.max(size.y);
        }
    }

    /// Reorders blocks based on the leader's position and reflows.
    pub fn reorder_and_reflow(&mut self, leader_id: Option<Uuid>, inner_width: f32) {
        if let Some(leader_id) = leader_id {
            let is_leader_chained = self.get(leader_id).map(|b| b.chained).unwrap_or(false);

            let mut moved_group = Vec::new();
            let mut remaining = Vec::new();

            let leader_exists = self.index_of(leader_id).is_some();
            if !leader_exists {
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
                self.reflow(inner_width);
                return;
            }

            remaining.sort_by(|a, b| a.cmp_layout(b));

            let leader_pos = moved_group
                .iter()
                .find(|b| b.id == leader_id)
                .unwrap()
                .pos
                .position;
            let is_leader_group = moved_group[0].group.is_group;

            let group_boundary = remaining
                .iter()
                .position(|b| !b.group.is_group)
                .unwrap_or(remaining.len());

            let insert_idx =
                Self::find_insert_index(&remaining, leader_pos, is_leader_group, group_boundary);

            self.blocks = remaining;
            for (i, block) in moved_group.into_iter().enumerate() {
                self.blocks.insert(insert_idx + i, block);
            }
        } else {
            self.blocks.sort_by(|a, b| a.cmp_layout(b));
        }
        self.reflow(inner_width);
    }

    /// Finds the insertion index for a block based on its position.
    fn find_insert_index(
        remaining: &[ImageBlock],
        leader_pos: Pos2,
        is_leader_group: bool,
        group_boundary: usize,
    ) -> usize {
        if is_leader_group {
            for (i, b) in remaining[..group_boundary].iter().enumerate() {
                if Self::should_insert_before(leader_pos, b.pos.position) {
                    return i;
                }
            }
            group_boundary
        } else {
            for (i, b) in remaining[group_boundary..].iter().enumerate() {
                if Self::should_insert_before(leader_pos, b.pos.position) {
                    return group_boundary + i;
                }
            }
            remaining.len()
        }
    }

    /// Determines if a block should be inserted before another based on Y-quantized position.
    fn should_insert_before(leader_pos: Pos2, block_pos: Pos2) -> bool {
        let leader_y_q = (leader_pos.y / ROW_QUANTIZATION_HEIGHT) as i32;
        let block_y_q = (block_pos.y / ROW_QUANTIZATION_HEIGHT) as i32;

        leader_y_q < block_y_q || (leader_y_q == block_y_q && leader_pos.x < block_pos.x)
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Utility Operations
    // ─────────────────────────────────────────────────────────────────────────────

    /// Resets counters for all blocks recursively.
    pub fn reset_all_counters(&mut self) {
        for block in &mut self.blocks {
            block.reset_counters_recursive();
        }
    }

    /// Returns true if any block is currently being dragged.
    pub fn any_dragging(&self) -> bool {
        self.blocks.iter().any(|b| b.pos.is_dragging)
    }
}

impl Default for BlockManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_insert_before() {
        // Same row, leader is to the left
        assert!(BlockManager::should_insert_before(
            pos2(100.0, 50.0),
            pos2(200.0, 50.0)
        ));
        // Same row, leader is to the right
        assert!(!BlockManager::should_insert_before(
            pos2(200.0, 50.0),
            pos2(100.0, 50.0)
        ));
        // Leader is above (different quantized row)
        assert!(BlockManager::should_insert_before(
            pos2(100.0, 50.0),
            pos2(100.0, 250.0)
        ));
        // Leader is below
        assert!(!BlockManager::should_insert_before(
            pos2(100.0, 250.0),
            pos2(100.0, 50.0)
        ));
    }
}
