//! The hardware backend: real screens through DRM/KMS, real devices through
//! libinput, and the seat (who may use them) through libseat.
//!
//! One GPU drives everything: the one the firmware booted on, which is where
//! the screens are plugged. Screens are found once, at start-up.

use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, anyhow};
use coquille_model::{OutputId, Rect, Scale, Size as ModelSize};
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::VrrSupport;
use smithay::backend::drm::compositor::{FrameFlags, PrimaryPlaneElement};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmEvent, DrmNode};
use smithay::backend::egl::context::ContextPriority;
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{all_gpus, primary_gpu};
use smithay::output::{Mode as OutputMode, Output, PhysicalProperties, Scale as OutputScale};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::drm::Device as _;
use smithay::reexports::drm::control::Device as _;
use smithay::reexports::drm::control::{ModeTypeFlags, connector, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::DeviceFd;
use smithay::wayland::drm_syncobj::{DrmSyncobjState, supports_syncobj_eventfd};
use smithay_drm_extras::display_info;
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use tracing::{debug, error, info, warn};

use super::Backend;
use crate::config::{GameConfig, MouseConfig, ScaleConfig, TouchpadClick, TouchpadConfig};
use crate::game::{Boost, FrameNote};
use crate::outputs::{Connected, ModeInfo, RoleConfig, assign_roles, origins, pick_mode};
use crate::render::scene::{self, Layers, SceneElement};
use crate::render::shaders::Shaders;
use crate::shell::ScreenInfo;
use crate::state::State;

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
type OutputManager = DrmOutputManager<Allocator, Exporter, (), DrmDeviceFd>;
type ScreenSurface = DrmOutput<Allocator, Exporter, (), DrmDeviceFd>;

/// Eight bits per channel, which every screen and driver accepts. Tried in
/// order: the screen needs no alpha channel, so those without come first.
const COLOR_FORMATS: [Fourcc; 4] = [
    Fourcc::Xrgb8888,
    Fourcc::Xbgr8888,
    Fourcc::Argb8888,
    Fourcc::Abgr8888,
];

/// Where a screen is in its frame cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pacing {
    /// Nothing pending: a redraw may start at once.
    Idle,
    /// A frame is on its way to the screen, or one brought no change and a
    /// timer stands in for the screen's refresh. `redraw` asks for another
    /// frame as soon as this one is done.
    Waiting { redraw: bool },
}

/// A lit screen.
struct Screen {
    id: OutputId,
    crtc: crtc::Handle,
    surface: ScreenSurface,
    pacing: Pacing,
    /// Duration of one refresh.
    frame: Duration,
    /// Frames that failed in a row, to keep the log readable.
    failures: u32,
    /// Game mode: the shortcuts this screen may still take.
    boost: Boost,
    /// The refresh rate follows the game at the moment.
    vrr_on: bool,
    /// The screen and the driver can follow a game without a mode set.
    vrr_usable: bool,
}

pub struct Udev {
    session: LibSeatSession,
    libinput: Libinput,
    pub renderer: GlesRenderer,
    pub shaders: Shaders,
    manager: OutputManager,
    screens: Vec<Screen>,
    /// False while another virtual terminal is shown.
    active: bool,
    /// Explicit synchronisation with clients, where the driver has it.
    pub syncobj: Option<DrmSyncobjState>,
    /// What the configuration allows in game mode, as it was read last.
    game: GameConfig,
    mouse: MouseConfig,
    touchpad: TouchpadConfig,
    pointer_devices: Vec<smithay::reexports::input::Device>,
    keyboard_devices: Vec<smithay::reexports::input::Device>,
}

/// A plugged screen found while scanning, before it is lit.
struct Found {
    connector: connector::Info,
    crtc: crtc::Handle,
    name: String,
    mode: smithay::reexports::drm::control::Mode,
}

/// A GPU ready to draw, with its screens not lit yet.
struct Gpu {
    renderer: GlesRenderer,
    shaders: Shaders,
    manager: OutputManager,
}

