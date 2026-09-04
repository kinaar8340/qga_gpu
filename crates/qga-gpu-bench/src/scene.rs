//! Observer / cones / separator torus. Software fact of this binary.
//! Same static sculpture as `qga-gpu-demo` (1 sphere, 2 cones, gold torus).
//! Tessellated once via `retain_meshes`. The torus centerline is the static
//! fiber; it is never rebuilt in the frame loop.

use glam::Vec3;
use qga_gpu::{hud_text, GpuHub, HudVert, Mesh};

/// Y-up. `Mesh::cone` height is +Y; feeling cone is `rotated_x(π/2)` onto +Z.
/// Visual cone stays +Y so the pair is 90°. Matches the README hero image.
pub fn sculpture_meshes() -> Vec<Mesh> {
    vec![
        Mesh::sphere(0.35).colored([0.75, 0.82, 0.95]),
        Mesh::cone(0.55, 0.95).colored([0.20, 0.60, 1.00]),
        Mesh::cone(0.42, 0.75)
            .rotated_x(std::f32::consts::FRAC_PI_2)
            .colored([1.00, 0.40, 0.20]),
        Mesh::torus(1.05, 0.03).colored([0.95, 0.85, 0.20]),
    ]
}

pub fn observer_hub() -> GpuHub {
    GpuHub::new(Vec3::ZERO, 0.10, Vec3::new(1.0, 0.85, 0.35))
}

pub fn hud(preset: &str, fibers: u32, particles: u32) -> Vec<HudVert> {
    let mut hud = Vec::<HudVert>::new();
    hud_text(
        &mut hud,
        -0.92,
        0.88,
        0.018,
        &format!("QGA-GPU BENCH  {preset}  fibers={fibers}  motes={particles}"),
        [0.92, 0.95, 1.0, 0.92],
    );
    hud
}
