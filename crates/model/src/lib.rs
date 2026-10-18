//! Pure placement logic of the coquille shell.
//!
//! This crate knows nothing about Wayland. The compositor calls an operation on
//! [`Desk`] and then applies [`Desk::arrangement`]; every placement rule lives
//! here and is unit tested.
//!
//! All rectangles are in logical pixels, relative to the top-left corner of
//! the output they belong to.

#![forbid(unsafe_code)]

pub mod desk;
pub mod geometry;
pub mod ids;
pub mod layout;
pub mod memory;
pub mod metrics;
pub mod output;
pub mod proportions;
pub mod scale;
pub mod screen;
pub mod stack;

pub use desk::{
    CapsuleGroup, Desk, DropZone, ModelError, OpenRequest, Placement, WindowGeometry, WindowRecord,
};
pub use geometry::{Insets, Point, Rect, Size};
pub use ids::{AppKey, OutputId, WindowId};
pub use layout::{LayoutKind, SlotId, Split, StackId};
pub use memory::{AppMemory, RememberedPlace};
pub use metrics::Metrics;
pub use output::{OutputInfo, OutputRole};
pub use proportions::Proportions;
pub use scale::Scale;
pub use screen::{FloatingWindow, Screen};
pub use stack::TabStack;
