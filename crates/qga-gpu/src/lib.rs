//! wgpu/Vulkan renderer. This crate owns the frame.
//!
//! Geometry meaning lives in qga / qga-math. Renderer claims are **Software fact**.

mod camera;
mod claims;
mod context;
mod hud;
mod mesh;
mod profile;
mod renderer;
mod types;

pub use camera::*;
pub use claims::{claim_line, print_claim_banner, CLAIM};
pub use context::*;
pub use hud::{hud_quad, hud_stroke, hud_text};
pub use mesh::*;
pub use profile::*;
pub use renderer::*;
pub use types::*;
