//! The whole desk: every screen, every window, and the placement rules.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::geometry::{Insets, Point, Rect, Size};
use crate::ids::{AppKey, OutputId, WindowId};
use crate::layout::{LayoutKind, SlotId, StackId};
use crate::memory::{AppMemory, RememberedPlace};
use crate::metrics::Metrics;
use crate::output::{OutputInfo, OutputRole};
use crate::proportions::Proportions;
use crate::screen::Screen;
use crate::stack::TabStack;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ModelError {
    #[error("unknown window {0:?}")]
    UnknownWindow(WindowId),
    #[error("window {0:?} already exists")]
    DuplicateWindow(WindowId),
    #[error("unknown output {0:?}")]
    UnknownOutput(OutputId),
    #[error("slot {0:?} does not exist in the current layout")]
    UnknownSlot(SlotId),
    #[error("{stack:?} has no tab {index}")]
    UnknownTab { stack: StackId, index: usize },
    #[error("output {0:?} is anchored by a fullscreen window")]
    Anchored(OutputId),
    #[error("window {0:?} is not floating")]
    NotFloating(WindowId),
    #[error("window {0:?} is not on a side screen")]
    NotOnSideScreen(WindowId),
    #[error("there is no center screen")]
    NoCenter,
}

/// Where a window lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Floating {
        output: OutputId,
        rect: Rect,
    },
    Slot(SlotId),
    /// Over the slots of its screen.
    Maximized(OutputId),
}

/// Context of a window opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenRequest {
    /// The output the cursor is on.
    pub cursor_output: OutputId,
    /// Size the window asks for, decorations included.
    pub preferred_size: Option<Size>,
    /// The window is a dialog of this one: it follows no habit, and opens
    /// over it when it is on the same screen.
    pub parent: Option<WindowId>,
}

impl OpenRequest {
    pub const fn at(cursor_output: OutputId) -> Self {
        Self {
            cursor_output,
            preferred_size: None,
            parent: None,
        }
    }

    pub const fn with_size(mut self, size: Size) -> Self {
        self.preferred_size = Some(size);
        self
    }

    pub const fn with_parent(mut self, parent: WindowId) -> Self {
        self.parent = Some(parent);
        self
    }
}

/// What the model knows about a window besides its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowRecord {
    pub app: Option<AppKey>,
    pub title: String,
    /// The client draws its own title bar, so the shell adds none.
    pub own_titlebar: bool,
}

/// Where a window stood when the user took hold of it, to put it back there
/// if they change their mind ([`Desk::lift`], [`Desk::put_down`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifted {
    id: WindowId,
    output: OutputId,
    from: LiftedFrom,
    /// The habit its application had then.
    habit: Option<RememberedPlace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiftedFrom {
    /// Its column; `alone` when it was the only window there.
    Slot {
        index: usize,
        alone: bool,
    },
    Maximized,
    Floating(Rect),
}

impl Lifted {
    pub fn window(&self) -> WindowId {
        self.id
    }
}

/// Where a dragged window can be released: a column of a screen, or the one
/// it would open on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropZone {
    pub slot: SlotId,
    pub rect: Rect,
    /// Another window is already there: releasing stacks as a tab.
    pub occupied: bool,
}

/// The capsules of a stack: one per window. Those of a slot are above it,
/// those of the maximized windows at the top of the middle of the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleGroup {
    pub stack: StackId,
    /// Where the capsules go, relative to the screen.
    pub band: Rect,
    /// In the order of the stack.
    pub windows: Vec<WindowId>,
    /// The window the stack shows; `None` when they are all minimized, or
    /// when the stack is hidden: slots under maximized windows, or
    /// maximized windows put aside.
    pub shown: Option<WindowId>,
    /// Shown as maximized windows: at the top of the middle of the screen,
    /// with "restore". Those of the only column of a screen are too.
    pub maximized: bool,
}

/// Geometry and state of one window, as the compositor must apply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub id: WindowId,
    pub output: OutputId,
    /// Whole decorated window.
    pub frame: Rect,
    /// `None` for an anchored window: it has no decoration.
    pub titlebar: Option<Rect>,
    /// What the client draws into.
    pub client: Rect,
    /// Hidden windows are inactive tabs, or anything under an anchored window.
    pub visible: bool,
    pub focused: bool,
    pub floating: bool,
    /// Over the slots of its screen.
    pub maximized: bool,
    pub anchored: bool,
    /// Hidden by the user, until restored.
    pub minimized: bool,
}

/// Where a window is inside its screen.
#[derive(Debug, Clone, Copy)]
enum Spot {
    Slot(usize),
    Floating(Rect),
    Maximized,
}

/// What a window is in, for its geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Slot,
    Maximized,
    Floating,
}

/// Where a maximized window goes back to, on its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Its column; `alone` when it was the only window there, so that the
    /// column closed behind it.
    Slot {
        index: usize,
        alone: bool,
    },
    Floating(Rect),
}

/// Where a remembered application reopens. Indices are into `Desk::screens`.
enum Target {
    Slot { screen: usize, slot: usize },
    Maximized { screen: usize, back: Option<usize> },
}

/// Every screen and every window. See the crate documentation.
#[derive(Debug, Clone)]
pub struct Desk {
    metrics: Metrics,
    screens: Vec<Screen>,
    windows: BTreeMap<WindowId, WindowRecord>,
    /// Every window exactly once, most recently focused last.
    focus_order: Vec<WindowId>,
    /// Windows hidden by the user, the one hidden last at the end.
    minimized: Vec<WindowId>,
    memory: AppMemory,
    /// Where each maximized window goes back to.
    restore: BTreeMap<WindowId, Origin>,
    last_side: Option<OutputId>,
    /// Screens asked for the next window of an application, once each. An
    /// application is asked for under every name its windows may bear.
    asked: Vec<(Vec<AppKey>, OutputId)>,
    /// Windows the user brought over the game that holds their screen:
    /// shown above it until the game is clicked again.
    over_game: Vec<WindowId>,
}

impl Desk {
    /// Each output starts with the default layout of its role.
    pub fn new(outputs: Vec<OutputInfo>, metrics: Metrics) -> Self {
        Self {
            metrics,
            screens: outputs.into_iter().map(Screen::new).collect(),
            windows: BTreeMap::new(),
            focus_order: Vec::new(),
            minimized: Vec::new(),
            memory: AppMemory::default(),
            restore: BTreeMap::new(),
            last_side: None,
            asked: Vec::new(),
            over_game: Vec::new(),
        }
    }

    /// Starts from habits saved by a previous session.
    pub fn with_memory(mut self, memory: AppMemory) -> Self {
        self.memory = memory;
        self
    }