/// Prepares rendering and mode setting on an opened GPU. `reset` turns every
/// screen off first, which only the owner of the screens may do.
fn prepare(fd: OwnedFd, reset: bool) -> anyhow::Result<(Gpu, DrmDeviceNotifier)> {
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let node = DrmNode::from_file(&fd).context("cannot identify the GPU")?;
    let (drm, drm_events) = DrmDevice::new(fd.clone(), reset).context("cannot set up DRM")?;
    let gbm = GbmDevice::new(fd).context("cannot set up GBM")?;

    let mut renderer = create_renderer(&gbm)?;
    let shaders = Shaders::compile(&mut renderer).context("cannot compile the shaders")?;

    let render_node = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten())
        .unwrap_or(node);
    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(gbm.clone(), Some(render_node).into());
    let render_formats = renderer.egl_context().dmabuf_render_formats().clone();
    let manager = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        COLOR_FORMATS,
        render_formats,
    );
    let gpu = Gpu {
        renderer,
        shaders,
        manager,
    };
    Ok((gpu, drm_events))
}

/// The role and place of each screen found; `None` for a screen left off.
/// Roles go by the shape of the screens, places by their logical size.
fn plan(found: &[Found], roles: &RoleConfig, scales: &ScaleConfig) -> Vec<Option<ScreenInfo>> {
    let connected: Vec<Connected> = found
        .iter()
        .map(|screen| {
            let (w, h) = screen.mode.size();
            Connected {
                name: screen.name.clone(),
                size: ModelSize::new(i32::from(w), i32::from(h)),
            }
        })
        .collect();
    let roles = assign_roles(&connected, roles);
    let placed: Vec<_> = connected
        .iter()
        .zip(&roles)
        .filter_map(|(screen, role)| {
            let role = (*role)?;
            Some((role, scales.of(role).logical(screen.size)))
        })
        .collect();
    let mut places = origins(&placed).into_iter().zip(&placed);

    let screens = found.iter().zip(roles);
    screens
        .map(|(found, role)| {
            let role = role?;
            let (corner, (_, size)) = places.next()?;
            Some(ScreenInfo {
                id: OutputId(found.crtc.into()),
                role,
                rect: Rect::new(corner.x, corner.y, size.w, size.h),
                scale: scales.of(role),
            })
        })
        .collect()
}

/// Debug aid: examines the GPU and the screens without showing anything,
/// which works from inside another session. Everything short of taking the
/// seat and setting modes is exercised.
pub fn probe(roles: &RoleConfig, scales: &ScaleConfig) -> anyhow::Result<()> {
    let path = gpu_path("seat0")?;
    info!(gpu = %path.display(), "probing this GPU");
    let flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let fd = rustix::fs::open(&path, flags, rustix::fs::Mode::empty())
        .with_context(|| format!("cannot open {}", path.display()))?;
    let (mut gpu, _events) = prepare(fd, false)?;
    // Marked inactive, the device leaves the screens alone when it is closed.
    gpu.manager.pause();

    let found = scan(&gpu.manager)?;
    for (screen, info) in found.iter().zip(plan(&found, roles, scales)) {
        let mode = OutputMode::from(screen.mode);
        info!(
            name = screen.name,
            width = mode.size.w,
            height = mode.size.h,
            refresh_mhz = mode.refresh,
            role = ?info.map(|info| info.role),
            position = ?info.map(|info| (info.rect.x, info.rect.y)),
            scale = ?info.map(|info| info.scale.get()),
            vrr_capable = ?flag(
                gpu.manager.device(),
                screen.connector.handle(),
                "vrr_capable"
            ),
            vrr_switch = flag(gpu.manager.device(), screen.crtc, "VRR_ENABLED").is_some(),
            "screen found"
        );
    }
    let sync = supports_syncobj_eventfd(gpu.manager.device().device_fd());
    info!(
        screens = found.len(),
        explicit_sync = sync,
        "probe done: nothing was displayed"
    );
    Ok(())
}

