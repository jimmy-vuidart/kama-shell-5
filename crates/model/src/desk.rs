//! The whole desk: every screen, every window, and the placement rules.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::geometry::{Insets, Point, Rect, Size};
use crate::ids::{AppKey, OutputId, WindowId};
use crate::layout::{LayoutKind, Ratio, SlotId};
use crate::memory::{AppMemory, RememberedPlace};
use crate::metrics::Metrics;
use crate::output::{OutputInfo, OutputRole};
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
    #[error("layout {kind:?} is not allowed on the {role:?} screen")]
    LayoutNotAllowed { kind: LayoutKind, role: OutputRole },
    #[error("slot {slot:?} has no tab {index}")]
    UnknownTab { slot: SlotId, index: usize },
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
    Floating { output: OutputId, rect: Rect },
    Slot(SlotId),
}

/// Context of a window opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenRequest {
    /// The output the cursor is on.
    pub cursor_output: OutputId,
    /// Size the window asks for, decorations included.
    pub preferred_size: Option<Size>,
}

impl OpenRequest {
    pub const fn at(cursor_output: OutputId) -> Self {
        Self {
            cursor_output,
            preferred_size: None,
        }
    }

    pub const fn with_size(mut self, size: Size) -> Self {
        self.preferred_size = Some(size);
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

/// Where a dragged window can be released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    Slot(SlotId),
    /// Anywhere on a `Free` screen.
    Float(OutputId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropZone {
    pub target: DropTarget,
    pub rect: Rect,
    /// Another window is already there: releasing stacks as a tab.
    pub occupied: bool,
}

/// The capsules above a slot: one per window of its stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleGroup {
    pub slot: SlotId,
    /// Where the capsules go, relative to the screen.
    pub band: Rect,
    /// In the order of the stack.
    pub windows: Vec<WindowId>,
    /// The window the slot shows; `None` when they are all minimized.
    pub shown: Option<WindowId>,
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
    pub anchored: bool,
    /// Hidden by the user, until restored.
    pub minimized: bool,
}

/// Where a window is inside its screen.
#[derive(Debug, Clone, Copy)]
enum Spot {
    Slot(usize),
    Floating(Rect),
}

/// Where a remembered application reopens. Indices are into `Desk::screens`.
enum Target {
    Slot { screen: usize, slot: usize },
    Float { screen: usize },
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
    last_side: Option<OutputId>,
    /// Screens asked for the next window of an application, once each. An
    /// application is asked for under every name its windows may bear.
    asked: Vec<(Vec<AppKey>, OutputId)>,
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
            last_side: None,
            asked: Vec::new(),
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
        })
    }

    /// The window with the focus: the one focused last that can be seen,
    /// that is neither minimized nor under a fullscreen window.
    pub fn focused(&self) -> Option<WindowId> {
        let shown = |id: &&WindowId| !self.minimized.contains(id) && !self.under_a_game(**id);
        self.focus_order.iter().rev().find(shown).copied()
    }

    /// `id` is on a screen that another window anchors.
    fn under_a_game(&self, id: WindowId) -> bool {
        self.screens
            .iter()
            .any(|screen| screen.anchored().is_some_and(|game| game != id) && screen.contains(id))
    }

    /// A new window appears (rules 1, 4 and 7) and takes the focus, unless a
    /// game is being played: the game keeps it, and the new window, shown on
    /// another screen, is next in line.
    ///
    /// An application the user tidied reopens in its slot. Anything else opens
    /// floating on the cursor's screen, or, when a game is anchored, on the
    /// side screen used last. No other window moves.
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

        let remembered = app.as_ref().and_then(|app| self.memory.get(app));
        let remembered = remembered.filter(|_| wanted.is_none());
        let placement = match remembered.and_then(|place| self.remembered_target(place)) {
            Some(Target::Slot { screen, slot }) => {
                if let Some(stack) = self.screens[screen].slot_mut(slot) {
                    stack.push(id);
                }
                Placement::Slot(SlotId::new(self.screens[screen].id(), slot))
            }
            Some(Target::Float { screen }) => {
                self.open_floating(id, screen, request.preferred_size)
            }
            None => {
                let free = wanted.filter(|screen| self.screens[*screen].anchored().is_none());
                let screen = free.unwrap_or_else(|| self.floating_screen(cursor));
                self.open_floating(id, screen, request.preferred_size)
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

        self.screens[screen].remove(id);
        self.windows.remove(&id);
        self.focus_order.retain(|window| *window != id);
        self.minimized.retain(|window| *window != id);
        self.settle();

        if was_focused {
            let neighbour = match spot {
                Spot::Slot(index) => self.screens[screen].slots()[index].active(),
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

    /// The user tidies a window into a slot (rules 2, 3 and 4).
    ///
    /// An occupied slot never refuses: the window stacks as a tab. The
    /// application will reopen there.
    pub fn move_to_slot(&mut self, id: WindowId, slot: SlotId) -> Result<(), ModelError> {
        let (from, _) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        let to = self.screen_index(slot.output)?;
        self.ensure_not_anchored(from)?;
        self.ensure_not_anchored(to)?;
        if slot.index >= self.screens[to].slots().len() {
            return Err(ModelError::UnknownSlot(slot));
        }

        self.screens[from].remove(id);
        if let Some(stack) = self.screens[to].slot_mut(slot.index) {
            stack.push(id);
        }
        self.settle();

        let app = self.windows.get(&id).and_then(|record| record.app.clone());
        if let Some(app) = app {
            let place = RememberedPlace {
                role: self.screens[to].role(),
                slot: slot.index,
            };
            self.memory.remember(app, place);
        }
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

        self.screens[from].remove(id);
        let rect = rect.clamped_to(self.screens[to].floating_area(&self.metrics));
        self.screens[to].push_floating(id, rect);
        self.settle();

        if matches!(spot, Spot::Slot(_)) {
            if let Some(app) = self.windows.get(&id).and_then(|record| record.app.as_ref()) {
                self.memory.forget(app);
            }
        }
        self.mark_focused(id);
        Ok(())
    }

    /// Where tidying a floating window on its own screen would put it: the
    /// first empty slot, else the slot focused last. `None` when the screen
    /// has no usable slot, or when the window is already in one.
    pub fn suggested_slot(&self, id: WindowId) -> Option<SlotId> {
        let (index, spot) = self.locate(id)?;
        let screen = &self.screens[index];
        if matches!(spot, Spot::Slot(_)) || screen.anchored().is_some() {
            return None;
        }
        let slots = screen.slots();
        let empty = slots.iter().position(TabStack::is_empty);
        let recent = || {
            let others = self
                .focus_order
                .iter()
                .rev()
                .filter(|window| **window != id);
            others.clone().find_map(|window| screen.slot_of(*window))
        };
        let slot = empty.or_else(recent).or((!slots.is_empty()).then_some(0))?;
        Some(SlotId::new(screen.id(), slot))
    }

    /// A floating window's client chose another size. Unlike
    /// [`Desk::float_window`], this is not an action of the user: the focus
    /// and the habits stay as they are.
    pub fn set_floating_rect(&mut self, id: WindowId, rect: Rect) -> Result<(), ModelError> {
        let (index, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
        if matches!(spot, Spot::Slot(_)) {
            return Err(ModelError::NotFloating(id));
        }
        let screen = &mut self.screens[index];
        let rect = rect.clamped_to(screen.floating_area(&self.metrics));
        screen.set_floating_rect(id, rect);
        Ok(())
    }

    /// "To the center": the user tidies a side window into the center
    /// screen, in the slot used last there. An occupied slot stacks it as a
    /// tab (rule 3); a `Free` center takes it floating, at its size.
    ///
    /// Refused while a game is anchored on either screen.
    pub fn send_to_center(&mut self, id: WindowId) -> Result<(), ModelError> {
        let (side, spot) = self.locate(id).ok_or(ModelError::UnknownWindow(id))?;
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
        if self.screens[center].slots().is_empty() {
            let area = self.screens[center].floating_area(&self.metrics);
            let size = match spot {
                Spot::Floating(rect) => rect.size(),
                Spot::Slot(_) => Size::new(area.w * 3 / 5, area.h * 7 / 10),
            };
            self.float_window(id, output, Rect::centered(size, area))
        } else {
            self.move_to_slot(id, SlotId::new(output, self.center_slot(center)))
        }
    }

    /// Changes the layout of a screen; see [`Screen`] for how windows are
    /// redistributed. The focused window stays visible.
    pub fn set_layout(&mut self, output: OutputId, kind: LayoutKind) -> Result<(), ModelError> {
        let index = self.screen_index(output)?;
        self.ensure_not_anchored(index)?;
        let role = self.screens[index].role();
        if !kind.allowed_on(role) {
            return Err(ModelError::LayoutNotAllowed { kind, role });
        }
        self.screens[index].apply_layout(kind, &self.metrics);
        self.settle();
        self.show_focused();
        Ok(())
    }

    pub fn set_ratio(&mut self, output: OutputId, ratio: Ratio) -> Result<(), ModelError> {
        let index = self.screen_index(output)?;
        self.ensure_not_anchored(index)?;
        self.screens[index].set_ratio(ratio);
        Ok(())
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

    pub fn activate_tab(&mut self, slot: SlotId, index: usize) -> Result<(), ModelError> {
        let stack = self.stack_mut(slot)?;
        let id = stack
            .activate_index(index)
            .ok_or(ModelError::UnknownTab { slot, index })?;
        self.mark_focused(id);
        Ok(())
    }

    /// Shows and focuses the next (or previous) tab of a slot.
    pub fn cycle_tab(&mut self, slot: SlotId, forward: bool) -> Result<(), ModelError> {
        if let Some(id) = self.stack_mut(slot)?.cycle(forward) {
            self.mark_focused(id);
        }
        Ok(())
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

    /// The gap between the two slots of a screen, relative to the screen.
    /// An anchored screen has none to grab.
    pub fn separator(&self, output: OutputId) -> Option<Rect> {
        let screen = self.screen(output)?;
        if screen.anchored().is_some() {
            return None;
        }
        screen.separator(&self.metrics)
    }

    /// The ratio that puts the separation of a screen under `point`, given
    /// relative to the screen.
    pub fn ratio_at(&self, output: OutputId, point: Point) -> Option<Ratio> {
        let screen = self.screen(output)?;
        if screen.anchored().is_some() {
            return None;
        }
        screen.ratio_at(&self.metrics, point)
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
        }
        Ok(())
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
            self.screens[from].remove(id);
            let size = match spot {
                Spot::Floating(rect) => Some(rect.size()),
                Spot::Slot(_) => None,
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

    /// Where `dragged` can be released (rule 2). Anchored screens offer
    /// nothing.
    pub fn drop_zones(&self, dragged: WindowId) -> Vec<DropZone> {
        let mut zones = Vec::new();
        for screen in &self.screens {
            if screen.anchored().is_some() {
                continue;
            }
            let rects = screen.slot_rects(&self.metrics);
            if rects.is_empty() {
                zones.push(DropZone {
                    target: DropTarget::Float(screen.id()),
                    rect: screen.info().usable(),
                    occupied: false,
                });
            }
            for (index, (rect, stack)) in rects.into_iter().zip(screen.slots()).enumerate() {
                zones.push(DropZone {
                    target: DropTarget::Slot(SlotId::new(screen.id(), index)),
                    rect,
                    occupied: stack.windows().iter().any(|window| *window != dragged),
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

    /// The capsules to show: one group above each occupied slot. Anchored
    /// and `Free` screens have none.
    pub fn capsule_groups(&self) -> Vec<CapsuleGroup> {
        let mut groups = Vec::new();
        for screen in &self.screens {
            if screen.anchored().is_some() {
                continue;
            }
            let parts = screen.slot_parts(&self.metrics);
            for (index, (stack, (band, _))) in screen.slots().iter().zip(parts).enumerate() {
                if stack.is_empty() {
                    continue;
                }
                groups.push(CapsuleGroup {
                    slot: SlotId::new(screen.id(), index),
                    band,
                    windows: stack.windows().to_vec(),
                    shown: stack
                        .active()
                        .filter(|window| !self.minimized.contains(window)),
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
            let mut add = |geometry: WindowGeometry| {
                if geometry.anchored {
                    on_top = Some(geometry);
                } else {
                    arrangement.push(geometry);
                }
            };

            let parts = screen.slot_parts(&self.metrics);
            for (stack, (_, frame)) in screen.slots().iter().zip(parts) {
                for id in stack.windows() {
                    let shown = stack.active() == Some(*id);
                    add(self.geometry(screen, *id, frame, shown, false));
                }
            }
            for window in screen.floating() {
                add(self.geometry(screen, window.id, window.rect, true, true));
            }
            arrangement.extend(on_top);
        }
        arrangement
    }

    fn geometry(
        &self,
        screen: &Screen,
        id: WindowId,
        frame: Rect,
        shown: bool,
        floating: bool,
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
            visible: anchored || (shown && !minimized && screen.anchored().is_none()),
            focused: self.focused() == Some(id),
            floating,
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
            let spot = slot.or_else(|| screen.floating_rect(id).map(Spot::Floating))?;
            Some((index, spot))
        })
    }

    fn stack_mut(&mut self, slot: SlotId) -> Result<&mut TabStack, ModelError> {
        let index = self.screen_index(slot.output)?;
        self.ensure_not_anchored(index)?;
        self.screens[index]
            .slot_mut(slot.index)
            .ok_or(ModelError::UnknownSlot(slot))
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
        Some(match screen.slots().len() {
            0 => Target::Float { screen: index },
            count => Target::Slot {
                screen: index,
                slot: place.slot.min(count - 1),
            },
        })
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

    /// Adds `id` floating on a screen, centered, at `preferred` size when it
    /// fits.
    fn open_floating(&mut self, id: WindowId, screen: usize, preferred: Option<Size>) -> Placement {
        let screen = &mut self.screens[screen];
        let area = screen.floating_area(&self.metrics);
        let size = preferred.unwrap_or(Size::new(area.w * 3 / 5, area.h * 7 / 10));
        let size = Size::new(
            size.w.clamp(1, area.w.max(1)),
            size.h.clamp(1, area.h.max(1)),
        );
        let rect = Rect::centered(size, area);
        screen.push_floating(id, rect);
        Placement::Floating {
            output: screen.id(),
            rect,
        }
    }

    /// The center slot a side window is sent to: the one focused last, else
    /// the first occupied one.
    fn center_slot(&self, center: usize) -> usize {
        let screen = &self.screens[center];
        let mut recent = self.focus_order.iter().rev();
        recent
            .find_map(|window| screen.slot_of(*window))
            .or_else(|| screen.slots().iter().position(|stack| !stack.is_empty()))
            .unwrap_or(0)
    }

    /// Records `id` as focused, shows its tab or raises it, and notes its
    /// side screen as used. `id` must be a known window.
    fn mark_focused(&mut self, id: WindowId) {
        self.focus_order.retain(|window| *window != id);
        self.focus_order.push(id);
        self.minimized.retain(|window| *window != id);

        if let Some((index, spot)) = self.locate(id) {
            let screen = &mut self.screens[index];
            match spot {
                Spot::Slot(slot) => {
                    if let Some(stack) = screen.slot_mut(slot) {
                        stack.activate(id);
                    }
                }
                Spot::Floating(_) => screen.raise(id),
            }
            if screen.role().is_side() {
                self.last_side = Some(screen.id());
            }
        }
    }

    /// Makes every stack show a tab that is not minimized, when it has one.
    /// Returns the tab that came to show in place of a minimized one, if any.
    fn settle(&mut self) -> Option<WindowId> {
        let minimized = &self.minimized;
        let mut shown = None;
        for screen in &mut self.screens {
            for slot in 0..screen.slots().len() {
                let stack = &screen.slots()[slot];
                let Some(position) = stack.active_index() else {
                    continue;
                };
                let windows = stack.windows();
                if !minimized.contains(&windows[position]) {
                    continue;
                }
                // The tabs after it first, then around to the ones before.
                let next = (1..windows.len())
                    .map(|step| windows[(position + step) % windows.len()])
                    .find(|window| !minimized.contains(window));
                if let (Some(next), Some(stack)) = (next, screen.slot_mut(slot)) {
                    stack.activate(next);
                    shown = Some(next);
                }
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
            if screen.slots().len() != screen.layout().slot_count() {
                return Err(format!("{output:?}: slot count does not match the layout"));
            }
            if screen
                .slots()
                .iter()
                .any(|s| !s.is_empty() && s.active().is_none())
            {
                return Err(format!("{output:?}: a stack has no active tab"));
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
            for stack in screen.slots() {
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
