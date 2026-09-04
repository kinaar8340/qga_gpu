//! Held lattice. Two clocks: static topology once, uniforms every frame,
//! a pulse every 30 frames for live harmonics + motes.
//!
//! **Model**, not Theorem. A frozen 32×32 centerline lattice with one live
//! correction. Same geometry field, three cadences. Not a copy of qga-app
//! realm / cosmos / OAM / reveal, and not the 65k gradient ocean (that
//! scene dirties particles every frame).

use crate::args::Args;
use glam::Vec3;
use qga_gpu::{
    hud_text, torus_centerline, FaceVert, GpuFiber, GpuHub, GpuParticle, HudVert, Mesh, VisualState,
};
use std::f32::consts::{FRAC_PI_2, TAU};

pub const LIVE_FIBERS: u32 = 12;
pub const PULSE_PERIOD: u32 = 30;
pub const DEFAULT_PARTICLES: u32 = 16_384;
pub const DEFAULT_TUBE: f32 = 0.012;

const _: () = assert!(LIVE_FIBERS >= 8 && LIVE_FIBERS <= 16);
const _: () = assert!(DEFAULT_PARTICLES == 16_384);
const _: () = assert!(DEFAULT_TUBE > 0.002 && DEFAULT_TUBE < 0.08);

pub struct HoldLattice {
    pub static_fibers: Vec<GpuFiber>,
    pub live: Vec<GpuFiber>,
    pub particles: Vec<GpuParticle>,
    pub hubs: Vec<GpuHub>,
    pub fabric: Vec<FaceVert>,
    pulse: u32,
    mote_phase: Vec<f32>,
    mote_xz: Vec<[f32; 2]>,
    n_samples: u32,
    span: f32,
}

impl HoldLattice {
    pub fn new(args: &Args) -> Self {
        let grid = args.grid.max(1);
        let n_samples = args.fiber_samples.max(8);
        let extent = args.cell_extent;
        let span = (grid.saturating_sub(1) as f32) * extent;
        let n_live = args.fibers.clamp(8, LIVE_FIBERS.max(16)) as usize;

        let mut static_fibers = Vec::with_capacity((grid * grid) as usize + 1);
        static_fibers.push(torus_centerline(
            1.85,
            0.03,
            n_samples.max(24),
            Vec3::new(0.95, 0.85, 0.20),
        ));
        for j in 0..grid {
            for i in 0..grid {
                let xz = cell_xz(i, j, grid, extent);
                static_fibers.push(static_ring(
                    xz,
                    args.ring_radius,
                    n_samples,
                    lattice_color(i, j, grid),
                ));
            }
        }

        let mut live = Vec::with_capacity(n_live);
        for _ in 0..n_live {
            live.push(GpuFiber {
                points: vec![Vec3::ZERO; n_samples as usize],
                color: Vec3::ONE,
            });
        }

        let mut lat = Self {
            static_fibers,
            live,
            particles: Vec::new(),
            hubs: sparse_hubs(grid, extent),
            fabric: glass_fabric(grid, extent, span),
            pulse: 0,
            mote_phase: Vec::new(),
            mote_xz: Vec::new(),
            n_samples,
            span,
        };
        lat.rebuild_live();
        let n_motes = if args.particles == 0 {
            DEFAULT_PARTICLES
        } else {
            args.particles
        };
        lat.seed_motes(n_motes);
        lat
    }

    /// Pulse on frame 30, 60, … — warmup is beat 0, so 300 frames → 10 live writes.
    pub fn is_pulse(frame: u32) -> bool {
        frame > 0 && frame % PULSE_PERIOD == 0
    }

    pub fn pulse_correction(&mut self) {
        self.pulse = self.pulse.wrapping_add(1);
        self.rebuild_live();
        self.resample_motes();
    }

    fn rebuild_live(&mut self) {
        let n = self.n_samples as usize;
        let n_live = self.live.len().max(1);
        let phase = self.pulse as f32 * 0.41;
        for (i, fiber) in self.live.iter_mut().enumerate() {
            let yaw = i as f32 / n_live as f32 * TAU + phase;
            let tilt = 0.32 + 0.14 * (phase + i as f32 * 0.6).sin();
            let r = 1.28 + 0.22 * (phase * 1.3 + i as f32 * 0.7).sin();
            let y0 = 0.62 + 0.10 * (phase + i as f32 * 1.1).sin();
            let (sy, cy) = yaw.sin_cos();
            let (st, ct) = tilt.sin_cos();
            for (k, p) in fiber.points.iter_mut().enumerate() {
                let th = k as f32 / n as f32 * TAU;
                let (s, c) = th.sin_cos();
                let local = Vec3::new(r * c, 0.0, r * s);
                let q = Vec3::new(
                    local.x,
                    local.y * ct - local.z * st,
                    local.y * st + local.z * ct,
                );
                *p = Vec3::new(q.x * cy + q.z * sy, q.y + y0, -q.x * sy + q.z * cy);
            }
            fiber.color = hsv(0.06 + i as f32 / n_live as f32 * 0.22, 0.72, 0.96);
        }
    }