/// Takes the seat, opens the GPU and lights every screen that gets a role.
pub fn init(
    handle: &LoopHandle<'static, State>,
    roles: &RoleConfig,
    scales: &ScaleConfig,
    game: &GameConfig,
    mouse: &MouseConfig,
    touchpad: &TouchpadConfig,
) -> anyhow::Result<(Udev, Vec<(ScreenInfo, Output)>)> {
    let (mut session, session_events) =
        LibSeatSession::new().map_err(|error| anyhow!("cannot take the seat: {error}"))?;
    let seat = session.seat();

    let path = gpu_path(&seat)?;
    info!(gpu = %path.display(), "using this GPU");
    let flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
    let fd = session
        .open(&path, flags)
        .map_err(|error| anyhow!("cannot open {}: {error}", path.display()))?;
    let (gpu, drm_events) = prepare(fd, true)?;
    let Gpu {
        mut renderer,
        shaders,
        mut manager,
    } = gpu;

    let found = scan(&manager)?;
    let mut screens = Vec::new();
    let mut outputs = Vec::new();
    for (found, info) in found.iter().zip(plan(&found, roles, scales)) {
        let Some(info) = info else {
            warn!(name = found.name, "this screen has no role and stays off");
            continue;
        };
        match light(&mut manager, &mut renderer, found, &info, game) {
            Ok((screen, output)) => {
                screens.push(screen);
                outputs.push((info, output));
            }
            Err(error) => error!(name = found.name, "cannot light this screen: {error:#}"),
        }
    }
    if screens.is_empty() {
        return Err(anyhow!("no screen could be lit"));
    }

    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat)
        .map_err(|()| anyhow!("cannot attach input devices to seat {seat}"))?;

    handle
        .insert_source(
            LibinputInputBackend::new(libinput.clone()),
            |event, _, state| {
                if let smithay::backend::input::InputEvent::DeviceAdded { ref device } = event {
                    let led_state = state
                        .seat
                        .get_keyboard()
                        .map(|keyboard| keyboard.led_state());
                    if let Backend::Udev(udev) = &mut state.backend {
                        udev.device_added(device, led_state);
                    }
                    // The settings screen shows the touchpad's settings
                    // only when there is one.
                    state.shell_changed();
                } else if let smithay::backend::input::InputEvent::DeviceRemoved { ref device } =
                    event
                {
                    if let Backend::Udev(udev) = &mut state.backend {
                        udev.device_removed(device);
                    }
                    state.shell_changed();
                }
                state.process_input_event(event);
            },
        )
        .map_err(|error| anyhow!("cannot watch input devices: {error}"))?;
    handle
        .insert_source(session_events, |event, _, state| state.session_event(event))
        .map_err(|error| anyhow!("cannot watch the session: {error}"))?;
    handle
        .insert_source(drm_events, |event, _, state| match event {
            DrmEvent::VBlank(crtc) => state.frame_shown(crtc),
            DrmEvent::Error(error) => error!(%error, "DRM error"),
        })
        .map_err(|error| anyhow!("cannot watch the screens: {error}"))?;

    let udev = Udev {
        session,
        libinput,
        renderer,
        shaders,
        manager,
        screens,
        active: true,
        syncobj: None,
        game: game.clone(),
        mouse: mouse.clone(),
        touchpad: touchpad.clone(),
        pointer_devices: Vec::new(),
        keyboard_devices: Vec::new(),
    };
    Ok((udev, outputs))
}

/// The GPU the firmware booted on, else the first one of the seat.
fn gpu_path(seat: &str) -> anyhow::Result<PathBuf> {
    let primary = primary_gpu(seat).context("cannot list GPUs")?;
    match primary {
        Some(path) => Ok(path),
        None => all_gpus(seat)
            .context("cannot list GPUs")?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no GPU on seat {seat}")),
    }
}

/// Creates the GLES renderer of a GPU.
#[expect(unsafe_code, reason = "Smithay only offers unsafe constructors here")]
fn create_renderer(gbm: &GbmDevice<DrmDeviceFd>) -> anyhow::Result<GlesRenderer> {
    // SAFETY: the display must not be terminated behind Smithay's back by
    // other EGL code using the same native display; nothing else in this
    // process creates an EGL display.
    let display = unsafe { EGLDisplay::new(gbm.clone()) }.context("cannot set up EGL")?;
    let context = EGLContext::new_with_priority(&display, ContextPriority::High)
        .context("cannot create the EGL context")?;
    // SAFETY: the context must not be current on another thread; it was just
    // created on this one and is handed over to the renderer.
    unsafe { GlesRenderer::new(context) }.context("cannot create the renderer")
}

/// The value of an on/off property of a connector or a CRTC, `None` when the
/// driver does not have it. Game mode reads what the screen and the driver
/// say about a variable refresh rate this way.
fn flag<H>(device: &DrmDevice, object: H, name: &str) -> Option<bool>
where
    H: smithay::reexports::drm::control::ResourceHandle,
{
    let properties = device.get_properties(object).ok()?;
    let (handles, values) = properties.as_props_and_values();
    handles.iter().zip(values).find_map(|(handle, value)| {
        let property = device.get_property(*handle).ok()?;
        (property.name().to_str() == Ok(name)).then_some(*value != 0)
    })
}

