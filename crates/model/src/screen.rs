//! One output and what it holds: a layout, its columns, floating windows.

use crate::geometry::{Insets, Point, Rect, Size};
use crate::ids::{OutputId, WindowId};
use crate::layout::{LayoutKind, Split};
use crate::metrics::Metrics;
use crate::output::{OutputInfo, OutputRole};
use crate::proportions::ScreenProportions;
use crate::stack::TabStack;

/// A window that is not in a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatingWindow {
    pub id: WindowId,
    /// Whole decorated window, relative to the output.
    pub rect: Rect,
}

/// State of one output.
#[derive(Debug, Clone)]
pub struct Screen {
    info: OutputInfo,
    layout: LayoutKind,
    /// The widths the user chose, by layout and number of columns.
    splits: ScreenProportions,
    /// The columns shown, left to right: never more than the layout allows,
    /// never an empty one (a column of minimized windows still counts).
    slots: Vec<TabStack>,
    /// Bottom to top.
    floating: Vec<FloatingWindow>,
    /// Windows that take the whole screen, over its slots: a pile of tabs
    /// of their own.
    maximized: TabStack,
    /// The maximized windows are shown and hide the slots; otherwise the
    /// slots show.
    covered: bool,
    anchored: Option<WindowId>,
}

impl Screen {
    pub fn new(info: OutputInfo) -> Self {
        Self {
            layout: LayoutKind::default_for(info.role),
            info,
            splits: ScreenProportions::new(),
            slots: Vec::new(),
            floating: Vec::new(),
            maximized: TabStack::default(),
            covered: false,
            anchored: None,
        }
    }

    pub fn info(&self) -> &OutputInfo {
        &self.info
    }

    pub fn id(&self) -> OutputId {
        self.info.id
    }

    pub fn role(&self) -> OutputRole {
        self.info.role
    }

    pub fn layout(&self) -> LayoutKind {
        self.layout
    }

    /// The proportions of the columns shown.
    pub fn split(&self) -> Split {
        self.split_of(self.slots.len())
    }

    /// The proportions of `count` columns of the current layout.
    fn split_of(&self, count: usize) -> Split {
        self.splits
            .get(&self.layout)
            .and_then(|splits| splits.get(&count))
            .cloned()
            .unwrap_or_else(|| Split::default_for(self.layout, count))
    }

    /// The widths the user chose for this screen.
    pub fn splits(&self) -> &ScreenProportions {
        &self.splits
    }

    /// Another column can be opened.
    pub fn has_room(&self) -> bool {
        self.slots.len() < self.layout.max_columns()
    }

    pub fn slots(&self) -> &[TabStack] {
        &self.slots
    }

    pub fn floating(&self) -> &[FloatingWindow] {
        &self.floating
    }

    /// The windows maximized over the slots.
    pub fn maximized(&self) -> &TabStack {
        &self.maximized
    }

    /// The maximized windows are shown, hiding the slots.
    pub fn covered(&self) -> bool {
        self.covered
    }

    /// Its only column takes the whole screen, as a maximized window would:
    /// its windows count as maximized. Not while real maximized windows are
    /// there, whose group holds the middle.
    pub fn whole_column(&self) -> bool {
        self.slots.len() == 1 && self.maximized.is_empty()
    }

    /// The fullscreen window that anchors this screen, if any.
    pub fn anchored(&self) -> Option<WindowId> {
        self.anchored
    }

    /// The columns shown. None on a screen without tidied windows.
    pub fn slot_rects(&self, metrics: &Metrics) -> Vec<Rect> {
        self.columns(self.slots.len(), metrics)
    }

    /// Where `count` columns would be.
    pub fn columns(&self, count: usize, metrics: &Metrics) -> Vec<Rect> {
        if count == 0 {
            return Vec::new();
        }
        self.split_of(count)
            .column_rects(self.info.usable(), metrics.gap)
    }

    /// Each slot from top to bottom: the band of its capsules, then the
    /// frame of its windows.
    pub fn slot_parts(&self, metrics: &Metrics) -> Vec<(Rect, Rect)> {
        self.slot_rects(metrics)
            .into_iter()
            .map(|slot| slot.split_rows(metrics.capsule_height, metrics.gap))
            .collect()
    }

    /// Where the capsules of each slot go: the band above it.
    pub fn slot_bands(&self, metrics: &Metrics) -> Vec<Rect> {
        self.slot_parts(metrics)
            .into_iter()
            .map(|(band, _)| band)
            .collect()
    }

    /// What a maximized window takes: the band of its capsules, the middle
    /// half of the top of the screen, then the frame of the window, as the
    /// single column would have it.
    pub fn maximized_parts(&self, metrics: &Metrics) -> (Rect, Rect) {
        let whole =
            Split::default_for(self.layout, 1).column_rects(self.info.usable(), metrics.gap);
        let whole = whole.first().copied().unwrap_or_default();
        let (band, frame) = whole.split_rows(metrics.capsule_height, metrics.gap);
        let band = Rect::new(band.x + band.w / 4, band.y, band.w / 2, band.h);
        (band, frame)
    }

    /// The gaps between the columns shown, left to right.
    pub fn separators(&self, metrics: &Metrics) -> Vec<Rect> {
        if self.slots.len() < 2 {
            return Vec::new();
        }
        self.split().separators(self.info.usable(), metrics.gap)
    }

