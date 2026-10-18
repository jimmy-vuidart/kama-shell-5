//! The X11 system tray (XEmbed, freedesktop "System Tray" specification):
//! Wine, Proton and other X11 programs dock their icons in it. The dock
//! holds the tray selection of the X server, keeps each icon in a hidden
//! window, reads its image, and replays the clicks of the dock on it.
//!
//! Without a tray, Wine shows its icons in a little window of its own.

use std::sync::{Arc, mpsc};

use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::errors::ReplyOrIdError;
use x11rb::protocol::Event;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::damage::{ConnectionExt as _, ReportLevel};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ButtonPressEvent, ClientMessageEvent,
    ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, EventMask, ImageFormat,
    ImageOrder, KeyButMask, PropMode, SetMode, VisualClass, Visualid, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::model::tray::TrayPixmap;

/// The size an icon gets when it asks for none that makes sense: the one
/// Wine asks for (a small Windows icon and a border of 2 pixels).
const SIZE: u16 = 20;
/// Larger than this, a tray icon is not an icon.
const MAX_SIZE: u16 = 64;

/// `SYSTEM_TRAY_REQUEST_DOCK`, the only opcode a tray must understand.
const REQUEST_DOCK: u32 = 0;
/// `XEMBED_EMBEDDED_NOTIFY`: tells a client it is embedded.
const EMBEDDED_NOTIFY: u32 = 0;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        MANAGER,
        UTF8_STRING,
        _NET_SYSTEM_TRAY_OPCODE,
        _NET_SYSTEM_TRAY_ORIENTATION,
        _NET_SYSTEM_TRAY_VISUAL,
        _NET_WM_NAME,
        _NET_WM_WINDOW_OPACITY,
        _XEMBED,
        _COQUILLE_WAKE,
        _COQUILLE_TIME,
    }
}

/// An X11 tray icon, as the dock shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct X11Icon {
    pub window: u32,
    /// The class of its program (`upc.exe`): stable, it orders the icons.
    pub class: String,
    /// Its window name, often the tooltip of the icon.
    pub name: String,
    pub pixmap: TrayPixmap,
}

/// A click of the dock, replayed on an icon.
#[derive(Debug, Clone, Copy)]
struct Click {
    window: u32,
    /// 1, 2 or 3: left, middle, right.
    button: u8,
    /// Where the click happened, in the coordinates of the whole desktop:
    /// Wine opens the menu of the icon there.
    x: i16,
    y: i16,
}

/// What the rest of the dock holds of the tray: it sends clicks.
#[derive(Clone)]
pub struct Tray {
    connection: Arc<RustConnection>,
    manager: Window,
    wake: Atom,
    clicks: mpsc::Sender<Click>,
}

impl Tray {
    /// Replays a click of `button` (1 left, 2 middle, 3 right) on an icon.
    pub fn click(&self, window: u32, button: u8, (x, y): (i32, i32)) {
        let click = Click {
            window,
            button,
            x: x.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
            y: y.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
        };
        if self.clicks.send(click).is_err() {
            return;
        }
        debug!(?click, "x11 tray: click asked");
        // The tray thread waits for X11 events only: a message to its own
        // window wakes it up.
        let wake = ClientMessageEvent::new(32, self.manager, self.wake, [0u32; 5]);
        let sent = self
            .connection
            .send_event(false, self.manager, EventMask::NO_EVENT, wake)
            .map(|_| ())
            .and_then(|()| self.connection.flush());
        if let Err(error) = sent {
            warn!(%error, "x11 tray: cannot replay a click");
        }
    }
}

/// Becomes the tray of the X server named by `DISPLAY`, if there is one and
/// no other program holds it. Icons are sent to `icons` each time they
/// change.
pub fn start(icons: UnboundedSender<Vec<X11Icon>>) -> Option<Tray> {
    std::env::var_os("DISPLAY")?;
    let host = match Host::new(icons) {
        Ok(Some(host)) => host,
        Ok(None) => return None,
        Err(error) => {
            warn!(%error, "x11 tray: cannot start, X11 tray icons show in windows of their own");
            return None;
        }
    };
    let tray = Tray {
        connection: host.connection.clone(),
        manager: host.manager,
        wake: host.atoms._COQUILLE_WAKE,
        clicks: host.clicks_in.clone(),
    };
    std::thread::Builder::new()
        .name("x11-tray".to_owned())
        .spawn(move || host.run())
        .ok()?;
    Some(tray)
}