/// Every plugged screen that has a CRTC to drive it, with the mode to use.
fn scan(manager: &OutputManager) -> anyhow::Result<Vec<Found>> {
    let device = manager.device();
    let mut scanner: DrmScanner = DrmScanner::new();
    let events = scanner
        .scan_connectors(device)
        .context("cannot scan connectors")?;

    let mut found = Vec::new();
    for event in events {
        let DrmScanEvent::Connected {
            connector,
            crtc: Some(crtc),
        } = event
        else {
            continue;
        };
        let name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );
        let modes: Vec<ModeInfo> = connector
            .modes()
            .iter()
            .map(|mode| {
                let (w, h) = mode.size();
                ModeInfo {
                    size: ModelSize::new(i32::from(w), i32::from(h)),
                    refresh: OutputMode::from(*mode).refresh,
                    preferred: mode.mode_type().contains(ModeTypeFlags::PREFERRED),
                }
            })
            .collect();
        let Some(mode) = pick_mode(&modes).map(|index| connector.modes()[index]) else {
            warn!(name, "this screen offers no mode");
            continue;
        };
        found.push(Found {
            connector,
            crtc,
            name,
            mode,
        });
    }
    // Scan order is not stable across boots; names are.
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Sets a screen's mode and prepares its frame cycle.
fn light(
    manager: &mut OutputManager,
    renderer: &mut GlesRenderer,
    found: &Found,
    info: &ScreenInfo,
    game: &GameConfig,
) -> anyhow::Result<(Screen, Output)> {
    let device = manager.device();
    let connector = &found.connector;
    let edid = display_info::for_connector(device, connector.handle());
    let describe = |text: Option<String>| text.unwrap_or_else(|| "Unknown".into());
    let (width_mm, height_mm) = connector.size().unwrap_or((0, 0));
    let output = Output::new(
        found.name.clone(),
        PhysicalProperties {
            size: (width_mm as i32, height_mm as i32).into(),
            subpixel: connector.subpixel().into(),
            make: describe(edid.as_ref().and_then(|edid| edid.make())),
            model: describe(edid.as_ref().and_then(|edid| edid.model())),
            serial_number: describe(edid.as_ref().and_then(|edid| edid.serial())),
        },
    );
    let mode = OutputMode::from(found.mode);
    let position = (info.rect.x, info.rect.y).into();
    output.set_preferred(mode);
    let scale = OutputScale::Fractional(info.scale.get());
    output.change_current_state(Some(mode), None, Some(scale), Some(position));

    let mut planes = device.planes(&found.crtc).context("cannot list planes")?;
    let driver = device.get_driver().context("cannot identify the driver")?;
    let is_nvidia = [driver.name(), driver.description()]
        .iter()
        .any(|text| text.to_string_lossy().to_lowercase().contains("nvidia"));
    if is_nvidia {
        // Its overlay planes are too restricted to be worth trying.
        planes.overlay = Vec::new();
    }

    let surface = manager
        .lock()
        .initialize_output::<GlesRenderer, SceneElement>(
            found.crtc,
            found.mode,
            &[connector.handle()],
            &output,
            Some(planes),
            renderer,
            &DrmOutputRenderElements::default(),
        )
        .map_err(|error| anyhow!("{error}"))?;

    // Game mode: what this screen and the driver say about a variable
    // refresh rate. It is only used where it needs no mode set, which would
    // blank the screen each time a game starts or stops.
    let vrr = surface.with_compositor(|compositor| compositor.vrr_supported(connector.handle()));
    let vrr_usable = matches!(vrr, Ok(VrrSupport::Supported));
    info!(
        name = found.name,
        role = ?info.role,
        width = mode.size.w,
        height = mode.size.h,
        refresh_mhz = mode.refresh,
        scale = info.scale.get(),
        vrr = ?vrr,
        vrr_for_games = game.vrr() && vrr_usable,
        direct_scanout_for_games = game.direct_scanout(),
        "screen lit"
    );
    let screen = Screen {
        id: info.id,
        crtc: found.crtc,
        surface,
        pacing: Pacing::Idle,
        frame: Duration::from_secs_f64(1000.0 / f64::from(mode.refresh.max(1))),
        failures: 0,
        boost: Boost::new(game.direct_scanout(), game.vrr() && vrr_usable),
        vrr_on: false,
        vrr_usable,
    };
    Ok((screen, output))
}