    pub fn memory(&self) -> &AppMemory {
        &self.memory
    }

    /// Starts from the widths of columns saved by a previous session.
    pub fn with_proportions(mut self, proportions: &Proportions) -> Self {
        for screen in &mut self.screens {
            screen.set_splits(proportions.of(screen.role()));
        }
        self
    }

    /// The widths the user gave to the columns, screen by screen.
    pub fn proportions(&self) -> Proportions {
        let mut proportions = Proportions::default();
        for screen in &self.screens {
            proportions.set(screen.role(), screen.splits().clone());
        }
        proportions
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }

    pub fn screen(&self, output: OutputId) -> Option<&Screen> {
        self.screens.iter().find(|screen| screen.id() == output)
    }

    pub fn window(&self, id: WindowId) -> Option<&WindowRecord> {
        self.windows.get(&id)
    }

    pub fn placement(&self, id: WindowId) -> Option<Placement> {
        let (screen, spot) = self.locate(id)?;
        let output = self.screens[screen].id();
        Some(match spot {
            Spot::Slot(index) => Placement::Slot(SlotId::new(output, index)),
            Spot::Floating(rect) => Placement::Floating { output, rect },
            Spot::Maximized => Placement::Maximized(output),
        })
    }

    /// The window with the focus: the one focused last that can be seen,
    /// that is neither minimized, under a fullscreen window, nor on the side
    /// of its screen that does not show (slots under maximized windows, or
    /// maximized windows put aside).
    pub fn focused(&self) -> Option<WindowId> {
        let shown = |id: &&WindowId| {
            !self.minimized.contains(id) && !self.under_a_game(**id) && !self.put_aside(**id)
        };
        self.focus_order.iter().rev().find(shown).copied()
    }

    /// `id` is in a slot under maximized windows, or maximized while its
    /// screen shows its slots.
    fn put_aside(&self, id: WindowId) -> bool {
        self.screens.iter().any(|screen| {
            if screen.covered() {
                screen.slot_of(id).is_some()
            } else {
                screen.maximized().contains(id)
            }
        })
    }

    /// `id` is on a screen that another window anchors, and was not
    /// brought over it.
    fn under_a_game(&self, id: WindowId) -> bool {
        let hidden = |screen: &Screen| {
            screen.anchored().is_some_and(|game| game != id) && screen.contains(id)
        };
        !self.over_game.contains(&id) && self.screens.iter().any(hidden)
    }

    /// The user asks for a window (from the dock): it shows and takes the
    /// focus. One that a game hides comes over the game, which stays
    /// fullscreen: on a lone screen there is nowhere else to show it.
    /// Clicking the game puts the game back in front.
    pub fn bring(&mut self, id: WindowId) -> Result<(), ModelError> {
        if !self.windows.contains_key(&id) {
            return Err(ModelError::UnknownWindow(id));
        }
        if self.under_a_game(id) {
            self.over_game.push(id);
        }
        self.mark_focused(id);
        Ok(())
    }

    /// `id` was brought over the game of its screen.
    pub fn is_over_game(&self, id: WindowId) -> bool {
        self.over_game.contains(&id)
    }

    /// A new window appears (rules 1, 4 and 7) and takes the focus, unless a
    /// game is being played: the game keeps it, and the new window, shown on
    /// another screen, is next in line.
    ///
    /// An application the user tidied reopens in its slot. Anything else opens
    /// floating on the cursor's screen, or, when a game is anchored, on the
    /// side screen used last. No other window moves. A dialog
    /// ([`OpenRequest::with_parent`]) never follows a habit: it floats over
    /// its window.
    pub fn open_window(
        &mut self,
        id: WindowId,
        app: Option<AppKey>,
        request: OpenRequest,
    ) -> Result<Placement, ModelError> {
        if self.windows.contains_key(&id) {
            return Err(ModelError::DuplicateWindow(id));
        }
        let cursor = self.screen_index(request.cursor_output)?;
        let game = self.game_in_play().and_then(|_| self.focused());

        // A screen asked for this window comes first, then the center for a
        // game; both leave the habits alone, and give way to a game being
        // played there.
        let asked = app.as_ref().and_then(|app| {
            let is_named = |names: &Vec<AppKey>| names.iter().any(|name| name.same_app(app));
            let index = self.asked.iter().position(|(names, _)| is_named(names))?;
            Some(self.asked.remove(index).1)
        });
        let asked = asked.and_then(|output| self.screen_index(output).ok());
        let is_game = app.as_ref().is_some_and(AppKey::is_game);
        let center = || {
            let is_center = |screen: &Screen| screen.role() == OutputRole::Center;
            self.screens.iter().position(is_center)
        };
        let wanted = asked.or_else(|| is_game.then(center).flatten());

        // A dialog has the name of its application, never its place.
        let remembered = app.as_ref().and_then(|app| self.memory.get(app));
        let remembered = remembered.filter(|_| wanted.is_none() && request.parent.is_none());
        let placement = match remembered.and_then(|place| self.remembered_target(place)) {
            Some(Target::Slot { screen, slot }) => {
                // Another window of the same application: the new one opens
                // next to it while there is room.
                let next_to = |stack: &TabStack| {
                    let app = app.as_ref();
                    stack.windows().iter().any(|window| {
                        let other = self.windows.get(window).and_then(|r| r.app.as_ref());
                        other.is_some_and(|other| app.is_some_and(|app| other.same_app(app)))
                    })
                };
                let shown = &self.screens[screen];
                let beside = shown.has_room() && shown.slots().get(slot).is_some_and(next_to);
                let slot = self.put_back(screen, id, slot + usize::from(beside), true);
                Placement::Slot(SlotId::new(self.screens[screen].id(), slot))
            }
            // Shown at once, even while a game keeps the focus elsewhere.
            Some(Target::Maximized { screen, back }) => {
                let origin = match back {
                    Some(index) => Origin::Slot { index, alone: true },
                    None => Origin::Floating(self.opening_rect(screen, request.preferred_size)),
                };
                let screen = &mut self.screens[screen];
                screen.maximized_mut().push(id);
                screen.set_covered(true);
                self.restore.insert(id, origin);
                Placement::Maximized(screen.id())
            }
            None => {
                let free = wanted.filter(|screen| self.screens[*screen].anchored().is_none());
                let screen = free.unwrap_or_else(|| self.floating_screen(cursor));
                let rect = self.opening_rect_over(screen, request.preferred_size, request.parent);
                let screen = &mut self.screens[screen];
                screen.push_floating(id, rect);
                Placement::Floating {
                    output: screen.id(),
                    rect,
                }
            }
        };

        self.windows.insert(
            id,
            WindowRecord {
                app,
                title: String::new(),
                own_titlebar: false,
            },
        );
        match game.and_then(|game| self.focus_order.iter().position(|id| *id == game)) {
            // Shown where it opened, right behind the game in the focus order.
            Some(rank) => self.focus_order.insert(rank, id),
            None => self.mark_focused(id),
        }
        Ok(placement)
    }

