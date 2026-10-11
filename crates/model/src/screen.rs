//! One output and what it holds: a layout, its slots, floating windows.

use crate::geometry::{Insets, Rect, Size};
use crate::ids::{OutputId, WindowId};
use crate::layout::{LayoutKind, Ratio};
use crate::metrics::Metrics;
use crate::output::{OutputInfo, OutputRole};
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
    ratio: Ratio,
    slots: Vec<TabStack>,
    /// Bottom to top.
    floating: Vec<FloatingWindow>,
    anchored: Option<WindowId>,
}

impl Screen {
    pub fn new(info: OutputInfo) -> Self {
        let layout = LayoutKind::default_for(info.role);
        Self {
            info,
            layout,
            ratio: Ratio::default(),
            slots: vec![TabStack::default(); layout.slot_count()],
            floating: Vec::new(),
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

    pub fn ratio(&self) -> Ratio {
        self.ratio
    }

    pub fn slots(&self) -> &[TabStack] {
        &self.slots
    }

    pub fn floating(&self) -> &[FloatingWindow] {
        &self.floating
    }

    /// The fullscreen window that anchors this screen, if any.
    pub fn anchored(&self) -> Option<WindowId> {
        self.anchored
    }

    pub fn slot_rects(&self, metrics: &Metrics) -> Vec<Rect> {
        self.layout
            .slot_rects(self.info.usable(), self.ratio, metrics.gap)
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
        self.slot_of(id).is_some() || self.floating_rect(id).is_some()
    }

    /// Every window of the screen, slots first.
    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.slots
            .iter()
            .flat_map(|stack| stack.windows().iter().copied())
            .chain(self.floating.iter().map(|window| window.id))
    }

    pub(crate) fn slot_mut(&mut self, index: usize) -> Option<&mut TabStack> {
        self.slots.get_mut(index)
    }

    /// Takes `id` out of the screen, releasing the anchor if it held it.
    pub(crate) fn remove(&mut self, id: WindowId) -> bool {
        let removed = self.remove_floating(id) || self.slots.iter_mut().any(|s| s.remove(id));
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

    /// Puts `new` where the floating window `old` is, keeping its stacking.
    pub(crate) fn replace_floating(&mut self, old: WindowId, new: WindowId) {
        if let Some(window) = self.floating.iter_mut().find(|window| window.id == old) {
            window.id = new;
        }
    }

    pub(crate) fn raise(&mut self, id: WindowId) {
        if let Some(index) = self.floating.iter().position(|window| window.id == id) {
            let window = self.floating.remove(index);
            self.floating.push(window);
        }
    }

    pub(crate) fn set_ratio(&mut self, ratio: Ratio) {
        self.ratio = ratio;
    }

    pub(crate) fn set_anchored(&mut self, anchored: Option<WindowId>) {
        self.anchored = anchored;
    }

    pub(crate) fn set_size(&mut self, size: Size, metrics: &Metrics) {
        self.info.size = size;
        let area = self.floating_area(metrics);
        for window in &mut self.floating {
            window.rect = window.rect.clamped_to(area);
        }
    }

    /// Switches layout, redistributing windows:
    /// - fewer slots: the slots in excess merge into the last one, as tabs;
    /// - more slots: the new ones start empty;
    /// - `Free`: every window floats where its slot was;
    /// - leaving `Free`: floating windows stay floating.
    pub(crate) fn apply_layout(&mut self, layout: LayoutKind, metrics: &Metrics) {
        let count = layout.slot_count();
        if count == 0 {
            let rects = self.slot_rects(metrics);
            for (stack, rect) in std::mem::take(&mut self.slots).into_iter().zip(rects) {
                let active = stack.active();
                let hidden = stack.windows().iter().filter(|id| Some(**id) != active);
                // The shown tab ends up above the ones it was hiding.
                for id in hidden.chain(active.iter()) {
                    self.floating.push(FloatingWindow { id: *id, rect });
                }
            }
        } else {
            if self.slots.len() > count {
                let extras = self.slots.split_off(count);
                let last = &mut self.slots[count - 1];
                for extra in extras {
                    last.absorb(extra);
                }
            }
            self.slots.resize_with(count, TabStack::default);
        }
        self.layout = layout;
    }
}