impl Udev {
    /// The configuration was read again: what it allows in game mode takes
    /// effect at the next frame. Nothing moves when it says the same as
    /// before, so that a shortcut given up after a failure stays given up.
    pub fn set_game(&mut self, game: &GameConfig) {
        if *game == self.game {
            return;
        }
        info!(
            direct_scanout = game.direct_scanout(),
            vrr = game.vrr(),
            "game mode: settings changed"
        );
        for screen in &mut self.screens {
            let vrr = game.vrr() && screen.vrr_usable;
            screen.boost.allow(game.direct_scanout(), vrr);
        }
        self.game = game.clone();
    }

    pub fn device_added(
        &mut self,
        device: &smithay::reexports::input::Device,
        led_state: Option<smithay::input::keyboard::LedState>,
    ) {
        if device.has_capability(smithay::reexports::input::DeviceCapability::Pointer) {
            let mut dev = device.clone();
            if is_touchpad(&dev) {
                info!(name = %device.name(), "touchpad added");
                apply_touchpad_to_device(&mut dev, &self.touchpad);
            } else {
                info!(name = %device.name(), "pointer device added");
                apply_mouse_to_device(&mut dev, &self.mouse);
            }
            if !self.pointer_devices.contains(&dev) {
                self.pointer_devices.push(dev);
            }
        }
        if device.has_capability(smithay::reexports::input::DeviceCapability::Keyboard) {
            info!(name = %device.name(), "keyboard device added");
            let mut dev = device.clone();
            if let Some(led_state) = led_state {
                dev.led_update(led_state.into());
            }
            if !self.keyboard_devices.contains(&dev) {
                self.keyboard_devices.push(dev);
            }
        }
    }

    pub fn device_removed(&mut self, device: &smithay::reexports::input::Device) {
        self.pointer_devices.retain(|d| d != device);
        self.keyboard_devices.retain(|d| d != device);
    }

    pub fn update_led_state(&mut self, led_state: smithay::input::keyboard::LedState) {
        let leds = led_state.into();
        for device in &mut self.keyboard_devices {
            device.led_update(leds);
        }
    }

    /// The mouse and the touchpad each get their own settings, when they
    /// changed.
    pub fn set_pointers(&mut self, mouse: &MouseConfig, touchpad: &TouchpadConfig) {
        if *mouse != self.mouse {
            info!(?mouse, "mouse settings changed");
            self.mouse = mouse.clone();
            for device in self
                .pointer_devices
                .iter_mut()
                .filter(|device| !is_touchpad(device))
            {
                apply_mouse_to_device(device, mouse);
            }
        }
        if *touchpad != self.touchpad {
            info!(?touchpad, "touchpad settings changed");
            self.touchpad = touchpad.clone();
            for device in self
                .pointer_devices
                .iter_mut()
                .filter(|device| is_touchpad(device))
            {
                apply_touchpad_to_device(device, touchpad);
            }
        }
    }

    /// A touchpad is plugged in (a laptop).
    pub fn has_touchpad(&self) -> bool {
        self.pointer_devices.iter().any(is_touchpad)
    }

    pub fn switch_vt(&mut self, vt: i32) {
        if let Err(error) = self.session.change_vt(vt) {
            warn!(vt, %error, "cannot switch terminal");
        }
    }

    /// Offers explicit synchronisation to clients when the driver can do it;
    /// the proprietary NVIDIA driver needs it for flicker-free clients.
    pub fn init_explicit_sync(&mut self, dh: &smithay::reexports::wayland_server::DisplayHandle) {
        let fd = self.manager.device().device_fd().clone();
        if supports_syncobj_eventfd(&fd) {
            self.syncobj = Some(DrmSyncobjState::new::<State>(dh, fd));
            info!("explicit sync is available");
        } else {
            info!("explicit sync is not available on this GPU");
        }
    }
}