    /// The next window of an application opens floating on `output`,
    /// whatever its habit says, which is left as it is. Asked from the dock
    /// ("open on"), which gives every name the windows of the application
    /// may bear. Asking again replaces what was asked before.
    pub fn open_next_on(&mut self, names: Vec<AppKey>, output: OutputId) -> Result<(), ModelError> {
        /// Applications asked for that never showed up are not kept for ever.
        const KEPT: usize = 8;
        self.screen_index(output)?;
        let overlaps = |known: &Vec<AppKey>| {
            known
                .iter()
                .any(|name| names.iter().any(|other| other.same_app(name)))
        };
        self.asked.retain(|(known, _)| !overlaps(known));
        self.asked.push((names, output));
        let excess = self.asked.len().saturating_sub(KEPT);
        self.asked.drain(..excess);
        Ok(())
    }

    /// Forgets where the user tidied an application: it opens floating again.
    pub fn forget_app(&mut self, app: &AppKey) {
        self.memory.forget(app);
    }

    /// Every window, the one focused last first.
    pub fn recent_windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.focus_order.iter().rev().copied()
    }

    /// A window is gone. If it had the focus, its tab neighbour gets it, else
    /// the window focused before it.
    pub fn close_window(&mut self, id: WindowId) -> Result<(), ModelError> {
        let (screen, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        let was_focused = self.focused() == Some(id);

        self.take(screen, id);
        // Its column may close, and the columns after it move left.
        let column_left = match spot {
            Spot::Slot(index) => !self.screens[screen].slots()[index].is_empty(),
            _ => false,
        };
        self.windows.remove(&id);
        self.focus_order.retain(|window| *window != id);
        self.minimized.retain(|window| *window != id);
        self.over_game.retain(|window| *window != id);
        self.forget_over_game();
        self.settle();

        if was_focused {
            let screen = &self.screens[screen];
            let neighbour = match spot {
                Spot::Slot(index) if column_left => screen.slots()[index].active(),
                Spot::Slot(_) => None,
                Spot::Maximized => screen.maximized().active().filter(|_| screen.covered()),
                Spot::Floating(_) => None,
            };
            let neighbour = neighbour.filter(|window| !self.minimized.contains(window));
            if let Some(next) = neighbour.or_else(|| self.focused()) {
                self.mark_focused(next);
            }
        }
        Ok(())
    }

    pub fn set_title(&mut self, id: WindowId, title: impl Into<String>) -> Result<(), ModelError> {
        let record = self
            .windows
            .get_mut(&id)
            .ok_or(ModelError::UnknownWindow(id))?;
        record.title = title.into();
        Ok(())
    }

    /// The user tidies a window into a slot (rules 2, 3 and 4): one of the
    /// columns of a screen, or, while the layout has room, the one past the
    /// last, which opens a new column on the right.
    ///
    /// An occupied slot never refuses: the window stacks as a tab. The
    /// application will reopen there.
    pub fn move_to_slot(&mut self, id: WindowId, slot: SlotId) -> Result<(), ModelError> {
        let (from, _) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        let to = self.screen_index(slot.output)?;
        self.ensure_not_anchored(from)?;
        self.ensure_not_anchored(to)?;
        let screen = &self.screens[to];
        let count = screen.slots().len();
        if slot.index > count || (slot.index == count && !screen.has_room()) {
            return Err(ModelError::UnknownSlot(slot));
        }

        self.take(from, id);
        self.screens[to].push_to_slot(slot.index, id);
        self.settle();

        // Its own column may have closed on the way, moving it left.
        let index = self.screens[to].slot_of(id).unwrap_or(slot.index);
        let place = RememberedPlace::in_slot(self.screens[to].role(), index);
        self.remember(id, Some(place));
        self.mark_focused(id);
        Ok(())
    }

    /// The user makes a window float, or moves a floating one.
    ///
    /// `rect` is kept inside the screen. Taking a window out of a slot forgets
    /// the habit of its application: floating windows never create one.
    pub fn float_window(
        &mut self,
        id: WindowId,
        output: OutputId,
        rect: Rect,
    ) -> Result<(), ModelError> {
        let (from, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        let to = self.screen_index(output)?;
        self.ensure_not_anchored(from)?;
        self.ensure_not_anchored(to)?;

        self.take(from, id);
        let rect = rect.clamped_to(self.screens[to].floating_area(&self.metrics));
        self.screens[to].push_floating(id, rect);
        self.settle();

        if !matches!(spot, Spot::Floating(_)) {
            self.remember(id, None);
        }
        self.mark_focused(id);
        Ok(())
    }

    /// Where `id` stands, before the user drags it away.
    pub fn lift(&self, id: WindowId) -> Option<Lifted> {
        let (screen, spot) = self.locate(id)?;
        let screen = &self.screens[screen];
        let from = match spot {
            Spot::Slot(index) => LiftedFrom::Slot {
                index,
                alone: screen.slots()[index].windows().len() == 1,
            },
            Spot::Maximized => LiftedFrom::Maximized,
            Spot::Floating(rect) => LiftedFrom::Floating(rect),
        };
        let app = self.windows.get(&id).and_then(|record| record.app.as_ref());
        Some(Lifted {
            id,
            output: screen.id(),
            from,
            habit: app.and_then(|app| self.memory.get(app)),
        })
    }

    /// The user changed their mind while dragging a window: it goes back
    /// where [`Desk::lift`] saw it (its column, opened again at its place
    /// if it closed and there is room, else as a tab), and its application
    /// gets its habit back.
    pub fn put_down(&mut self, lifted: Lifted) -> Result<(), ModelError> {
        let id = lifted.id;
        let (from, _) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        let to = self.screen_index(lifted.output)?;
        self.ensure_not_anchored(from)?;
        self.ensure_not_anchored(to)?;

        self.take(from, id);
        match lifted.from {
            LiftedFrom::Slot { index, alone } => {
                self.put_back(to, id, index, alone);
            }
            LiftedFrom::Maximized => {
                let screen = &mut self.screens[to];
                screen.maximized_mut().push(id);
                screen.set_covered(true);
            }
            LiftedFrom::Floating(rect) => {
                let area = self.screens[to].floating_area(&self.metrics);
                self.screens[to].push_floating(id, rect.clamped_to(area));
            }
        }
        self.settle();
        self.remember(id, lifted.habit);
        self.mark_focused(id);
        Ok(())
    }

    /// Where tidying a floating window on its own screen would put it: a
    /// new column while the layout has room, else the column focused last.
    /// `None` on an anchored screen, or when the window is not floating.
    pub fn suggested_slot(&self, id: WindowId) -> Option<SlotId> {
        let (index, spot) = self.locate(id)?;
        let screen = &self.screens[index];
        if !matches!(spot, Spot::Floating(_)) || screen.anchored().is_some() {
            return None;
        }
        Some(SlotId::new(screen.id(), self.tidy_slot(index, id)))
    }

    /// The slot a window tidied on a screen without saying where goes to:
    /// a new column while there is room, else the column of the window
    /// focused last there, else the first one.
    fn tidy_slot(&self, screen: usize, id: WindowId) -> usize {
        let screen = &self.screens[screen];
        if screen.has_room() {
            return screen.slots().len();
        }
        let mut others = self
            .focus_order
            .iter()
            .rev()
            .filter(|window| **window != id);
        others
            .find_map(|window| screen.slot_of(*window))
            .unwrap_or(0)
    }

    /// A floating window's client chose another size. Unlike
    /// [`Desk::float_window`], this is not an action of the user: the focus
    /// and the habits stay as they are.
    pub fn set_floating_rect(&mut self, id: WindowId, rect: Rect) -> Result<(), ModelError> {
        let (index, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        if !matches!(spot, Spot::Floating(_)) {
            return Err(ModelError::NotFloating(id));
        }
        let screen = &mut self.screens[index];
        let rect = rect.clamped_to(screen.floating_area(&self.metrics));
        screen.set_floating_rect(id, rect);
        Ok(())
    }

    /// "To the center": the user tidies a side window into the center
    /// screen, in a new column while there is room, else in the column used
    /// last there, as a tab (rule 3).
    ///
    /// Refused while a game is anchored on either screen.
    pub fn send_to_center(&mut self, id: WindowId) -> Result<(), ModelError> {
        let (side, _) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        if !self.screens[side].role().is_side() {
            return Err(ModelError::NotOnSideScreen(id));
        }
        let center = self
            .screens
            .iter()
            .position(|screen| screen.role() == OutputRole::Center)
            .ok_or(ModelError::NoCenter)?;
        self.ensure_not_anchored(center)?;
        self.ensure_not_anchored(side)?;

        let output = self.screens[center].id();
        self.move_to_slot(id, SlotId::new(output, self.tidy_slot(center, id)))
    }

    /// Changes the layout of a screen, a setting of the user: columns
    /// beyond what it allows merge into its last one, as tabs. The focused
    /// window stays visible.
    pub fn set_layout(&mut self, output: OutputId, kind: LayoutKind) -> Result<(), ModelError> {
        let index = self.screen_index(output)?;
        if self.screens[index].layout() == kind {
            return Ok(());
        }
        self.screens[index].apply_layout(kind);
        self.settle();
        self.show_focused();
        Ok(())
    }

    /// Puts separation `index` of a screen under `point`, given relative to
    /// the screen. Says whether the columns changed.
    pub fn move_separator(
        &mut self,
        output: OutputId,
        index: usize,
        point: Point,
    ) -> Result<bool, ModelError> {
        let screen = self.screen_index(output)?;
        self.ensure_not_anchored(screen)?;
        let screen = &mut self.screens[screen];
        match screen.split_at(&self.metrics, index, point) {
            Some(split) if split != screen.split() => {
                screen.set_split(split);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// The output changed size; floating windows are kept inside it.
    pub fn set_output_size(&mut self, output: OutputId, size: Size) -> Result<(), ModelError> {
        let index = self.screen_index(output)?;
        self.screens[index].set_size(size, &self.metrics);
        Ok(())
    }

    /// A panel of the shell takes (or gives back) room along the edges of an
    /// output: slots and floating windows keep out of it.
    pub fn set_reserved(&mut self, output: OutputId, reserved: Insets) -> Result<(), ModelError> {
        let index = self.screen_index(output)?;
        self.screens[index].set_reserved(reserved, &self.metrics);
        Ok(())
    }

    /// Gives the focus to a window, showing its tab or raising it.
    pub fn focus(&mut self, id: WindowId) -> Result<(), ModelError> {
        if !self.windows.contains_key(&id) {
            return Err(ModelError::UnknownWindow(id));
        }
        self.mark_focused(id);
        Ok(())
    }

    /// An application asks to be brought forward (its tray icon, a second
    /// launch, a notification): the window shows, minimized or behind another
    /// tab, and takes the focus. While a game is being played, the game keeps
    /// it, and the window is next in line, as one that opens then.
    pub fn activate(&mut self, id: WindowId) -> Result<(), ModelError> {
        if !self.windows.contains_key(&id) {
            return Err(ModelError::UnknownWindow(id));
        }
        let game = self.game_in_play().and_then(|_| self.focused());
        self.mark_focused(id);
        if let Some(game) = game.filter(|game| *game != id) {
            self.focus_order.retain(|window| *window != game);
            self.focus_order.push(game);
        }
        Ok(())
    }

    /// The user acted on `output` (pointer, click). Only side screens are
    /// recorded: rule 7 sends new windows to the one used last.
    pub fn note_output_used(&mut self, output: OutputId) {
        if self
            .screen(output)
            .is_some_and(|screen| screen.role().is_side())
        {
            self.last_side = Some(output);
        }
    }

    /// Shows and focuses a tab. A tab of a slot brings the slots back from
    /// under maximized windows; a maximized one covers them again.
    pub fn activate_tab(
        &mut self,
        stack: impl Into<StackId>,
        index: usize,
    ) -> Result<(), ModelError> {
        let stack = stack.into();
        let id = self
            .stack_mut(stack)?
            .activate_index(index)
            .ok_or(ModelError::UnknownTab { stack, index })?;
        self.mark_focused(id);
        Ok(())
    }

    /// Shows and focuses the next (or previous) tab of a stack.
    pub fn cycle_tab(
        &mut self,
        stack: impl Into<StackId>,
        forward: bool,
    ) -> Result<(), ModelError> {
        if let Some(id) = self.stack_mut(stack.into())?.cycle(forward) {
            self.mark_focused(id);
        }
        Ok(())
    }

    /// `id` is maximized over the slots of its screen, or in the only column
    /// of its screen, which counts as maximized.
    pub fn is_maximized(&self, id: WindowId) -> bool {
        match self.locate(id) {
            Some((_, Spot::Maximized)) => true,
            Some((index, Spot::Slot(_))) => self.screens[index].whole_column(),
            _ => false,
        }
    }

    /// A window asks for its whole screen, or the user does it for it: it
    /// leaves its slot, or stops floating, and covers the slots of its
    /// screen, its capsule at the top of the middle. Its application will
    /// reopen there, maximized.
    ///
    /// A window in the only column of its screen already fills it and counts
    /// as maximized: it stays as it is. Refused on an anchored screen.
    pub fn maximize(&mut self, id: WindowId) -> Result<(), ModelError> {
        let (index, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        self.ensure_not_anchored(index)?;
        let origin = match spot {
            Spot::Maximized => {
                self.mark_focused(id);
                return Ok(());
            }
            Spot::Slot(_) if self.screens[index].slots().len() == 1 => return Ok(()),
            Spot::Slot(slot) => Origin::Slot {
                index: slot,
                alone: self.screens[index].slots()[slot].windows().len() == 1,
            },
            Spot::Floating(rect) => Origin::Floating(rect),
        };

        self.screens[index].remove(id);
        self.screens[index].maximized_mut().push(id);
        self.restore.insert(id, origin);
        self.settle();

        let back = match origin {
            Origin::Slot { index, .. } => Some(index),
            Origin::Floating(_) => None,
        };
        let place = RememberedPlace::maximized(self.screens[index].role(), back);
        self.remember(id, Some(place));
        self.mark_focused(id);
        Ok(())
    }

    /// A maximized window goes back where it was: its column, opened again
    /// if it closed and there is room, else as a tab; or floating where it
    /// floated. Its application reopens there; floating, it has no habit
    /// any more.
    ///
    /// A window of the only column of its screen, maximized as it is, leaves
    /// it and floats, and its application has no habit any more.
    pub fn unmaximize(&mut self, id: WindowId) -> Result<(), ModelError> {
        let (index, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        match spot {
            Spot::Maximized => {}
            Spot::Slot(_) if self.screens[index].whole_column() => {
                self.ensure_not_anchored(index)?;
                self.screens[index].remove(id);
                let rect = self.opening_rect(index, None);
                self.screens[index].push_floating(id, rect);
                self.settle();
                self.remember(id, None);
                self.mark_focused(id);
                return Ok(());
            }
            Spot::Slot(_) | Spot::Floating(_) => return Ok(()),
        }
        self.ensure_not_anchored(index)?;
        let origin = self.restore.remove(&id);
        self.screens[index].remove(id);

        let area = self.screens[index].floating_area(&self.metrics);
        let back = match origin {
            Some(Origin::Slot { index: slot, alone }) => {
                Some(self.put_back(index, id, slot, alone))
            }
            Some(Origin::Floating(rect)) => {
                self.screens[index].push_floating(id, rect.clamped_to(area));
                None
            }
            None => {
                let rect = self.opening_rect(index, None);
                self.screens[index].push_floating(id, rect);
                None
            }
        };
        self.settle();

        let role = self.screens[index].role();
        self.remember(id, back.map(|slot| RememberedPlace::in_slot(role, slot)));
        self.mark_focused(id);
        Ok(())
    }

    /// Maximizes a window, or restores it if it is maximized: the gesture
    /// of the shell's own title bar and capsules.
    pub fn toggle_maximized(&mut self, id: WindowId) -> Result<(), ModelError> {
        if self.is_maximized(id) {
            self.unmaximize(id)
        } else {
            self.maximize(id)
        }
    }

    /// Hides a window until it is restored or shown again. Its place is kept;
    /// in a stack the next tab shows, and the focus goes to the window that
    /// had it before.
    pub fn minimize(&mut self, id: WindowId) -> Result<(), ModelError> {
        let (index, _) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        self.ensure_not_anchored(index)?;
        let was_focused = self.focused() == Some(id);
        if !self.minimized.contains(&id) {
            self.minimized.push(id);
        }
        // Its stack shows another tab; that tab takes the focus the window
        // had, else the window focused before does.
        let shown = self.settle();
        if was_focused {
            let successor = shown.or_else(|| self.focused());
            if let Some(successor) = successor {
                self.mark_focused(successor);
            }
        }
        Ok(())
    }

    /// Brings back the window minimized last, focused, where it was.
    pub fn restore_last(&mut self) -> Option<WindowId> {
        let id = self.minimized.pop()?;
        self.mark_focused(id);
        Some(id)
    }

    /// The gaps between the columns of a screen, left to right, relative
    /// to the screen. An anchored screen has none to grab.
    pub fn separators(&self, output: OutputId) -> Vec<Rect> {
        match self.screen(output) {
            Some(screen) if screen.anchored().is_none() => screen.separators(&self.metrics),
            _ => Vec::new(),
        }
    }

    /// A window entered or left fullscreen on the screen it is on. While
    /// anchored, it covers its whole output and no shell gesture applies to
    /// that screen.
    pub fn set_anchored(&mut self, id: WindowId, anchored: bool) -> Result<(), ModelError> {
        if anchored {
            return self.set_fullscreen(id, None);
        }
        let (screen, _) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        if self.screens[screen].anchored() == Some(id) {
            self.screens[screen].set_anchored(None);
            self.forget_over_game();
        }
        Ok(())
    }

    /// Windows only stay over a game while there is one.
    fn forget_over_game(&mut self) {
        let screens = &self.screens;
        self.over_game.retain(|id| {
            screens.iter().any(|screen| {
                screen.anchored().is_some_and(|game| game != *id) && screen.contains(*id)
            })
        });
    }

    /// A window goes fullscreen on `output`, or on its own screen when it
    /// names none: that screen is anchored, and the window takes the focus.
    ///
    /// A window sent to another screen floats there once it leaves
    /// fullscreen. Its application asked for the move, not the user: habits
    /// are left alone. A window that already anchored the target screen
    /// gives it up.
    pub fn set_fullscreen(
        &mut self,
        id: WindowId,
        output: Option<OutputId>,
    ) -> Result<(), ModelError> {
        let (from, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        let to = match output {
            Some(output) => self.screen_index(output)?,
            None => from,
        };
        if to != from {
            self.take(from, id);
            let size = match spot {
                Spot::Floating(rect) => Some(rect.size()),
                Spot::Slot(_) | Spot::Maximized => None,
            };
            self.open_floating(id, to, size);
            self.settle();
        }
        self.screens[to].set_anchored(Some(id));
        self.mark_focused(id);
        Ok(())
    }

    /// The fullscreen window the user means when asking to put "the game"
    /// back in a window: the focused one, else the one of the screen the
    /// pointer is on, else the first one found, center first.
    pub fn game_to_release(&self, cursor: OutputId) -> Option<WindowId> {
        let anchors = |screen: &Screen| screen.anchored();
        let focused = self.focused().filter(|id| {
            self.screens
                .iter()
                .any(|screen| anchors(screen) == Some(*id))
        });
        let under_cursor = || self.screen(cursor).and_then(anchors);
        let any = || {
            let center = self
                .screens
                .iter()
                .filter(|screen| screen.role() == OutputRole::Center);
            center.chain(&self.screens).find_map(anchors)
        };
        focused.or_else(under_cursor).or_else(any)
    }

    /// The screen of the game being played: the one anchored by the window
    /// that has the focus.
    pub fn game_in_play(&self) -> Option<OutputId> {
        let focused = self.focused()?;
        let screen = self
            .screens
            .iter()
            .find(|screen| screen.anchored() == Some(focused))?;
        Some(screen.id())
    }

    /// "To the center" applies to `id`: it is on a side screen, there is a
    /// center, and no game holds either of them.
    pub fn can_send_to_center(&self, id: WindowId) -> bool {
        let Some((side, _)) = self.locate(id) else {
            return false;
        };
        let free = |screen: &Screen| screen.anchored().is_none();
        let side = &self.screens[side];
        let mut centers = self
            .screens
            .iter()
            .filter(|screen| screen.role() == OutputRole::Center);
        side.role().is_side() && free(side) && centers.next().is_some_and(free)
    }

    /// Where `dragged` can be released (rule 2): the columns of each screen,
    /// shown as they would be with one more on the right while the layout
    /// has room, that one free. Anchored screens offer nothing.
    pub fn drop_zones(&self, dragged: WindowId) -> Vec<DropZone> {
        let mut zones = Vec::new();
        for screen in &self.screens {
            if screen.anchored().is_some() {
                continue;
            }
            let count = screen.slots().len() + usize::from(screen.has_room());
            for (index, rect) in screen.columns(count, &self.metrics).into_iter().enumerate() {
                let stack = screen.slots().get(index);
                zones.push(DropZone {
                    slot: SlotId::new(screen.id(), index),
                    rect,
                    occupied: stack.is_some_and(|stack| {
                        stack.windows().iter().any(|window| *window != dragged)
                    }),
                });
            }
        }
        zones
    }

    /// Says whether a window draws its own title bar.
    pub fn set_own_titlebar(&mut self, id: WindowId, own: bool) -> Result<(), ModelError> {
        let record = self
            .windows
            .get_mut(&id)
            .ok_or(ModelError::UnknownWindow(id))?;
        record.own_titlebar = own;
        Ok(())
    }

    /// The capsules to show: on each screen, the group of the maximized
    /// windows first, if any, then one group above each occupied slot.
    /// Anchored screens have none.
    pub fn capsule_groups(&self) -> Vec<CapsuleGroup> {
        let mut groups = Vec::new();
        for screen in &self.screens {
            if screen.anchored().is_some() {
                continue;
            }
            let covered = screen.covered();
            let shown = |stack: &TabStack, showing: bool| {
                let active = stack.active().filter(|_| showing);
                active.filter(|window| !self.minimized.contains(window))
            };
            let maximized = screen.maximized();
            if !maximized.is_empty() {
                groups.push(CapsuleGroup {
                    stack: StackId::Maximized(screen.id()),
                    band: screen.maximized_parts(&self.metrics).0,
                    windows: maximized.windows().to_vec(),
                    shown: shown(maximized, covered),
                    maximized: true,
                });
            }
            // The only column has the whole screen: its group sits where
            // that of maximized windows would.
            let whole = screen.whole_column();
            let bands = if whole {
                vec![screen.maximized_parts(&self.metrics).0]
            } else {
                screen.slot_bands(&self.metrics)
            };
            for (index, (stack, band)) in screen.slots().iter().zip(bands).enumerate() {
                if stack.is_empty() {
                    continue;
                }
                groups.push(CapsuleGroup {
                    stack: StackId::Slot(SlotId::new(screen.id(), index)),
                    band,
                    windows: stack.windows().to_vec(),
                    shown: shown(stack, !covered),
                    maximized: whole,
                });
            }
        }
        groups
    }

    /// Every window with its geometry, screen by screen, bottom to top.
    pub fn arrangement(&self) -> Vec<WindowGeometry> {
        let mut arrangement = Vec::with_capacity(self.windows.len());
        for screen in &self.screens {
            let mut on_top = None;
            // Over the game: what the user brought there.
            let mut above = Vec::new();
            let mut add = |geometry: WindowGeometry| {
                if geometry.anchored {
                    on_top = Some(geometry);
                } else if self.over_game.contains(&geometry.id) {
                    above.push(geometry);
                } else {
                    arrangement.push(geometry);
                }
            };

            let covered = screen.covered();
            let parts = screen.slot_parts(&self.metrics);
            for (stack, (_, frame)) in screen.slots().iter().zip(parts) {
                for id in stack.windows() {
                    let shown = !covered && stack.active() == Some(*id);
                    add(self.geometry(screen, *id, frame, shown, Layer::Slot));
                }
            }
            let (_, frame) = screen.maximized_parts(&self.metrics);
            let maximized = screen.maximized();
            for id in maximized.windows() {
                let shown = covered && maximized.active() == Some(*id);
                add(self.geometry(screen, *id, frame, shown, Layer::Maximized));
            }
            for window in screen.floating() {
                add(self.geometry(screen, window.id, window.rect, true, Layer::Floating));
            }
            arrangement.extend(on_top);
            arrangement.extend(above);
        }
        arrangement
    }

    fn geometry(
        &self,
        screen: &Screen,
        id: WindowId,
        frame: Rect,
        shown: bool,
        layer: Layer,
    ) -> WindowGeometry {
        let anchored = screen.anchored() == Some(id);
        let minimized = self.minimized.contains(&id);
        let own_titlebar = self
            .windows
            .get(&id)
            .is_some_and(|record| record.own_titlebar);
        let (frame, titlebar, client) = if anchored {
            let bounds = screen.info().bounds();
            (bounds, None, bounds)
        } else if own_titlebar {
            // The client draws its bar itself: the whole frame is its own.
            (frame, None, frame)
        } else {
            let (titlebar, client) = frame.split_rows(self.metrics.titlebar_height, 0);
            (frame, Some(titlebar), client)
        };
        WindowGeometry {
            id,
            output: screen.id(),
            frame,
            titlebar,
            client,
            visible: anchored
                || (shown
                    && !minimized
                    && (screen.anchored().is_none() || self.over_game.contains(&id))),
            focused: self.focused() == Some(id),
            floating: layer == Layer::Floating,
            maximized: !anchored
                && (layer == Layer::Maximized || (layer == Layer::Slot && screen.whole_column())),
            anchored,
            minimized,
        }
    }

    fn screen_index(&self, output: OutputId) -> Result<usize, ModelError> {
        self.screens
            .iter()
            .position(|screen| screen.id() == output)
            .ok_or(ModelError::UnknownOutput(output))
    }

    fn locate(&self, id: WindowId) -> Option<(usize, Spot)> {
        self.screens.iter().enumerate().find_map(|(index, screen)| {
            let slot = screen.slot_of(id).map(Spot::Slot);
            let maximized = || screen.maximized().contains(id).then_some(Spot::Maximized);
            let floating = || screen.floating_rect(id).map(Spot::Floating);
            let spot = slot.or_else(maximized).or_else(floating)?;
            Some((index, spot))
        })
    }

    /// Takes `id` out of a screen, with where it would have gone back to.
    fn take(&mut self, screen: usize, id: WindowId) {
        self.screens[screen].remove(id);
        self.restore.remove(&id);
    }

    /// Records where the application of `id` reopens, or forgets it.
    fn remember(&mut self, id: WindowId, place: Option<RememberedPlace>) {
        let Some(app) = self.windows.get(&id).and_then(|record| record.app.clone()) else {
            return;
        };
        match place {
            Some(place) => self.memory.remember(app, place),
            None => self.memory.forget(&app),
        }
    }

    fn stack_mut(&mut self, stack: StackId) -> Result<&mut TabStack, ModelError> {
        let index = self.screen_index(stack.output())?;
        self.ensure_not_anchored(index)?;
        let screen = &mut self.screens[index];
        match stack {
            StackId::Slot(slot) => screen
                .slot_mut(slot.index)
                .ok_or(ModelError::UnknownSlot(slot)),
            StackId::Maximized(_) => Ok(screen.maximized_mut()),
        }
    }

    fn ensure_not_anchored(&self, screen: usize) -> Result<(), ModelError> {
        let screen = &self.screens[screen];
        match screen.anchored() {
            Some(_) => Err(ModelError::Anchored(screen.id())),
            None => Ok(()),
        }
    }

    /// Where a habit leads, unless its screen is unplugged or anchored.
    fn remembered_target(&self, place: RememberedPlace) -> Option<Target> {
        let index = self
            .screens
            .iter()
            .position(|screen| screen.role() == place.role)?;
        let screen = &self.screens[index];
        if screen.anchored().is_some() {
            return None;
        }
        if place.maximized {
            return Some(Target::Maximized {
                screen: index,
                back: place.slot,
            });
        }
        let slot = place.slot?;
        Some(Target::Slot {
            screen: index,
            slot,
        })
    }

    /// Puts `id` back in column `slot` of a screen, where it was or where its
    /// habit says: as a tab when the column is there and the window was not
    /// `alone` in it, else in a column opened at that place while there is
    /// room, else as a tab in the nearest column. Returns its column.
    fn put_back(&mut self, screen: usize, id: WindowId, slot: usize, alone: bool) -> usize {
        let screen = &mut self.screens[screen];
        let count = screen.slots().len();
        if slot < count && !(alone && screen.has_room()) {
            screen.push_to_slot(slot, id);
            slot
        } else if screen.has_room() {
            screen.insert_slot(slot, id)
        } else {
            let slot = slot.min(count - 1);
            screen.push_to_slot(slot, id);
            slot
        }
    }

    /// The screen a floating window opens on (rules 1 and 7).
    fn floating_screen(&self, cursor: usize) -> usize {
        let is_free = |screen: &Screen| screen.anchored().is_none();
        let game_at_center = self
            .screens
            .iter()
            .any(|screen| screen.role() == OutputRole::Center && !is_free(screen));
        if !game_at_center && is_free(&self.screens[cursor]) {
            return cursor;
        }

        let position = |wanted: &dyn Fn(&Screen) -> bool| {
            self.screens
                .iter()
                .position(|screen| is_free(screen) && wanted(screen))
        };
        position(&|screen| Some(screen.id()) == self.last_side)
            .or_else(|| position(&|screen| screen.role().is_side()))
            .or_else(|| position(&|_| true))
            .unwrap_or(cursor)
    }

    /// Adds `id` floating on a screen, where [`Desk::opening_rect`] says.
    fn open_floating(&mut self, id: WindowId, screen: usize, preferred: Option<Size>) -> Placement {
        let rect = self.opening_rect(screen, preferred);
        let screen = &mut self.screens[screen];
        screen.push_floating(id, rect);
        Placement::Floating {
            output: screen.id(),
            rect,
        }
    }

    /// Where a window opens floating on a screen: centered, at `preferred`
    /// size when it fits.
    fn opening_rect(&self, screen: usize, preferred: Option<Size>) -> Rect {
        self.opening_rect_over(screen, preferred, None)
    }

    /// Where a window opens floating on a screen: centered on `parent` (the
    /// window a dialog belongs to) when it is on that screen, else on the
    /// screen, at `preferred` size when it fits. A floating window already
    /// centered there moves it down and to the right, so that the new one
    /// never hides it; the parent itself does not count.
    ///
    /// Centers are compared, not corners: a client that then chooses its own
    /// size stays centered where it opened.
    fn opening_rect_over(
        &self,
        screen: usize,
        preferred: Option<Size>,
        parent: Option<WindowId>,
    ) -> Rect {
        /// How far each window opened at the same place is moved from the
        /// one before.
        const CASCADE: i32 = 32;
        /// Past this many, windows pile up where the cascade stopped.
        const STEPS: usize = 12;

        let area = self.screens[screen].floating_area(&self.metrics);
        let size = preferred.unwrap_or(Size::new(area.w * 3 / 5, area.h * 7 / 10));
        let size = Size::new(
            size.w.clamp(1, area.w.max(1)),
            size.h.clamp(1, area.h.max(1)),
        );
        let over = parent.and_then(|parent| self.frame_on(screen, parent));
        let mut rect = Rect::centered(size, over.unwrap_or(area)).clamped_to(area);
        let center = |rect: Rect| (rect.x + rect.w / 2, rect.y + rect.h / 2);
        let others = self.screens[screen].floating().iter();
        let others: Vec<Rect> = others
            .filter(|window| Some(window.id) != parent)
            .map(|window| window.rect)
            .collect();
        for _ in 0..STEPS {
            let (x, y) = center(rect);
            let taken = others.iter().any(|other| {
                let (ox, oy) = center(*other);
                (x - ox).abs() < CASCADE / 2 && (y - oy).abs() < CASCADE / 2
            });
            let next = rect.translated(Point::new(CASCADE, CASCADE));
            if !taken || next.right() > area.right() || next.bottom() > area.bottom() {
                break;
            }
            rect = next;
        }
        rect
    }

    /// The frame of `id`, relative to its screen, if it is on `screen`.
    fn frame_on(&self, screen: usize, id: WindowId) -> Option<Rect> {
        let output = self.screens[screen].id();
        self.arrangement()
            .into_iter()
            .find(|geometry| geometry.id == id && geometry.output == output)
            .map(|geometry| geometry.frame)
    }

    /// Records `id` as focused, shows its tab or raises it, and notes its
    /// side screen as used. `id` must be a known window.
    fn mark_focused(&mut self, id: WindowId) {
        self.focus_order.retain(|window| *window != id);
        self.focus_order.push(id);
        self.minimized.retain(|window| *window != id);
        // The game is back in front of what was brought over it.
        if let Some(screen) = self
            .screens
            .iter()
            .find(|screen| screen.anchored() == Some(id))
        {
            self.over_game.retain(|window| !screen.contains(*window));
        }

        if let Some((index, spot)) = self.locate(id) {
            let screen = &mut self.screens[index];
            match spot {
                Spot::Slot(slot) => {
                    if let Some(stack) = screen.slot_mut(slot) {
                        stack.activate(id);
                    }
                    screen.set_covered(false);
                }
                Spot::Maximized => {
                    screen.maximized_mut().activate(id);
                    screen.set_covered(true);
                }
                Spot::Floating(_) => screen.raise(id),
            }
            if screen.role().is_side() {
                self.last_side = Some(screen.id());
            }
        }
    }

    /// Makes every stack show a tab that is not minimized, when it has one;
    /// maximized windows stop covering their screen when none shows.
    /// Returns the tab that came to show in place of a minimized one, if any.
    fn settle(&mut self) -> Option<WindowId> {
        let minimized = &self.minimized;
        let mut shown = None;
        for screen in &mut self.screens {
            screen.compact();
            for slot in 0..screen.slots().len() {
                if let Some(next) = successor(&screen.slots()[slot], minimized) {
                    if let Some(stack) = screen.slot_mut(slot) {
                        stack.activate(next);
                        shown = Some(next);
                    }
                }
            }
            if let Some(next) = successor(screen.maximized(), minimized) {
                screen.maximized_mut().activate(next);
                shown = Some(next);
            }
            let maximized = screen.maximized().active();
            if !maximized.is_some_and(|window| !minimized.contains(&window)) {
                screen.set_covered(false);
            }
        }
        shown
    }

    /// Makes sure the focused window is not hidden behind another tab.
    fn show_focused(&mut self) {
        if let Some(id) = self.focused() {
            self.mark_focused(id);
        }
    }

    #[cfg(test)]
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        let mut seen = BTreeMap::new();
        for screen in &self.screens {
            let output = screen.id();
            if screen.slots().len() > screen.layout().max_columns() {
                return Err(format!("{output:?} has more columns than its layout"));
            }
            if screen.slots().iter().any(TabStack::is_empty) {
                return Err(format!("{output:?} shows an empty column"));
            }
            let stacks = || screen.slots().iter().chain([screen.maximized()]);
            if stacks().any(|s| !s.is_empty() && s.active().is_none()) {
                return Err(format!("{output:?}: a stack has no active tab"));
            }
            let showing = screen.maximized().active();
            if screen.covered() && !showing.is_some_and(|id| !self.minimized.contains(&id)) {
                return Err(format!("{output:?} is covered by no maximized window"));
            }
            let area = screen.floating_area(&self.metrics);
            for window in screen.floating() {
                if window.rect.is_empty() || window.rect != window.rect.clamped_to(area) {
                    return Err(format!("{:?} floats outside {output:?}", window.id));
                }
            }
            if screen.anchored().is_some_and(|id| !screen.contains(id)) {
                return Err(format!(
                    "{output:?} is anchored by a window it does not hold"
                ));
            }
            for id in screen.windows() {
                *seen.entry(id).or_insert(0) += 1;
            }
        }

        if seen.values().any(|count| *count != 1) {
            return Err("a window is in several places".into());
        }
        if !seen.keys().eq(self.windows.keys()) {
            return Err("screens and records disagree on which windows exist".into());
        }
        let mut maximized: Vec<_> = self
            .screens
            .iter()
            .flat_map(|screen| screen.maximized().windows().iter().copied())
            .collect();
        maximized.sort();
        if !maximized.iter().eq(self.restore.keys()) {
            return Err("maximized windows and where they go back disagree".into());
        }
        let mut order = self.focus_order.clone();
        order.sort();
        if !order.iter().eq(self.windows.keys()) {
            return Err("the focus order does not hold every window once".into());
        }

        let mut hidden = self.minimized.clone();
        hidden.sort();
        hidden.dedup();
        if hidden.len() != self.minimized.len()
            || hidden.iter().any(|id| !self.windows.contains_key(id))
        {
            return Err("the minimized windows are not a set of known windows".into());
        }
        for screen in &self.screens {
            for stack in screen.slots().iter().chain([screen.maximized()]) {
                let is_hidden = |id: &WindowId| self.minimized.contains(id);
                let stuck = stack.active().is_some_and(|id| is_hidden(&id))
                    && !stack.windows().iter().all(is_hidden);
                if stuck {
                    return Err(format!("{:?} shows a minimized tab", screen.id()));
                }
            }
        }

        let arrangement = self.arrangement();
        let focused: Vec<_> = arrangement.iter().filter(|g| g.focused).collect();
        let hidden = |id: &WindowId| self.minimized.contains(id) || self.under_a_game(*id);
        match focused.as_slice() {
            [] if self.windows.keys().all(hidden) => {}
            [one] => {
                if !one.visible {
                    return Err(format!("focused {:?} cannot be seen", one.id));
                }
            }
            _ => return Err(format!("{} windows are focused", focused.len())),
        }
        Ok(())
    }
}

/// The tab a stack shows instead of its active one, when that one is
/// minimized: the next one that is not, after it first, then around to the
/// ones before.
fn successor(stack: &TabStack, minimized: &[WindowId]) -> Option<WindowId> {
    let position = stack.active_index()?;
    let windows = stack.windows();
    if !minimized.contains(&windows[position]) {
        return None;
    }
    (1..windows.len())
        .map(|step| windows[(position + step) % windows.len()])
        .find(|window| !minimized.contains(window))
}
