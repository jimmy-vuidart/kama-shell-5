//! The window switcher (Alt+Tab): while Alt is held, Tab goes through the
//! windows in the order they were used, and letting go of Alt shows the one
//! chosen. Escape gives up.

use coquille_model::{OutputId, Rect, WindowId};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;
use tracing::warn;

use crate::decoration::naming::badge_color;
use crate::decoration::paint::{SwitcherSpec, SwitcherTile};
use crate::state::State;

/// The windows to go through, and the one chosen. Pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    windows: Vec<WindowId>,
    selected: usize,
}

impl Choice {
    /// Starts on the window used before the last one: a single Tab goes
    /// back to it. Backwards, on the one used longest ago. `None` without
    /// window.
    pub fn start(windows: Vec<WindowId>, forward: bool) -> Option<Self> {
        let count = windows.len();
        if count == 0 {
            return None;
        }
        let selected = match (forward, count) {
            (_, 1) => 0,
            (true, _) => 1,
            (false, _) => count - 1,
        };
        Some(Self { windows, selected })
    }

    /// The next window, or the previous one, around the ends.
    pub fn step(&mut self, forward: bool) {
        let count = self.windows.len();
        self.selected = if forward {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
    }

    /// Keeps only the windows still there. `false` when none is left.
    pub fn keep(&mut self, alive: impl Fn(WindowId) -> bool) -> bool {
        let chosen = self.selected();
        self.windows.retain(|window| alive(*window));
        let count = self.windows.len();
        if count == 0 {
            return false;
        }
        self.selected = match self.windows.iter().position(|window| *window == chosen) {
            Some(index) => index,
            None => self.selected.min(count - 1),
        };
        true
    }

    pub fn windows(&self) -> &[WindowId] {
        &self.windows
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> WindowId {
        self.windows[self.selected]
    }
}

/// The switcher as it is shown, on one screen.
pub struct Switcher {
    pub choice: Choice,
    pub output: OutputId,
    /// In the global space.
    pub rect: Rect,
    pub buffer: Option<MemoryRenderBuffer>,
    /// What it was painted from.
    spec: Option<SwitcherSpec>,
}

impl State {
    /// Alt+Tab, or Alt+Shift+Tab: starts the switch on the screen of the
    /// pointer, or goes to the next window.
    pub fn switch_window(&mut self, forward: bool) {
        if let Some(switcher) = &mut self.switcher {
            switcher.choice.step(forward);
        } else {
            let known = |id: &WindowId| self.shell.by_id(*id).is_some();
            let windows: Vec<WindowId> = self.shell.desk.recent_windows().filter(known).collect();
            let output = self
                .pointer_screen()
                .or_else(|| self.shell.screens.first().copied())
                .map(|screen| screen.id);
            let (Some(choice), Some(output)) = (Choice::start(windows, forward), output) else {
                return;
            };
            self.switcher = Some(Switcher {
                choice,
                output,
                rect: Rect::default(),
                buffer: None,
                spec: None,
            });
        }
        self.queue_redraw();
    }

    /// Alt let go: the window chosen shows and takes the focus. Minimized,
    /// it comes back; behind a game, it comes over it.
    pub fn finish_switch(&mut self) {
        let Some(switcher) = self.switcher.take() else {
            return;
        };
        let chosen = switcher.choice.selected();
        match self.shell.desk.bring(chosen) {
            Ok(()) => self.apply_arrangement(),
            Err(error) => warn!(%error, "cannot show the window chosen"),
        }
        self.refresh_constraint();
        self.full_redraw = true;
        self.queue_redraw();
    }

    /// Escape: nothing changes.
    pub fn cancel_switch(&mut self) {
        if self.switcher.take().is_some() {
            self.full_redraw = true;
            self.queue_redraw();
        }
    }

    /// Keeps the switcher in step with the windows; called before each
    /// frame. A window that closed leaves it, and it ends with the last one.
    pub fn refresh_switcher(&mut self) {
        let Some(mut switcher) = self.switcher.take() else {
            return;
        };
        let alive = |id: WindowId| self.shell.by_id(id).is_some();
        let screen = self.shell.screen(switcher.output).copied();
        let screen = screen.or_else(|| self.shell.screens.first().copied());
        let (true, Some(screen)) = (switcher.choice.keep(alive), screen) else {
            self.full_redraw = true;
            return;
        };
        let tile = |id: &WindowId| {
            let managed = self.shell.by_id(*id)?;
            Some(SwitcherTile {
                label: managed.name.clone(),
                title: managed.title.clone(),
                icon: managed.icon.clone(),
                badge: badge_color(&managed.app_id),
                minimized: managed.geometry.minimized,
            })
        };
        let spec = SwitcherSpec {
            tiles: switcher.choice.windows().iter().filter_map(tile).collect(),
            selected: switcher.choice.selected_index(),
            screen_width: screen.rect.w,
            theme: self.theme.kind,
            accent: self.theme.accent,
            scale: screen.scale,
        };
        if switcher.spec.as_ref() != Some(&spec) {
            match self.painter.switcher(&spec) {
                Some(painted) => {
                    let size = painted.size;
                    switcher.rect = Rect::centered(size, screen.rect);
                    switcher.buffer = Some(MemoryRenderBuffer::from_slice(
                        painted.pixmap.data(),
                        Fourcc::Abgr8888,
                        (
                            painted.pixmap.width() as i32,
                            painted.pixmap.height() as i32,
                        ),
                        1,
                        Transform::Normal,
                        None,
                    ));
                }
                None => switcher.buffer = None,
            }
            switcher.spec = Some(spec);
        }
        self.switcher = Some(switcher);
    }
}

impl Switcher {
    /// What the frame draws, and where.
    pub fn panel(&self) -> Option<(&MemoryRenderBuffer, Rect)> {
        Some((self.buffer.as_ref()?, self.rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(ids: &[u64]) -> Vec<WindowId> {
        ids.iter().copied().map(WindowId).collect()
    }

    #[test]
    fn one_tab_goes_back_to_the_window_used_before() {
        let choice = Choice::start(windows(&[7, 3, 5]), true).unwrap();

        assert_eq!(choice.selected(), WindowId(3));
    }

    #[test]
    fn backwards_the_switch_starts_on_the_window_used_longest_ago() {
        let choice = Choice::start(windows(&[7, 3, 5]), false).unwrap();

        assert_eq!(choice.selected(), WindowId(5));
    }

    #[test]
    fn tab_goes_round_the_windows() {
        let mut choice = Choice::start(windows(&[7, 3, 5]), true).unwrap();

        choice.step(true);
        assert_eq!(choice.selected(), WindowId(5));
        choice.step(true);
        assert_eq!(choice.selected(), WindowId(7));
        choice.step(false);
        assert_eq!(choice.selected(), WindowId(5));
    }

    #[test]
    fn a_lone_window_is_chosen_and_nothing_starts_without_window() {
        let choice = Choice::start(windows(&[7]), true).unwrap();

        assert_eq!(choice.selected(), WindowId(7));
        assert!(Choice::start(Vec::new(), true).is_none());
    }

    #[test]
    fn a_window_that_closes_leaves_the_choice() {
        let mut choice = Choice::start(windows(&[7, 3, 5]), true).unwrap();
        choice.step(true);

        // The chosen one stays chosen when another one goes.
        assert!(choice.keep(|id| id != WindowId(3)));
        assert_eq!(choice.selected(), WindowId(5));
        // The chosen one gone, its neighbour is.
        assert!(choice.keep(|id| id != WindowId(5)));
        assert_eq!(choice.selected(), WindowId(7));
        assert!(!choice.keep(|_| false));
    }
}