/// An icon docked in the tray.
struct Docked {
    window: Window,
    /// The hidden window it lives in.
    container: Window,
    damage: u32,
    /// Width and height of the icon window, chosen by its program.
    size: (u16, u16),
    class: String,
    name: String,
    /// The client shows it (`MapNotify`), which it may stop doing.
    shown: bool,
    pixmap: Option<TrayPixmap>,
}

struct Host {
    connection: Arc<RustConnection>,
    atoms: Atoms,
    root: Window,
    selection: Atom,
    manager: Window,
    /// Depth, visual and colormap of the hidden windows: the visual offered
    /// to the icons, with an alpha channel when the server has one.
    depth: u8,
    visual: Visualid,
    colormap: u32,
    byte_order: ImageOrder,
    icons: Vec<Docked>,
    clicks_in: mpsc::Sender<Click>,
    clicks: mpsc::Receiver<Click>,
    /// Clicks waiting for a timestamp of the server.
    waiting: Vec<Click>,
    updates: UnboundedSender<Vec<X11Icon>>,
}

impl Host {
    fn new(
        updates: UnboundedSender<Vec<X11Icon>>,
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let (connection, screen_number) = x11rb::connect(None)?;
        let connection = Arc::new(connection);
        let atoms = Atoms::new(&*connection)?.reply()?;
        let selection_name = format!("_NET_SYSTEM_TRAY_S{screen_number}");
        let selection = connection
            .intern_atom(false, selection_name.as_bytes())?
            .reply()?
            .atom;
        connection.composite_query_version(0, 4)?.reply()?;
        connection.damage_query_version(1, 1)?.reply()?;

        let setup = connection.setup();
        let screen = &setup.roots[screen_number];
        let root = screen.root;
        let byte_order = setup.image_byte_order;

        let owner = connection.get_selection_owner(selection)?.reply()?.owner;
        if owner != x11rb::NONE {
            info!("x11 tray: another program holds the X11 tray icons");
            return Ok(None);
        }

        // A visual with an alpha channel lets the icons be drawn on nothing.
        let argb = screen
            .allowed_depths
            .iter()
            .filter(|depth| depth.depth == 32)
            .flat_map(|depth| depth.visuals.iter())
            .find(|visual| visual.class == VisualClass::TRUE_COLOR)
            .map(|visual| visual.visual_id);
        let (depth, visual, colormap) = match argb {
            Some(visual) => {
                let colormap = connection.generate_id()?;
                connection.create_colormap(ColormapAlloc::NONE, colormap, root, visual)?;
                (32, visual, colormap)
            }
            None => (
                screen.root_depth,
                screen.root_visual,
                screen.default_colormap,
            ),
        };

        let manager = connection.generate_id()?;
        connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            manager,
            root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        connection.change_property32(
            PropMode::REPLACE,
            manager,
            atoms._NET_SYSTEM_TRAY_ORIENTATION,
            AtomEnum::CARDINAL,
            &[0],
        )?;
        connection.change_property32(
            PropMode::REPLACE,
            manager,
            atoms._NET_SYSTEM_TRAY_VISUAL,
            AtomEnum::VISUALID,
            &[visual],
        )?;
        connection.set_selection_owner(manager, selection, x11rb::CURRENT_TIME)?;
        if connection.get_selection_owner(selection)?.reply()?.owner != manager {
            info!("x11 tray: another program took the X11 tray icons first");
            return Ok(None);
        }
        // Programs already running dock their icons once told.
        let announce = ClientMessageEvent::new(
            32,
            root,
            atoms.MANAGER,
            [x11rb::CURRENT_TIME, selection, manager, 0, 0],
        );
        connection.send_event(false, root, EventMask::STRUCTURE_NOTIFY, announce)?;
        connection.flush()?;
        info!("x11 tray: X11 tray icons go to the dock");

        let (clicks_in, clicks) = mpsc::channel();
        Ok(Some(Self {
            connection,
            atoms,
            root,
            selection,
            manager,
            depth,
            visual,
            colormap,
            byte_order,
            icons: Vec::new(),
            clicks_in,
            clicks,
            waiting: Vec::new(),
            updates,
        }))
    }

