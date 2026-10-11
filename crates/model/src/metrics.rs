//! Sizes shared by the model and the renderer (see `docs/maquettes/`).

/// Logical pixel sizes of the shell chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// Space between slots, and between slots and the usable area's edges.
    pub gap: i32,
    /// Height of the server-side title bar.
    pub titlebar_height: i32,
    /// Corner radius of windows.
    pub corner_radius: i32,
    /// Width of the accent ring around the active window.
    pub ring_width: i32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            gap: 8,
            titlebar_height: 38,
            corner_radius: 12,
            ring_width: 2,
        }
    }
}
