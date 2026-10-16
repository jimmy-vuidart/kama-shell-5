//! Where frames go and where input comes from: a window of another session,
//! or the real screens.

pub mod udev;
pub mod winit;

use smithay::backend::renderer::gles::GlesRenderer;
use smithay::input::pointer::CursorImageStatus;

use self::udev::Udev;
use self::winit::Winit;
use crate::render::shaders::Shaders;

pub enum Backend {
    Winit(Box<Winit>),
    Udev(Box<Udev>),
}

impl Backend {
    /// The shaders compiled on the GPU.
    pub fn shaders(&self) -> &Shaders {
        match self {
            Self::Winit(winit) => &winit.shaders,
            Self::Udev(udev) => &udev.shaders,
        }
    }

    /// The renderer, for work done outside of a frame (imports, set-up).
    pub fn renderer(&mut self) -> &mut GlesRenderer {
        match self {
            Self::Winit(winit) => {
                winit.window_is_current = false;
                winit.backend.renderer()
            }
            Self::Udev(udev) => &mut udev.renderer,
        }
    }

    /// Nested, named cursors are drawn by the host; on real screens the
    /// compositor draws every cursor itself.
    pub fn show_host_cursor(&self, image: &CursorImageStatus) {
        match self {
            Self::Winit(winit) => winit.show_host_cursor(image),
            Self::Udev(_) => {}
        }
    }

    pub fn switch_vt(&mut self, vt: i32) {
        match self {
            // The host session owns the virtual terminals.
            Self::Winit(_) => tracing::debug!(vt, "no terminal switch when nested"),
            Self::Udev(udev) => udev.switch_vt(vt),
        }
    }

    pub fn update_led_state(&mut self, led_state: smithay::input::keyboard::LedState) {
        match self {
            Self::Winit(_) => {}
            Self::Udev(udev) => udev.update_led_state(led_state),
        }
    }
}