    fn run(mut self) {
        loop {
            let event = match self.connection.wait_for_event() {
                Ok(event) => event,
                Err(error) => {
                    info!(%error, "x11 tray: the X server is gone");
                    break;
                }
            };
            match self.handle(event) {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => debug!(%error, "x11 tray: a request failed"),
            }
            let _ = self.connection.flush();
        }
        let _ = self.updates.send(Vec::new());
    }

    /// `false` once the tray is no longer ours.
    fn handle(&mut self, event: Event) -> Result<bool, ReplyOrIdError> {
        match event {
            Event::ClientMessage(message) if message.window == self.manager => {
                let data = message.data.as_data32();
                if message.type_ == self.atoms._NET_SYSTEM_TRAY_OPCODE && data[1] == REQUEST_DOCK {
                    self.dock(data[2])?;
                } else if message.type_ == self.atoms._COQUILLE_WAKE {
                    self.waiting.extend(self.clicks.try_iter());
                    // A click needs the time of the server: changing a
                    // property of our own window tells it.
                    self.connection.change_property8(
                        PropMode::APPEND,
                        self.manager,
                        self.atoms._COQUILLE_TIME,
                        AtomEnum::STRING,
                        &[],
                    )?;
                }
            }
            Event::PropertyNotify(change) => {
                if change.window == self.manager && change.atom == self.atoms._COQUILLE_TIME {
                    for click in std::mem::take(&mut self.waiting) {
                        debug!(?click, time = change.time, "x11 tray: click replayed");
                        self.replay(click, change.time)?;
                    }
                } else if let Some(index) = self.index(change.window)
                    && (change.atom == u32::from(AtomEnum::WM_NAME)
                        || change.atom == self.atoms._NET_WM_NAME)
                {
                    self.icons[index].name = self.name_of(change.window);
                    self.publish();
                }
            }
            Event::DamageNotify(damage) => {
                if let Some(index) = self.index(damage.drawable) {
                    self.connection
                        .damage_subtract(damage.damage, x11rb::NONE, x11rb::NONE)?;
                    self.refresh(index);
                }
            }
            Event::MapNotify(map) => {
                if let Some(index) = self.index(map.window) {
                    self.icons[index].shown = true;
                    self.refresh(index);
                }
            }
            Event::UnmapNotify(unmap) => {
                if let Some(index) = self.index(unmap.window) {
                    self.icons[index].shown = false;
                    self.publish();
                }
            }
            Event::DestroyNotify(gone) => {
                if let Some(index) = self.index(gone.window) {
                    self.forget(index, true)?;
                }
            }
            // The program took its icon back.
            Event::ReparentNotify(moved) => {
                if let Some(index) = self.index(moved.window)
                    && moved.parent != self.icons[index].container
                {
                    self.forget(index, false)?;
                }
            }
            Event::SelectionClear(clear) if clear.selection == self.selection => {
                info!("x11 tray: another program took the X11 tray icons");
                return Ok(false);
            }
            Event::Error(error) => debug!(?error, "x11 tray: X11 error"),
            _ => {}
        }
        Ok(true)
    }

    fn index(&self, window: Window) -> Option<usize> {
        self.icons.iter().position(|icon| icon.window == window)
    }