impl State {
    /// Draws the screens that are due. Called after each batch of events.
    pub fn redraw_screens(&mut self) {
        let Backend::Udev(udev) = &mut self.backend else {
            return;
        };
        if !udev.active || !std::mem::take(&mut self.redraw_needed) {
            return;
        }
        if std::mem::take(&mut self.full_redraw) {
            for screen in &udev.screens {
                screen.surface.reset_buffers();
            }
        }

        let mut due = Vec::new();
        for (index, screen) in udev.screens.iter_mut().enumerate() {
            match &mut screen.pacing {
                Pacing::Idle => due.push(index),
                Pacing::Waiting { redraw } => *redraw = true,
            }
        }
        for index in due {
            self.render_screen(index);
        }
    }

    /// Draws one screen and sends the frame on its way.
    fn render_screen(&mut self, index: usize) {
        self.shell.refresh_chrome(
            &mut self.painter,
            &self.theme,
            self.hover,
            self.capsule_hover,
        );
        let scale = match &self.backend {
            Backend::Udev(udev) => udev.screens.get(index).map(|screen| screen.id),
            Backend::Winit(_) => None,
        }
        .map_or(Scale::ONE, |id| self.shell.scale_of(id));
        let cursor = self.cursor(true, scale);
        self.refresh_bar();
        let overlay = self.overlay();
        let panels = self.panels();
        let wallpapers = self.active_wallpapers();
        let freeze = self.active_freeze();
        let Backend::Udev(udev) = &mut self.backend else {
            return;
        };
        let Some(screen) = udev.screens.get_mut(index) else {
            return;
        };
        let Some(view) = self.shell.screen(screen.id).map(|info| info.rect) else {
            return;
        };
        let scale = scale.get();

        self.panel_glass
            .retain(|(surface, _), _| surface.is_alive());
        let elements = scene::build(
            &mut udev.renderer,
            &udev.shaders,
            &mut self.shell,
            &self.theme,
            Layers {
                view,
                scale,
                overlay: &overlay,
                panel: self.bar.as_ref().map(|bar| (&bar.buffer, bar.rect)),
                panels: &panels,
                cursor,
                wallpapers: &wallpapers,
                freeze: &freeze,
                glass: &self.glass,
                panel_glass: &mut self.panel_glass,
                bar_glass: &mut self.bar_glass,
                drop_glass: &mut self.drop_glass,
                bar_shadow: &mut self.bar_shadow,
                overlay_shapes: &mut self.overlay_shapes,
            },
        );
        let crtc = screen.crtc;
        // Game mode: a screen held by a fullscreen window may show the
        // buffer of that window as it is, and follow its pace. Every other
        // screen is composited, as it always was.
        let anchored = self
            .shell
            .desk
            .screen(screen.id)
            .is_some_and(|model| model.anchored().is_some());
        let wants_vrr = screen.boost.vrr(anchored);
        if wants_vrr != screen.vrr_on {
            let switched = screen
                .surface
                .with_compositor(|compositor| compositor.use_vrr(wants_vrr));
            match switched {
                Ok(()) => {
                    info!(?crtc, on = wants_vrr, "variable refresh rate switched");
                    screen.vrr_on = wants_vrr;
                }
                Err(error) => {
                    // Not asked again: the screen keeps its fixed rate.
                    screen.boost.vrr_failed();
                    screen.vrr_on = false;
                    warn!(?crtc, %error, "the driver refused the variable refresh rate: left off");
                }
            }
        }

        // Around a game the screen is black, which is also what lets its
        // buffer be shown alone.
        let clear = if anchored {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            self.theme.backdrop.premultiplied()
        };
        let scanout = screen.boost.scanout(anchored) && self.screenshot_mode.is_none();
        // Only the primary plane: the pointer is still drawn by the
        // compositor, so the buffer of the game is shown directly while
        // nothing else is on its screen.
        let flags = if scanout {
            FrameFlags::ALLOW_PRIMARY_PLANE_SCANOUT
        } else {
            FrameFlags::empty()
        };
        let mut draw = |flags| {
            screen
                .surface
                .render_frame(&mut udev.renderer, &elements, clear, flags)
                .map(|frame| {
                    let direct = matches!(frame.primary_element, PrimaryPlaneElement::Element(_));
                    (frame.is_empty, direct)
                })
                .map_err(|error| error.to_string())
        };
        let mut rendered = draw(flags);
        let mut gave_up = false;
        if let (Err(error), true) = (&rendered, scanout) {
            // Whatever went wrong, the composited path is known to work.
            warn!(
                ?crtc,
                error, "direct scanout failed: this screen is composited from now on"
            );
            rendered = draw(FrameFlags::empty());
            gave_up = screen.boost.scanout_failed();
        }
        let direct = matches!(rendered, Ok((_, true)));
        match screen.boost.frame_shown(anchored, direct) {
            Some(FrameNote::Direct) => {
                info!(
                    ?crtc,
                    "direct scanout: the buffer of the game is shown as it is"
                );
            }
            Some(FrameNote::Composited) => info!(
                ?crtc,
                elements = elements.len(),
                "direct scanout not possible for this frame: composited \
                 (more than one element means something else is on the screen, \
                 one element that the driver or the buffer format did not allow it)"
            ),
            None => {}
        }
        let queued = match rendered {
            Ok((true, _)) => Ok(false),
            Ok((false, _)) => screen
                .surface
                .queue_frame(())
                .map(|()| true)
                .map_err(|error| error.to_string()),
            Err(error) => Err(error),
        };
        if queued.is_err() && direct && screen.boost.scanout_failed() {
            warn!(
                ?crtc,
                "a directly shown frame was refused: this screen is composited from now on"
            );
            gave_up = true;
        }
        // A frame that fails while the refresh rate follows the game: the
        // fixed rate is known to work, and comes back with the next frame.
        if queued.is_err() && screen.vrr_on && screen.boost.vrr_failed() {
            warn!(
                ?crtc,
                "a frame failed with the variable refresh rate on: it is turned off for good"
            );
            gave_up = true;
        }
        if gave_up {
            // Start again from clean buffers, with the next frame.
            self.full_redraw = true;
            self.redraw_needed = true;
        }

        let (sent, failed) = match queued {
            Ok(sent) => {
                screen.failures = 0;
                (sent, false)
            }
            Err(error) => {
                // Said once, then every few seconds: a broken screen must
                // not flood the log.
                if screen.failures % 240 == 0 {
                    warn!(?crtc, error, "cannot draw a frame");
                }
                screen.failures += 1;
                (false, true)
            }
        };
        // After a failure, another try follows this frame's time.
        screen.pacing = Pacing::Waiting { redraw: failed };
        if !sent {
            // Nothing went to the screen, so nothing will come back from it:
            // a timer ends this frame instead.
            let timer = Timer::from_duration(screen.frame);
            let armed = self.loop_handle.insert_source(timer, move |_, _, state| {
                state.frame_done(crtc);
                TimeoutAction::Drop
            });
            if let Err(error) = armed {
                error!(%error, "cannot arm a frame timer");
                screen.pacing = Pacing::Idle;
            }
        }
    }

