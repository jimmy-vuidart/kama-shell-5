//! X11 applications, through XWayland: the compositor starts the X server
//! and acts as its window manager.
//!
//! Ordinary X11 windows are placed by the model like Wayland ones. Windows
//! that manage themselves (menus, tooltips, dropdowns: "override-redirect")
//! are only shown where they ask, above everything, and receive input.

use std::fs::File;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Stdio;

use coquille_model::{OutputId, OutputRole, Placement, Rect, Size as ModelSize, WindowId};
use smithay::desktop::Window;
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::selection::SelectionTarget;
use smithay::wayland::selection::data_device::{
    clear_data_device_selection, current_data_device_selection_userdata,
    request_data_device_client_selection, set_data_device_selection,
};
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge, WmWindowProperty, WmWindowType, XwmId};
use smithay::xwayland::{X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler};
use tracing::{debug, info, warn};

use crate::shell::{NewWindow, ScreenInfo};
use crate::state::State;

/// Whether an X11 window follows the habits of its application: a main
/// window does, a dialog, a splash screen or a toolbox does not.
pub fn follows_habits(kind: Option<WmWindowType>, transient: bool) -> bool {
    !transient && matches!(kind, None | Some(WmWindowType::Normal))
}

/// The screen an X11 window covers exactly, which is how games say where
/// they want to be fullscreen.
pub fn covered_screen(window: Rect, screens: &[ScreenInfo]) -> Option<OutputId> {
    screens
        .iter()
        .find(|screen| screen.rect == window)
        .map(|screen| screen.id)
}

/// The value of `DISPLAY` for an X server number.
pub fn display_name(number: u32) -> String {
    format!(":{number}")
}

fn model_rect(rect: Rectangle<i32, Logical>) -> Rect {
    Rect::new(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h)
}

impl State {
    /// Starts the X server. Its display number is known at once, so that
    /// programs started from now on find it; windows are managed as soon as
    /// it is ready. `log` receives what the X server has to say.
    pub fn start_xwayland(&mut self, log: Option<&Path>) {
        let errors = log
            .and_then(|path| File::create(path).ok())
            .map_or_else(Stdio::null, Stdio::from);
        let spawned = XWayland::spawn(
            &self.display_handle,
            None,
            std::iter::empty::<(String, String)>(),
            std::iter::empty::<String>(),
            true,
            Stdio::null(),
            errors,
            |_| (),
        );
        let (xwayland, client) = match spawned {
            Ok(spawned) => spawned,
            Err(error) => {
                warn!(%error, "cannot start XWayland: X11 applications will not run");
                return;
            }
        };
        let number = xwayland.display_number();
        let watched =
            self.loop_handle
                .insert_source(xwayland, move |event, _, state| match event {
                    XWaylandEvent::Ready { x11_socket, .. } => {
                        state.xwayland_ready(x11_socket, client.clone());
                    }
                    XWaylandEvent::Error => {
                        warn!("XWayland stopped while starting: X11 applications will not run");
                        state.xdisplay = None;
                    }
                });
        match watched {
            Ok(_) => {
                info!(display = display_name(number), "XWayland is starting");
                self.xdisplay = Some(number);
            }
            Err(error) => warn!(%error, "cannot watch XWayland"),
        }
    }

    /// The X server runs: becomes its window manager.
    fn xwayland_ready(&mut self, socket: UnixStream, client: Client) {
        let started = X11Wm::start_wm(
            self.loop_handle.clone(),
            &self.display_handle,
            socket,
            client,
        );
        let mut xwm = match started {
            Ok(xwm) => xwm,
            Err(error) => {
                warn!(%error, "cannot manage X11 windows: X11 applications will not run");
                self.xdisplay = None;
                return;
            }
        };

        // What X11 clients show when they set no pointer image themselves.
        if let Some(image) = self.cursor_images.default_image() {
            let size = (image.width as u16, image.height as u16).into();
            let hotspot = (image.xhot as u16, image.yhot as u16).into();
            if let Err(error) = xwm.set_cursor(&image.pixels_rgba, size, hotspot) {
                debug!(%error, "cannot set the default X11 cursor");
            }
        }
        // Games ask X11 which screen is the main one: the center.
        let center = self
            .shell
            .screens
            .iter()
            .find(|screen| screen.role == OutputRole::Center)
            .and_then(|screen| self.output(screen.id));
        if let Some(center) = center {
            if let Err(error) = xwm.set_randr_primary_output(Some(center)) {
                debug!(?error, "cannot name the main screen to X11");
            }
            // Smithay ignores the X11 events that answer the request above,
            // which it tells by their sequence number: every event carries
            // it until the next request, so one must follow at once or
            // windows would never be seen.
            match xwm.get_randr_primary_output() {
                Ok(name) => debug!(?name, "main screen for X11"),
                Err(error) => debug!(%error, "cannot read the main screen of X11"),
            }
        }

        info!(display = ?self.xdisplay.map(display_name), "XWayland is ready");
        self.xwm = Some(xwm);
    }

