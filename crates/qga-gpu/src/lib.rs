//! wgpu/Vulkan renderer. This crate owns the frame.
//!
//! Geometry meaning lives in qga / qga-math. Renderer claims are **Software fact**.

mod camera;
mod context;
mod hud;
mod mesh;
mod profile;
mod renderer;
mod types;

#[cfg(feature = "qga-math")]
mod math_convert;

pub use camera::*;
pub use context::*;
pub use hud::{hud_quad, hud_stroke, hud_text};
pub use mesh::*;
pub use profile::*;
pub use renderer::*;
pub use types::*;