    /// The screen showed the frame that was queued.
    fn frame_shown(&mut self, crtc: crtc::Handle) {
        if let Backend::Udev(udev) = &mut self.backend {
            let screen = udev.screens.iter_mut().find(|screen| screen.crtc == crtc);
            if let Some(Err(error)) = screen.map(|screen| screen.surface.frame_submitted()) {
                debug!(?crtc, %error, "frame not acknowledged");
            }
        }
        self.frame_done(crtc);
    }

    /// A frame cycle ended: clients may draw again, and a redraw asked in
    /// the meantime starts now.
    fn frame_done(&mut self, crtc: crtc::Handle) {
        let Backend::Udev(udev) = &mut self.backend else {
            return;
        };
        let Some(index) = udev.screens.iter().position(|screen| screen.crtc == crtc) else {
            return;
        };
        let screen = &mut udev.screens[index];
        let redraw = screen.pacing == Pacing::Waiting { redraw: true };
        screen.pacing = Pacing::Idle;
        let id = screen.id;
        let active = udev.active;

        self.send_frame_callbacks(Some(id));
        if redraw && active {
            self.render_screen(index);
        }
    }

    /// The user switched to another virtual terminal, or came back.
    fn session_event(&mut self, event: SessionEvent) {
        let Backend::Udev(udev) = &mut self.backend else {
            return;
        };
        match event {
            SessionEvent::PauseSession => {
                info!("session paused");
                udev.active = false;
                udev.libinput.suspend();
                udev.manager.pause();
            }
            SessionEvent::ActivateSession => {
                info!("session resumed");
                if udev.libinput.resume().is_err() {
                    error!("cannot resume input devices");
                }
                if let Err(error) = udev.manager.lock().activate(false) {
                    error!(%error, "cannot take the screens back");
                }
                udev.active = true;
                for screen in &mut udev.screens {
                    screen.pacing = Pacing::Idle;
                    // Asked again with the next frame of a game.
                    screen.vrr_on = false;
                }
                self.full_redraw = true;
                self.queue_redraw();
            }
        }
    }
}