    /// An X11 window asks to be shown: the model places it.
    fn manage_x11(&mut self, window: X11Surface) {
        let asked = model_rect(window.last_configure());
        let wanted = window.geometry().size;
        let parent = window.is_transient_for().and_then(|parent| {
            let known = |managed: &&crate::shell::ManagedWindow| {
                managed
                    .window
                    .x11_surface()
                    .is_some_and(|other| other.window_id() == parent)
            };
            self.shell.windows.iter().find(known)
        });
        // A game that starts fullscreen says on which screen by covering it.
        let fullscreen = window
            .is_fullscreen()
            .then(|| covered_screen(asked, &self.shell.screens));
        self.manage(NewWindow {
            app_id: window.class(),
            title: window.title(),
            habits: follows_habits(window.window_type(), window.is_transient_for().is_some()),
            // A dialog opens on the screen of its window.
            screen: parent.map(|parent| parent.geometry.output),
            size: Some(ModelSize::new(wanted.w, wanted.h)),
            fullscreen,
            // It said it wants no decoration from the window manager (Steam
            // and others that draw their own title bar).
            own_titlebar: window.is_decorated(),
            window: Window::new_x11_window(window),
        });
    }

    /// An X11 window is hidden or gone.
    fn forget_x11(&mut self, window: &X11Surface) {
        let before = self.shell.overrides.len();
        self.shell.overrides.retain(|other| other != window);
        if self.shell.overrides.len() != before {
            self.refresh_pointer_focus();
            self.queue_redraw();
        }
        if let Some(id) = self.shell.by_x11(window).map(|managed| managed.id) {
            self.remove_window(id);
        }
    }

    fn x11_id(&self, window: &X11Surface) -> Option<WindowId> {
        self.shell.by_x11(window).map(|managed| managed.id)
    }

    /// Forgets the windows of the X server, which is gone.
    fn drop_x11_windows(&mut self) {
        self.shell.overrides.clear();
        let gone: Vec<_> = self
            .shell
            .windows
            .iter()
            .filter(|managed| managed.window.x11_surface().is_some())
            .map(|managed| managed.id)
            .collect();
        for id in gone {
            self.remove_window(id);
        }
    }
}

impl XWaylandShellHandler for State {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    /// The X11 window got its surface: it can be drawn, focused and pointed
    /// at from now on.
    fn surface_associated(&mut self, _xwm: XwmId, _surface: WlSurface, window: X11Surface) {
        if self.x11_id(&window).is_some() {
            self.apply_arrangement();
        }
        self.refresh_pointer_focus();
        self.queue_redraw();
    }
}

impl XwmHandler for State {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        // X11 events only come from the source the window manager created,
        // and it is never taken out once stored.
        self.xwm
            .as_mut()
            .expect("an X11 event without a window manager")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(error) = window.set_mapped(true) {
            warn!(%error, "cannot show an X11 window");
            return;
        }
        self.manage_x11(window);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.shell.overrides.retain(|other| *other != window);
        self.shell.overrides.push(window);
        self.queue_redraw();
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.forget_x11(&window);
        if !window.is_override_redirect() {
            if let Err(error) = window.set_mapped(false) {
                debug!(%error, "cannot hide an X11 window");
            }
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.forget_x11(&window);
    }

    /// A window wants another size or place. The place is the shell's to
    /// decide; the size is granted to a floating window, as it is to a
    /// Wayland one, and to a window not shown yet.
    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let width = w.and_then(|w| i32::try_from(w).ok());
        let height = h.and_then(|h| i32::try_from(h).ok());
        let Some(managed) = self.shell.by_x11(&window) else {
            let mut rect = window.last_configure();
            rect.loc.x = x.unwrap_or(rect.loc.x);
            rect.loc.y = y.unwrap_or(rect.loc.y);
            rect.size.w = width.unwrap_or(rect.size.w);
            rect.size.h = height.unwrap_or(rect.size.h);
            if let Err(error) = window.configure(rect) {
                debug!(%error, "cannot configure an X11 window");
            }
            return;
        };

        let (id, geometry) = (managed.id, managed.geometry);
        let resized = match self.shell.desk.placement(id) {
            Some(Placement::Floating { output, rect }) if !geometry.anchored => {
                // What the window asks for includes its own shadows.
                let shadows = window.frame_extents();
                let bar = geometry.frame.h - geometry.client.h;
                let wanted = Rect::new(
                    rect.x,
                    rect.y,
                    width.map_or(rect.w, |w| w - shadows.left - shadows.right),
                    height.map_or(rect.h, |h| h - shadows.top - shadows.bottom + bar),
                );
                // The model keeps the window on its screen: it may refuse
                // part of the size, or all of it.
                wanted != rect
                    && !wanted.is_empty()
                    && self.shell.desk.set_floating_rect(id, wanted).is_ok()
                    && self.shell.desk.placement(id) != Some(Placement::Floating { output, rect })
            }
            _ => false,
        };
        if resized {
            self.apply_arrangement();
        } else if let Err(error) = window.configure(None) {
            // Told again where it is: the answer to a request not granted.
            debug!(%error, "cannot configure an X11 window");
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        // A menu moved or changed size by itself.
        if window.is_override_redirect() {
            self.queue_redraw();
        }
    }

