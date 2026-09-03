//! GPU records. Software fact: sizes are ABI, not QGA meaning.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

const _: () = assert!(std::mem::size_of::<FrameUniforms>() == 256);
const _: () = assert!(std::mem::size_of::<GpuParticle>() == 32);
const _: () = assert!(std::mem::size_of::<GpuFiberPoint>() == 32);
const _: () = assert!(std::mem::size_of::<GpuHub>() == 32);
const _: () = assert!(std::mem::size_of::<GpuOrbInstance>() == 32);
const _: () = assert!(std::mem::size_of::<GpuParticle>() as u64 % 8 == 0);
const _: () = assert!(std::mem::size_of::<GpuParticle>() as u64 % 4 == 0);

/// BGRA8 `bytes_per_row` for `copy_*_texture`. Portable pitch is 256, not 4/8.
pub fn copy_bytes_per_row_bgra(width: u32) -> u32 {
    let unpadded = width.saturating_mul(4);
    unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

/// Bytes to map/copy for `n` particles. 0 means do not map (`map_async` size 0).
pub fn particle_ring_need_bytes(n: usize) -> u64 {
    (n * std::mem::size_of::<GpuParticle>()) as u64
}

/// 256-byte frame uniform block. wgpu uniform offset alignment is 256.
/// Live params (aperture, height_scale, zener, time) live here so callers
/// do not retessellate when they change.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameUniforms {
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 3],
    pub time: f32,
    pub cam_right: [f32; 3],
    pub pulse: f32,
    pub cam_up: [f32; 3],
    pub glow: f32,
    pub tube_radius: f32,
    pub aperture: f32,
    pub height_scale: f32,
    pub zener: f32,
}

impl FrameUniforms {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        view: Mat4,
        proj: Mat4,
        cam_pos: Vec3,
        cam_right: Vec3,
        cam_up: Vec3,
        time: f32,
        pulse: f32,
        glow: f32,
        tube_radius: f32,
        aperture: f32,
        height_scale: f32,
        zener: f32,
    ) -> Self {
        Self {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            view_proj: (proj * view).to_cols_array_2d(),
            cam_pos: cam_pos.into(),
            time,
            cam_right: cam_right.into(),
            pulse,
            cam_up: cam_up.into(),
            glow,
            tube_radius,
            aperture,
            height_scale,
            zener,
        }
    }
}

/// One centerline sample. Vertex shader extrudes the tube.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuFiberPoint {
    pub pos: [f32; 3],
    pub along: f32,
    pub color: [f32; 3],
    pub phase: f32,
}

/// CPU-side fiber strip. Callers convert from qga-math `Fiber`.
#[derive(Clone, Debug)]
pub struct GpuFiber {
    pub points: Vec<Vec3>,
    pub color: Vec3,
}

impl GpuFiber {
    pub fn new(points: Vec<Vec3>, color: Vec3) -> Self {
        Self { points, color }
    }
}

/// Old name. Prefer [`GpuFiber`].
pub type SolidFiber = GpuFiber;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct HudVert {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

impl HudVert {
    pub fn new(pos: [f32; 2], color: [f32; 4]) -> Self {
        Self { pos, color }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuParticle {
    pub pos: [f32; 3],
    pub mass: f32,
    pub vel: [f32; 3],
    pub pad: f32,
}

impl GpuParticle {
    pub fn new(pos: Vec3, vel: Vec3, mass: f32) -> Self {
        Self {
            pos: pos.into(),
            mass,
            vel: vel.into(),
            pad: 0.0,
        }
    }

    /// `hue` in (0, 1] — particle shader uses a four-bin tint. 0 keeps the cyan/gold mix.
    pub fn with_hue(mut self, hue: f32) -> Self {
        let h = hue.rem_euclid(1.0);
        self.pad = if h < 1e-4 { 1.0 } else { h };
        self
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuHub {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
    pub pad: f32,
}

impl GpuHub {
    pub fn new(pos: Vec3, radius: f32, color: Vec3) -> Self {
        Self {
            pos: pos.into(),
            radius,
            color: color.into(),
            pad: 0.0,
        }
    }
}

/// Triangle-list vertex for heat-map faces and opacity shells.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FaceVert {
    pub pos: [f32; 3],
    pub alpha: f32,
    pub color: [f32; 3],
    pub pad: f32,
    pub nrm: [f32; 3],
    pub pad2: f32,
}

/// Line-list vertex for geodesic scaffolding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVert {
    pub pos: [f32; 3],
    pub pad: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct LineStyle {
    pub color: Vec3,
    pub width: f32,
    pub depth_bias: f32,
    pub opacity: f32,
}

impl LineStyle {
    pub fn black_hairline() -> Self {
        Self {
            color: Vec3::ZERO,
            width: 0.0018,
            depth_bias: 0.0004,
            opacity: 0.92,
        }
    }
}

/// Instanced geodesic orb. 32 bytes: translation, uniform scale, color, lod.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuOrbInstance {
    pub offset: [f32; 3],
    pub scale: f32,
    pub color: [f32; 3],
    pub lod: f32,
}

impl GpuOrbInstance {
    pub fn new(offset: Vec3, color: Vec3) -> Self {
        Self {
            offset: offset.into(),
            scale: 1.0,
            color: color.into(),
            lod: 1.0,
        }
    }

    pub fn from_transform(transform: Mat4, color: Vec3, lod: u32) -> Self {
        let offset = transform.w_axis.truncate();
        let scale = transform.x_axis.truncate().length().max(1e-6);
        Self {
            offset: offset.into(),
            scale,
            color: color.into(),
            lod: lod as f32,
        }
    }

    pub fn identity() -> Self {
        Self::new(Vec3::ZERO, Vec3::ONE)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FiberMeta {
    pub n_points: u32,
    pub n_fibers: u32,
    pub radius: f32,
    pub _pad: u32,
}
