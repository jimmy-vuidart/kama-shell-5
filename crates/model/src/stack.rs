//! A pile of tabs: what occupies a slot.

use crate::ids::WindowId;

/// Windows stacked in one slot. Exactly one is active (shown) unless the
/// stack is empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabStack {
    windows: Vec<WindowId>,
    active: usize,
}

impl TabStack {
    pub fn windows(&self) -> &[WindowId] {
        &self.windows
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn contains(&self, id: WindowId) -> bool {
        self.windows.contains(&id)
    }

    /// The shown window.
    pub fn active(&self) -> Option<WindowId> {
        self.windows.get(self.active).copied()
    }

    pub fn active_index(&self) -> Option<usize> {
        (!self.windows.is_empty()).then_some(self.active)
    }

    /// Adds `id` as the last tab and shows it. A window already in the stack
    /// is only shown.
    pub fn push(&mut self, id: WindowId) {
        if !self.activate(id) {
            self.windows.push(id);
            self.active = self.windows.len() - 1;
        }
    }

    /// Removes `id`. When it was the active tab, the tab that takes its
    /// position becomes active, or the previous one if it was last.
    pub fn remove(&mut self, id: WindowId) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        self.windows.remove(index);
        if index < self.active || self.active >= self.windows.len() {
            self.active = self.active.saturating_sub(1);
        }
        true
    }

    /// Shows `id`. Returns `false` when it is not in the stack.
    pub fn activate(&mut self, id: WindowId) -> bool {
        match self.position(id) {
            Some(index) => {
                self.active = index;
                true
            }
            None => false,
        }
    }

    /// Shows the tab at `index` and returns its window.
    pub fn activate_index(&mut self, index: usize) -> Option<WindowId> {
        let id = self.windows.get(index).copied()?;
        self.active = index;
        Some(id)
    }

    /// Shows the next (or previous) tab, wrapping around.
    pub fn cycle(&mut self, forward: bool) -> Option<WindowId> {
        let len = self.windows.len();
        if len == 0 {
            return None;
        }
        let step = if forward { 1 } else { len - 1 };
        self.activate_index((self.active + step) % len)
    }

    /// Appends the tabs of `other`. The active tab is kept; an empty stack
    /// adopts the active tab of `other`.
    pub fn absorb(&mut self, other: TabStack) {
        if self.windows.is_empty() {
            *self = other;
        } else {
            self.windows.extend(other.windows);
        }
    }

    fn position(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|window| *window == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: WindowId = WindowId(1);
    const B: WindowId = WindowId(2);
    const C: WindowId = WindowId(3);

    fn stack(ids: &[WindowId]) -> TabStack {
        let mut stack = TabStack::default();
        for id in ids {
            stack.push(*id);
        }
        stack
    }

    #[test]
    fn an_empty_stack_has_no_active_tab() {
        let stack = TabStack::default();

        assert_eq!(stack.active(), None);
        assert_eq!(stack.active_index(), None);
    }

    #[test]
    fn pushing_shows_the_new_tab() {
        let stack = stack(&[A, B]);

        assert_eq!(stack.windows(), &[A, B]);
        assert_eq!(stack.active(), Some(B));
    }

    #[test]
    fn pushing_a_window_already_there_only_shows_it() {
        let mut stack = stack(&[A, B]);

        stack.push(A);

        assert_eq!(stack.windows(), &[A, B]);
        assert_eq!(stack.active(), Some(A));
    }

    #[test]
    fn removing_the_active_tab_shows_the_one_that_takes_its_place() {
        let mut stack = stack(&[A, B, C]);
        stack.activate(B);

        assert!(stack.remove(B));

        assert_eq!(stack.windows(), &[A, C]);
        assert_eq!(stack.active(), Some(C));
    }

    #[test]
    fn removing_the_last_active_tab_shows_the_previous_one() {
        let mut stack = stack(&[A, B, C]);

        stack.remove(C);

        assert_eq!(stack.active(), Some(B));
    }

    #[test]
    fn removing_a_tab_before_the_active_one_keeps_the_active_window() {
        let mut stack = stack(&[A, B, C]);

        stack.remove(A);

        assert_eq!(stack.active(), Some(C));
    }

    #[test]
    fn removing_the_only_tab_empties_the_stack() {
        let mut stack = stack(&[A]);

        stack.remove(A);

        assert!(stack.is_empty());
        assert_eq!(stack.active(), None);
    }

    #[test]
    fn removing_an_unknown_window_changes_nothing() {
        let mut stack = stack(&[A, B]);

        assert!(!stack.remove(C));
        assert_eq!(stack.active(), Some(B));
    }

    #[test]
    fn activating_an_unknown_window_is_refused() {
        let mut stack = stack(&[A, B]);

        assert!(!stack.activate(C));
        assert_eq!(stack.active(), Some(B));
    }

    #[test]
    fn activating_an_index_out_of_range_is_refused() {
        let mut stack = stack(&[A, B]);

        assert_eq!(stack.activate_index(2), None);
        assert_eq!(stack.activate_index(0), Some(A));
        assert_eq!(stack.active(), Some(A));
    }

    #[test]
    fn cycling_wraps_around_in_both_directions() {
        let mut stack = stack(&[A, B, C]);

        assert_eq!(stack.cycle(true), Some(A));
        assert_eq!(stack.cycle(false), Some(C));
        assert_eq!(stack.cycle(false), Some(B));
    }

    #[test]
    fn cycling_an_empty_stack_gives_nothing() {
        assert_eq!(TabStack::default().cycle(true), None);
    }

    #[test]
    fn absorbing_appends_and_keeps_the_active_tab() {
        let mut first = stack(&[A]);

        first.absorb(stack(&[B, C]));

        assert_eq!(first.windows(), &[A, B, C]);
        assert_eq!(first.active(), Some(A));
    }

    #[test]
    fn an_empty_stack_adopts_the_active_tab_it_absorbs() {
        let mut other = stack(&[A, B, C]);
        other.activate(B);
        let mut empty = TabStack::default();

        empty.absorb(other);

        assert_eq!(empty.active(), Some(B));
    }
}