/// A touchpad can be tapped; a mouse cannot.
fn is_touchpad(device: &smithay::reexports::input::Device) -> bool {
    device.config_tap_finger_count() > 0
}

fn apply_touchpad_to_device(
    device: &mut smithay::reexports::input::Device,
    touchpad: &TouchpadConfig,
) {
    use smithay::reexports::input::{ClickMethod, ScrollMethod};

    let name = device.name().into_owned();
    let check = |what: &str, result: Result<(), smithay::reexports::input::DeviceConfigError>| {
        if let Err(error) = result {
            warn!(name = %name, ?error, "cannot set the touchpad's {what}");
        }
    };
    check("tap", device.config_tap_set_enabled(touchpad.tap()));
    check(
        "tap and drag",
        device.config_tap_set_drag_enabled(touchpad.tap_drag()),
    );
    if device.config_dwt_is_available() {
        check(
            "disable while typing",
            device.config_dwt_set_enabled(touchpad.disable_while_typing()),
        );
    }
    if device.config_scroll_has_natural_scroll() {
        check(
            "natural scroll",
            device.config_scroll_set_natural_scroll_enabled(touchpad.natural_scroll()),
        );
    }
    if device
        .config_scroll_methods()
        .contains(&ScrollMethod::TwoFinger)
    {
        check(
            "two-finger scroll",
            device.config_scroll_set_method(ScrollMethod::TwoFinger),
        );
    }
    let method = match touchpad.click_method() {
        TouchpadClick::Fingers => ClickMethod::Clickfinger,
        TouchpadClick::Areas => ClickMethod::ButtonAreas,
    };
    if device.config_click_methods().contains(&method) {
        check("click method", device.config_click_set_method(method));
    }
    if device.config_accel_is_available() {
        let speed = touchpad.accel_speed.unwrap_or(0.0).clamp(-1.0, 1.0);
        check("speed", device.config_accel_set_speed(speed));
    }
}

fn apply_mouse_to_device(device: &mut smithay::reexports::input::Device, mouse: &MouseConfig) {
    use crate::config::MouseAccelProfile;
    use smithay::reexports::input::AccelProfile;

    if device.config_accel_is_available() {
        let speed = mouse
            .accel_speed
            .unwrap_or_else(|| device.config_accel_default_speed());
        let clamped = speed.clamp(-1.0, 1.0);
        if let Err(err) = device.config_accel_set_speed(clamped) {
            warn!(name = %device.name(), ?err, speed = clamped, "cannot set pointer speed");
        }

        let profile = mouse
            .accel_profile
            .map(|p| match p {
                MouseAccelProfile::Flat => AccelProfile::Flat,
                MouseAccelProfile::Adaptive => AccelProfile::Adaptive,
            })
            .or_else(|| device.config_accel_default_profile());
        if let Some(profile) = profile {
            if device.config_accel_profiles().contains(&profile) {
                if let Err(err) = device.config_accel_set_profile(profile) {
                    warn!(
                        name = %device.name(),
                        ?err,
                        ?profile,
                        "cannot set pointer acceleration profile"
                    );
                }
            }
        }
    }

    if device.config_scroll_has_natural_scroll() {
        let natural = mouse
            .natural_scroll
            .unwrap_or_else(|| device.config_scroll_default_natural_scroll_enabled());
        if let Err(err) = device.config_scroll_set_natural_scroll_enabled(natural) {
            warn!(name = %device.name(), ?err, natural, "cannot set natural scroll");
        }
    }

    if device.config_left_handed_is_available() {
        let left_handed = mouse
            .left_handed
            .unwrap_or_else(|| device.config_left_handed_default());
        if let Err(err) = device.config_left_handed_set(left_handed) {
            warn!(name = %device.name(), ?err, left_handed, "cannot set left handed mode");
        }
    }
}