    /// The proportions with separation `index` under `point`.
    pub fn split_at(&self, metrics: &Metrics, index: usize, point: Point) -> Option<Split> {
        self.split()
            .with_separator(index, self.info.usable(), metrics.gap, point)
    }

    /// Where floating windows may be: the usable area minus the gap.
    pub fn floating_area(&self, metrics: &Metrics) -> Rect {
        self.info.usable().inset(Insets::uniform(metrics.gap))
    }

    /// Index of the slot holding `id`.
    pub fn slot_of(&self, id: WindowId) -> Option<usize> {
        self.slots.iter().position(|stack| stack.contains(id))
    }

    pub fn floating_rect(&self, id: WindowId) -> Option<Rect> {
        self.floating
            .iter()
            .find(|window| window.id == id)
            .map(|window| window.rect)
    }

    pub fn contains(&self, id: WindowId) -> bool {
        self.slot_of(id).is_some()
            || self.floating_rect(id).is_some()
            || self.maximized.contains(id)
    }

    /// Every window of the screen: slots, then maximized, then floating.
    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.slots
            .iter()
            .flat_map(|stack| stack.windows().iter().copied())
            .chain(self.maximized.windows().iter().copied())
            .chain(self.floating.iter().map(|window| window.id))
    }

    pub(crate) fn slot_mut(&mut self, index: usize) -> Option<&mut TabStack> {
        self.slots.get_mut(index)
    }

    /// Puts `id` in column `index`, as a tab if it holds windows already;
    /// one past the last column opens a new one on the right. `false` when
    /// there is no such column, or no room for another.
    pub(crate) fn push_to_slot(&mut self, index: usize, id: WindowId) -> bool {
        if index == self.slots.len() && self.has_room() {
            self.slots.push(TabStack::default());
        }
        match self.slots.get_mut(index) {
            Some(stack) => {
                stack.push(id);
                true
            }
            None => false,
        }
    }

    /// Opens a column holding `id` at `index` (or at the right end), the
    /// columns from there on moving right. There must be room.
    pub(crate) fn insert_slot(&mut self, index: usize, id: WindowId) -> usize {
        let index = index.min(self.slots.len());
        let mut stack = TabStack::default();
        stack.push(id);
        self.slots.insert(index, stack);
        index
    }

    /// Closes the columns left without a window; the others widen.
    pub(crate) fn compact(&mut self) {
        self.slots.retain(|stack| !stack.is_empty());
    }

    /// Takes `id` out of the screen, releasing the anchor if it held it.
    pub(crate) fn remove(&mut self, id: WindowId) -> bool {
        let removed = self.remove_floating(id)
            || self.maximized.remove(id)
            || self.slots.iter_mut().any(|s| s.remove(id));
        if removed && self.anchored == Some(id) {
            self.anchored = None;
        }
        removed
    }

    fn remove_floating(&mut self, id: WindowId) -> bool {
        let before = self.floating.len();
        self.floating.retain(|window| window.id != id);
        self.floating.len() != before
    }

    /// Adds `id` on top of the floating windows.
    pub(crate) fn push_floating(&mut self, id: WindowId, rect: Rect) {
        self.floating.push(FloatingWindow { id, rect });
    }

    /// Changes the rectangle of a floating window, keeping its stacking.
    pub(crate) fn set_floating_rect(&mut self, id: WindowId, rect: Rect) {
        if let Some(window) = self.floating.iter_mut().find(|window| window.id == id) {
            window.rect = rect;
        }
    }

    pub(crate) fn raise(&mut self, id: WindowId) {
        if let Some(index) = self.floating.iter().position(|window| window.id == id) {
            let window = self.floating.remove(index);
            self.floating.push(window);
        }
    }

    pub(crate) fn maximized_mut(&mut self) -> &mut TabStack {
        &mut self.maximized
    }

    pub(crate) fn set_covered(&mut self, covered: bool) {
        self.covered = covered;
    }

    /// Keeps `split` for the current layout and as many columns.
    pub(crate) fn set_split(&mut self, split: Split) {
        self.splits
            .entry(self.layout)
            .or_default()
            .insert(split.count(), split);
    }

    pub(crate) fn set_splits(&mut self, splits: ScreenProportions) {
        self.splits = splits;
    }

    pub(crate) fn set_anchored(&mut self, anchored: Option<WindowId>) {
        self.anchored = anchored;
    }

    pub(crate) fn set_size(&mut self, size: Size, metrics: &Metrics) {
        self.info.size = size;
        self.keep_floating_inside(metrics);
    }

    pub(crate) fn set_reserved(&mut self, reserved: Insets, metrics: &Metrics) {
        self.info.reserved = reserved;
        self.keep_floating_inside(metrics);
    }

    fn keep_floating_inside(&mut self, metrics: &Metrics) {
        let area = self.floating_area(metrics);
        for window in &mut self.floating {
            window.rect = window.rect.clamped_to(area);
        }
    }

    /// Switches layout. Columns beyond what the new one allows merge into
    /// the last one it keeps, as tabs; nothing else moves.
    pub(crate) fn apply_layout(&mut self, layout: LayoutKind) {
        let count = layout.max_columns();
        if self.slots.len() > count {
            let extras = self.slots.split_off(count);
            let last = &mut self.slots[count - 1];
            for extra in extras {
                last.absorb(extra);
            }
        }
        self.layout = layout;
    }
}