    /// A program asks the tray to hold its icon.
    fn dock(&mut self, window: Window) -> Result<(), ReplyOrIdError> {
        if window == x11rb::NONE || self.index(window).is_some() {
            return Ok(());
        }
        // A window that is already gone is not docked.
        let Ok(geometry) = self.connection.get_geometry(window)?.reply() else {
            return Ok(());
        };
        // The program draws its icon at the size it chose, border included.
        let fits = |side: u16| (1..=MAX_SIZE).contains(&side);
        let size = if fits(geometry.width) && fits(geometry.height) {
            (geometry.width, geometry.height)
        } else {
            (SIZE, SIZE)
        };
        let connection = &self.connection;
        let container = connection.generate_id()?;
        connection.create_window(
            self.depth,
            container,
            self.root,
            0,
            0,
            size.0,
            size.1,
            0,
            WindowClass::INPUT_OUTPUT,
            self.visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(self.colormap)
                .override_redirect(1)
                .event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        )?;
        // Never drawn by coquille (`Shell::visible_overrides`), but mapped:
        // an unmapped window has no image to read.
        connection.change_property32(
            PropMode::REPLACE,
            container,
            self.atoms._NET_WM_WINDOW_OPACITY,
            AtomEnum::CARDINAL,
            &[0],
        )?;
        connection.map_window(container)?;

        connection.change_window_attributes(
            window,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                .event_mask(EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE),
        )?;
        // If the dock goes, the icon goes back to the root window instead
        // of being destroyed with the container.
        connection.change_save_set(SetMode::INSERT, window)?;
        connection.reparent_window(window, container, 0, 0)?;
        connection.composite_redirect_window(window, Redirect::MANUAL)?;
        connection.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(0)
                .y(0)
                .width(u32::from(size.0))
                .height(u32::from(size.1)),
        )?;
        let embedded = ClientMessageEvent::new(
            32,
            window,
            self.atoms._XEMBED,
            [x11rb::CURRENT_TIME, EMBEDDED_NOTIFY, 0, container, 0],
        );
        connection.send_event(false, window, EventMask::NO_EVENT, embedded)?;
        connection.map_window(window)?;
        let damage = connection.generate_id()?;
        connection.damage_create(damage, window, ReportLevel::NON_EMPTY)?;