    fn property_notify(&mut self, _xwm: XwmId, window: X11Surface, property: WmWindowProperty) {
        let Some(id) = self.x11_id(&window) else {
            return;
        };
        match property {
            WmWindowProperty::Title | WmWindowProperty::Class => {
                self.rename_window(id, window.class(), window.title());
            }
            // Its shadows changed: so does the room it needs around it.
            WmWindowProperty::FrameExtents => self.apply_arrangement(),
            _ => {}
        }
    }

    /// Fullscreen anchors the screen (game mode), as for Wayland windows.
    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.x11_id(&window) {
            self.enter_fullscreen(id, None);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.x11_id(&window) {
            self.leave_fullscreen(id);
        }
    }

    /// Games ask to be minimized when they lose the focus; a fullscreen one
    /// stays where it is.
    fn minimize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        let Some(managed) = self.shell.by_x11(&window) else {
            return;
        };
        if !managed.geometry.anchored {
            self.minimize_window(managed.id);
        }
    }

    fn unminimize_request(&mut self, _xwm: XwmId, window: X11Surface) {
        let Some(managed) = self.shell.by_x11(&window) else {
            return;
        };
        if managed.geometry.minimized && self.shell.desk.focus(managed.id).is_ok() {
            self.apply_arrangement();
        }
    }

    // Resizing belongs to the shell: the edges of a floating window.
    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _edge: ResizeEdge,
    ) {
    }

    /// The user drags the title bar the application draws itself.
    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        if let Some(id) = self.shell.by_x11(&window).map(|managed| managed.id) {
            self.drag_from_client(id);
        }
    }

    /// The clipboard is shared with the X11 window that has the focus.
    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        let focus = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus());
        focus
            .as_ref()
            .and_then(|focus| focus.x11())
            .is_some_and(|window| window.xwm_id() == Some(xwm))
    }

    /// An X11 client pastes what a Wayland client copied.
    fn send_selection(
        &mut self,
        _xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        if selection != SelectionTarget::Clipboard {
            return;
        }
        if let Err(error) = request_data_device_client_selection(&self.seat, mime_type, fd) {
            debug!(%error, "cannot hand the clipboard to an X11 client");
        }
    }

    /// An X11 client copied something: Wayland clients may paste it.
    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        if selection == SelectionTarget::Clipboard {
            set_data_device_selection(&self.display_handle, &self.seat, mime_types, ());
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        if selection == SelectionTarget::Clipboard
            && current_data_device_selection_userdata(&self.seat).is_some()
        {
            clear_data_device_selection(&self.display_handle, &self.seat);
        }
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        warn!("XWayland stopped: X11 applications will not run any more");
        self.xdisplay = None;
        self.drop_x11_windows();
    }
}

#[cfg(test)]
mod tests {
    use coquille_model::OutputRole;

    use super::*;

    fn screens() -> Vec<ScreenInfo> {
        let screen = |id, role, rect| ScreenInfo {
            id: OutputId(id),
            role,
            rect,
        };
        vec![
            screen(1, OutputRole::Left, Rect::new(0, 0, 1920, 1080)),
            screen(2, OutputRole::Center, Rect::new(1920, 0, 3440, 1440)),
            screen(3, OutputRole::Right, Rect::new(5360, 0, 1920, 1080)),
        ]
    }

    #[test]
    fn a_main_window_follows_the_habits_of_its_application() {
        assert!(follows_habits(None, false));
        assert!(follows_habits(Some(WmWindowType::Normal), false));
    }

    #[test]
    fn dialogs_and_other_companions_have_no_habit() {
        assert!(!follows_habits(Some(WmWindowType::Dialog), false));
        assert!(!follows_habits(Some(WmWindowType::Splash), false));
        assert!(!follows_habits(Some(WmWindowType::Utility), false));
        assert!(!follows_habits(Some(WmWindowType::Normal), true));
        assert!(!follows_habits(None, true));
    }

    #[test]
    fn a_window_that_covers_a_screen_names_it() {
        let screens = screens();

        assert_eq!(
            covered_screen(Rect::new(1920, 0, 3440, 1440), &screens),
            Some(OutputId(2))
        );
        assert_eq!(
            covered_screen(Rect::new(5360, 0, 1920, 1080), &screens),
            Some(OutputId(3))
        );
    }

    #[test]
    fn a_window_that_covers_no_screen_exactly_names_none() {
        let screens = screens();

        assert_eq!(covered_screen(Rect::new(0, 0, 800, 600), &screens), None);
        assert_eq!(covered_screen(Rect::new(10, 0, 1920, 1080), &screens), None);
        assert_eq!(covered_screen(Rect::new(0, 0, 1920, 1080), &[]), None);
    }

    #[test]
    fn the_display_is_named_with_a_colon() {
        assert_eq!(display_name(0), ":0");
        assert_eq!(display_name(12), ":12");
    }
}