    fn seed_motes(&mut self, n_particles: u32) {
        let n = n_particles.max(4) as usize;
        let side = (n as f32).sqrt().round().max(2.0) as usize;
        self.mote_xz.clear();
        self.mote_phase.clear();
        self.particles.clear();
        self.mote_xz.reserve(side * side);
        self.mote_phase.reserve(side * side);
        self.particles.reserve(side * side);
        let span = self.span.max(1e-3);
        for j in 0..side {
            for i in 0..side {
                let u = i as f32 / (side - 1) as f32;
                let v = j as f32 / (side - 1) as f32;
                let xz = [(u - 0.5) * span, (v - 0.5) * span];
                self.mote_xz.push(xz);
                self.mote_phase
                    .push((i as f32 * 0.17 + j as f32 * 0.31).rem_euclid(1.0));
                self.particles
                    .push(GpuParticle::new(Vec3::ZERO, Vec3::ZERO, 0.20));
            }
        }
        self.resample_motes();
    }

    fn resample_motes(&mut self) {
        let t = self.pulse as f32;
        for i in 0..self.particles.len() {
            let xz = self.mote_xz[i];
            let ph = (self.mote_phase[i] + t * 0.07).rem_euclid(1.0);
            let y = 0.05 + 0.035 * (ph * TAU).sin();
            let hue = (0.52 + ph * 0.18).rem_euclid(1.0).max(0.001);
            self.particles[i] = GpuParticle::new(
                Vec3::new(xz[0], y, xz[1]),
                Vec3::new(0.0, 0.015 * (ph * TAU).cos(), 0.0),
                0.18,
            )
            .with_hue(hue);
        }
    }

    pub fn faces(&self) -> Vec<FaceVert> {
        let mut out = Vec::new();
        for m in meshes() {
            out.extend(m.tessellate(1).faces);
        }
        out.extend_from_slice(&self.fabric);
        out
    }
}

/// Two cones. Separator torus is a static fiber, not a second retain.
pub fn meshes() -> Vec<Mesh> {
    vec![
        Mesh::cone(0.85, 1.55).colored([0.32, 0.70, 0.95]),
        Mesh::cone(0.62, 1.15)
            .rotated_x(FRAC_PI_2)
            .colored([0.95, 0.52, 0.38]),
    ]
}

/// Frame uniforms only. Does not retessellate or dirty hashes.
pub fn breathe(vis: &mut VisualState, time: f32, tube: f32) {
    vis.aperture = 0.90 + 0.10 * (time * 0.85).sin();
    vis.height_scale = 0.94 + 0.06 * (time * 0.55).sin();
    vis.zener = 2.4 + 0.70 * (time * 0.40).sin();
    let tube = if tube > 0.0 { tube } else { DEFAULT_TUBE };
    vis.tube_radius = tube * (1.0 + 0.28 * (time * 1.15).sin());
    vis.pulse = 0.40 + 0.18 * (time * 0.90).sin();
}

pub fn camera_distance(args: &Args) -> f32 {
    let span = (args.grid.saturating_sub(1) as f32) * args.cell_extent;
    // In the sheet, not a bird's-eye of the whole rug. The lattice is planar
    // (it holds), so pitch stays a little above edge-on or the field collapses
    // to a line. Same *scheme* as gradient-record: fill the frame, close.
    (span * 0.28).max(2.4)
}

pub fn hud(args: &Args) -> Vec<HudVert> {
    let mut hud = Vec::<HudVert>::new();
    hud_text(
        &mut hud,
        -0.92,
        0.88,
        0.016,
        &format!(
            "HOLD  {}  lattice={}×{}  live={}  motes={}  (Model)",
            args.preset.as_str(),
            args.grid,
            args.grid,
            args.fibers,
            args.particles
        ),
        [0.85, 0.90, 1.0, 0.88],
    );
    hud
}

fn cell_xz(i: u32, j: u32, grid: u32, extent: f32) -> [f32; 2] {
    let origin = (grid.saturating_sub(1) as f32) * 0.5;
    [(i as f32 - origin) * extent, (j as f32 - origin) * extent]
}

fn static_ring(xz: [f32; 2], radius: f32, n_samples: u32, color: Vec3) -> GpuFiber {
    let n = n_samples.max(8) as usize;
    let r = radius.max(1e-4);
    let mut points = Vec::with_capacity(n);
    for k in 0..n {
        let th = k as f32 / n as f32 * TAU;
        let (s, c) = th.sin_cos();
        points.push(Vec3::new(xz[0] + r * c, 0.0, xz[1] + r * s));
    }
    GpuFiber { points, color }
}

fn lattice_color(i: u32, j: u32, grid: u32) -> Vec3 {
    let t = (i + j) as f32 / (2 * grid.max(1)) as f32;
    hsv(0.52 + t * 0.10, 0.35, 0.55)
}