        let class = self.class_of(window);
        let name = self.name_of(window);
        debug!(window, class, name, "x11 tray: icon docked");
        self.icons.push(Docked {
            window,
            container,
            damage,
            size,
            class,
            name,
            shown: true,
            pixmap: None,
        });
        self.refresh(self.icons.len() - 1);
        Ok(())
    }

    /// The icon is gone (`destroyed`), or taken back by its program.
    fn forget(&mut self, index: usize, destroyed: bool) -> Result<(), ReplyOrIdError> {
        let icon = self.icons.remove(index);
        debug!(window = icon.window, "x11 tray: icon gone");
        // A destroyed window takes its damage object with it.
        if !destroyed {
            self.connection.damage_destroy(icon.damage)?;
        }
        self.connection.destroy_window(icon.container)?;
        self.publish();
        Ok(())
    }

    /// Reads the image of an icon again.
    fn refresh(&mut self, index: usize) {
        let window = self.icons[index].window;
        let (width, height) = self.icons[index].size;
        let image = self
            .connection
            .get_image(ImageFormat::Z_PIXMAP, window, 0, 0, width, height, !0)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        let pixmap = image
            .and_then(|image| {
                to_rgba(
                    image.depth,
                    self.byte_order,
                    u32::from(width),
                    u32::from(height),
                    &image.data,
                )
            })
            .map(trim);
        if pixmap != self.icons[index].pixmap {
            self.icons[index].pixmap = pixmap;
            self.publish();
        }
    }

    fn publish(&self) {
        let icons = self
            .icons
            .iter()
            .filter(|icon| icon.shown)
            .filter_map(|icon| {
                Some(X11Icon {
                    window: icon.window,
                    class: icon.class.clone(),
                    name: icon.name.clone(),
                    pixmap: icon.pixmap.clone()?,
                })
            })
            .collect();
        let _ = self.updates.send(icons);
    }

    /// Replays a click on an icon: what Wine reads is the press and the
    /// release sent to its window, with where the pointer was.
    fn replay(&self, click: Click, time: u32) -> Result<(), ReplyOrIdError> {
        let Some(index) = self.index(click.window) else {
            return Ok(());
        };
        let window = self.icons[index].window;
        let (width, height) = self.icons[index].size;
        let middle = ((width / 2) as i16, (height / 2) as i16);
        // Wine reads the click only where its window is: the hidden window
        // goes under the click first (it stays hidden, see `dock`).
        self.connection.configure_window(
            self.icons[index].container,
            &ConfigureWindowAux::new()
                .x(i32::from(click.x) - i32::from(middle.0))
                .y(i32::from(click.y) - i32::from(middle.1)),
        )?;
        let press = ButtonPressEvent {
            response_type: BUTTON_PRESS_EVENT,
            detail: click.button,
            sequence: 0,
            time,
            root: self.root,
            event: window,
            child: x11rb::NONE,
            root_x: click.x,
            root_y: click.y,
            event_x: middle.0,
            event_y: middle.1,
            state: KeyButMask::default(),
            same_screen: true,
        };
        let release = ButtonPressEvent {
            response_type: BUTTON_RELEASE_EVENT,
            state: held(click.button),
            ..press
        };
        self.connection
            .send_event(false, window, EventMask::BUTTON_PRESS, press)?;
        self.connection
            .send_event(false, window, EventMask::BUTTON_RELEASE, release)?;
        Ok(())
    }

    fn name_of(&self, window: Window) -> String {
        let read = |property: Atom, kind: Atom| {
            let reply = self
                .connection
                .get_property(false, window, property, kind, 0, 256)
                .ok()?
                .reply()
                .ok()?;
            let text = String::from_utf8_lossy(&reply.value).trim().to_owned();
            (!text.is_empty()).then_some(text)
        };
        read(self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING)
            .or_else(|| read(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
            .unwrap_or_default()
    }

    fn class_of(&self, window: Window) -> String {
        let reply = self
            .connection
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        reply
            .map(|reply| class_name(&reply.value))
            .unwrap_or_default()
    }
}

/// The state of the pointer buttons while `button` is held.
fn held(button: u8) -> KeyButMask {
    match button {
        1 => KeyButMask::BUTTON1,
        2 => KeyButMask::BUTTON2,
        3 => KeyButMask::BUTTON3,
        _ => KeyButMask::default(),
    }
}

/// The class of `WM_CLASS` (`instance\0class\0`), else its instance.
fn class_name(value: &[u8]) -> String {
    let mut parts = value
        .split(|byte| *byte == 0)
        .map(|part| String::from_utf8_lossy(part).into_owned());
    let instance = parts.next().unwrap_or_default();
    parts
        .next()
        .filter(|class| !class.is_empty())
        .unwrap_or(instance)
}

/// An X11 image (`ZPixmap`, 32 bits per pixel) as RGBA pixels. With an alpha
/// channel (depth 32) the pixels are premultiplied, as X11 has them; without
/// one, the icon is opaque. `None` while the icon shows nothing.
fn to_rgba(
    depth: u8,
    order: ImageOrder,
    width: u32,
    height: u32,
    data: &[u8],
) -> Option<TrayPixmap> {
    let count = (width * height) as usize;
    if data.len() < count * 4 || !(depth == 24 || depth == 32) {
        return None;
    }
    let mut pixels = Vec::with_capacity(count * 4);
    for chunk in data.chunks_exact(4).take(count) {
        let [b, g, r, a] = match order {
            ImageOrder::LSB_FIRST => [chunk[0], chunk[1], chunk[2], chunk[3]],
            _ => [chunk[3], chunk[2], chunk[1], chunk[0]],
        };
        let a = if depth == 32 { a } else { 255 };
        let straight = |c: u8| match a {
            0 => 0,
            255 => c,
            a => ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8,
        };
        pixels.extend_from_slice(&[straight(r), straight(g), straight(b), a]);
    }
    let blank = pixels.chunks_exact(4).all(|pixel| pixel[3] == 0);
    (!blank).then_some(TrayPixmap {
        width,
        height,
        pixels,
    })
}

/// The icon without the transparent border around it (Wine draws one of 2
/// pixels): the dock gives every icon the same room.
fn trim(pixmap: TrayPixmap) -> TrayPixmap {
    let (width, height) = (pixmap.width as usize, pixmap.height as usize);
    let alpha = |x: usize, y: usize| pixmap.pixels[(y * width + x) * 4 + 3];
    let row_empty = |y: usize| (0..width).all(|x| alpha(x, y) == 0);
    let column_empty = |x: usize| (0..height).all(|y| alpha(x, y) == 0);
    let top = (0..height).find(|y| !row_empty(*y)).unwrap_or(0);
    let bottom = (0..height)
        .rev()
        .find(|y| !row_empty(*y))
        .unwrap_or(height - 1);
    let left = (0..width).find(|x| !column_empty(*x)).unwrap_or(0);
    let right = (0..width)
        .rev()
        .find(|x| !column_empty(*x))
        .unwrap_or(width - 1);
    // A square, centered on what is drawn: icons are square.
    let side = (right - left + 1).max(bottom - top + 1);
    let left = (left + right + 1).saturating_sub(side) / 2;
    let top = (top + bottom + 1).saturating_sub(side) / 2;
    let side = side.min(width - left).min(height - top);
    let mut pixels = Vec::with_capacity(side * side * 4);
    for y in top..top + side {
        let start = (y * width + left) * 4;
        pixels.extend_from_slice(&pixmap.pixels[start..start + side * 4]);
    }
    TrayPixmap {
        width: side as u32,
        height: side as u32,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `size`×`size` image, opaque inside `[from, to)` on both axes.
    fn framed(size: u32, from: u32, to: u32) -> TrayPixmap {
        let mut pixels = Vec::new();
        for y in 0..size {
            for x in 0..size {
                let inside = (from..to).contains(&x) && (from..to).contains(&y);
                pixels.extend_from_slice(&[9, 9, 9, if inside { 255 } else { 0 }]);
            }
        }
        TrayPixmap {
            width: size,
            height: size,
            pixels,
        }
    }

    #[test]
    fn the_border_wine_draws_around_an_icon_is_trimmed() {
        let trimmed = trim(framed(20, 2, 18));
        assert_eq!((trimmed.width, trimmed.height), (16, 16));
        assert!(trimmed.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn an_icon_drawn_to_its_edges_is_kept_whole() {
        let trimmed = trim(framed(16, 0, 16));
        assert_eq!((trimmed.width, trimmed.height), (16, 16));
    }

    #[test]
    fn a_premultiplied_pixel_becomes_straight() {
        // Half-transparent red, premultiplied: B G R A.
        let data = [0, 0, 128, 128];
        let pixmap = to_rgba(32, ImageOrder::LSB_FIRST, 1, 1, &data).unwrap();
        assert_eq!(pixmap.pixels, vec![255, 0, 0, 128]);
    }

    #[test]
    fn an_image_without_alpha_is_opaque() {
        let data = [10, 20, 30, 0];
        let pixmap = to_rgba(24, ImageOrder::LSB_FIRST, 1, 1, &data).unwrap();
        assert_eq!(pixmap.pixels, vec![30, 20, 10, 255]);
    }

    #[test]
    fn an_icon_not_drawn_yet_has_no_image() {
        let data = [0u8; 16];
        assert_eq!(to_rgba(32, ImageOrder::LSB_FIRST, 2, 2, &data), None);
    }

    #[test]
    fn a_short_image_is_refused() {
        assert_eq!(to_rgba(32, ImageOrder::LSB_FIRST, 2, 2, &[255; 8]), None);
    }

    #[test]
    fn the_class_is_read_from_wm_class() {
        assert_eq!(class_name(b"upc.exe\0upc.exe\0"), "upc.exe");
        assert_eq!(class_name(b"steam\0Steam\0"), "Steam");
        assert_eq!(class_name(b"lonely\0"), "lonely");
    }

    #[test]
    fn a_release_says_which_button_was_held() {
        assert_eq!(held(1), KeyButMask::BUTTON1);
        assert_eq!(held(3), KeyButMask::BUTTON3);
    }
}