fn sparse_hubs(grid: u32, extent: f32) -> Vec<GpuHub> {
    let stride = (grid / 8).max(1);
    let mut hubs = Vec::new();
    let mut j = 0u32;
    while j < grid {
        let mut i = 0u32;
        while i < grid {
            let xz = cell_xz(i, j, grid, extent);
            hubs.push(GpuHub::new(
                Vec3::new(xz[0], 0.02, xz[1]),
                0.045,
                Vec3::new(0.95, 0.82, 0.40),
            ));
            i = i.saturating_add(stride);
            if i == 0 {
                break;
            }
        }
        j = j.saturating_add(stride);
        if j == 0 {
            break;
        }
    }
    hubs
}

fn glass_fabric(grid: u32, extent: f32, span: f32) -> Vec<FaceVert> {
    let g = grid.max(2) as usize;
    let mut fabric = Vec::with_capacity((g - 1) * (g - 1) * 6);
    let col = Vec3::new(0.62, 0.78, 0.92);
    for j in 0..g - 1 {
        for i in 0..g - 1 {
            let a = glass_pt(cell_xz(i as u32, j as u32, grid, extent), span);
            let b = glass_pt(cell_xz(i as u32 + 1, j as u32, grid, extent), span);
            let c = glass_pt(cell_xz(i as u32 + 1, j as u32 + 1, grid, extent), span);
            let d = glass_pt(cell_xz(i as u32, j as u32 + 1, grid, extent), span);
            push_tri(&mut fabric, a, b, c, col, 0.13);
            push_tri(&mut fabric, a, c, d, col, 0.13);
        }
    }
    fabric
}

fn glass_pt(xz: [f32; 2], span: f32) -> Vec3 {
    let s = span.max(1.0);
    let r = (xz[0] * xz[0] + xz[1] * xz[1]).sqrt() / s;
    Vec3::new(xz[0], 0.035 + 0.02 * (1.0 - r), xz[1])
}

fn push_tri(out: &mut Vec<FaceVert>, a: Vec3, b: Vec3, c: Vec3, color: Vec3, alpha: f32) {
    let nrm = (b - a).cross(c - a).normalize_or_zero();
    for p in [a, b, c] {
        out.push(FaceVert {
            pos: p.into(),
            alpha,
            color: color.into(),
            pad: 0.0,
            nrm: nrm.into(),
            pad2: 0.0,
        });
    }
}

fn hsv(h: f32, s: f32, v: f32) -> Vec3 {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match (i as i32).rem_euclid(6) {
        0 => Vec3::new(v, t, p),
        1 => Vec3::new(q, v, p),
        2 => Vec3::new(p, v, t),
        3 => Vec3::new(p, q, v),
        4 => Vec3::new(t, p, q),
        _ => Vec3::new(v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::parse_from;

    fn hold_args() -> Args {
        parse_from([
            "--headless",
            "--scene",
            "hold",
            "--preset",
            "4090",
            "--frames",
            "300",
            "--no-capture",
        ])
        .unwrap()
    }

    #[test]
    fn lattice_is_32_by_32_plus_torus() {
        let lat = HoldLattice::new(&hold_args());
        assert_eq!(lat.static_fibers.len(), 32 * 32 + 1);
        assert_eq!(lat.live.len(), LIVE_FIBERS as usize);
        assert_eq!(lat.hubs.len(), 64);
        assert_eq!(lat.particles.len(), DEFAULT_PARTICLES as usize);
        assert!(!lat.fabric.is_empty());
    }

    #[test]
    fn pulse_cadence_is_every_30_not_frame_zero() {
        assert!(!HoldLattice::is_pulse(0));
        assert!(!HoldLattice::is_pulse(29));
        assert!(HoldLattice::is_pulse(30));
        assert!(HoldLattice::is_pulse(270));
        assert!(!HoldLattice::is_pulse(299));
        let n = (0..300).filter(|&i| HoldLattice::is_pulse(i)).count();
        assert_eq!(n, 9);
    }

    #[test]
    fn pulse_changes_live_and_particle_bytes() {
        let mut lat = HoldLattice::new(&hold_args());
        let live0 = lat.live[0].points[0];
        let p0 = lat.particles[0].pos;
        lat.pulse_correction();
        assert!(lat.live[0].points[0].distance(live0) > 1e-4);
        assert!(
            (lat.particles[0].pos[0] - p0[0]).abs() > 1e-6
                || (lat.particles[0].pos[1] - p0[1]).abs() > 1e-6
                || (lat.particles[0].pos[2] - p0[2]).abs() > 1e-6
                || (lat.particles[0].pad - 0.0).abs() > 0.0
        );
        let static0 = lat.static_fibers[1].points[0];
        lat.pulse_correction();
        assert_eq!(lat.static_fibers[1].points[0], static0);
    }

    #[test]
    fn breathe_does_not_touch_geometry() {
        let lat = HoldLattice::new(&hold_args());
        let mut vis = VisualState::default();
        let tube = vis.tube_radius;
        breathe(&mut vis, 1.25, DEFAULT_TUBE);
        assert!((vis.aperture - 1.0).abs() > 1e-4 || (vis.height_scale - 1.0).abs() > 1e-4);
        assert_ne!(vis.tube_radius, tube);
        assert_eq!(lat.static_fibers.len(), 1025);
    }
}
