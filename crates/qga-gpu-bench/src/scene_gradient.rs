//! ngsm lattice + optical-skyrmion field. **Model**, not Theorem, not a clone.
//!
//! Lattice (orb + one thin ring per cell): visual theme
//! "gradient / structure" by Toshiyuki Nagashima (@ngsm)
//! https://x.com/ngsm/status/2094596901345825098
//!
//! Field (rainbow, small waves, local ring tilt): Model inspired by
//! Stokes-skyrmion textures in Chen et al., Nanophotonics (2026)
//! "Vectorial Diffractive Neural Metasurfaces for Multiplexed Synthesis
//! of Optical Skyrmions", and by the gauged Hopf-lattice analog in
//! Kinder, arXiv:2607.16520. Not a port of those simulations, not a
//! p5.js sketch, not a copy of qga-app OAM/reveal scenes.
//!
//! Implementation is `draw_geodesic_orb` + fiber centerlines on qga-gpu.

use crate::args::{Args, Multiply};
use glam::{Quat, Vec3};
use qga_gpu::{hud_text, FaceVert, GpuFiber, GpuParticle, HudVert};
use std::f32::consts::{PI, TAU};

struct Cell {
    xz: [f32; 2],
    pos: Vec3,
    orb_color: Vec3,
    ring_color: Vec3,
    q: Quat,
    u: Vec3,
}

pub struct GradientLattice {
    pub fibers: Vec<GpuFiber>,
    pub particles: Vec<GpuParticle>,
    pub fabric: Vec<FaceVert>,
    pub orb_scale: f32,
    pub fluid: bool,
    cells: Vec<Cell>,
    mote_cell: Vec<u32>,
    mote_phase: Vec<f32>,
    fluid_xz: Vec<[f32; 2]>,
    fluid_y: Vec<f32>,
    n_samples: u32,
    ring_radius: f32,
    multiply: Multiply,
    span: f32,
    time: f32,
    grid: u32,
}

impl GradientLattice {
    pub fn new(args: &Args) -> Self {
        let grid = args.grid.max(1);
        let n_samples = args.fiber_samples.max(8);
        let extent = args.cell_extent;
        let span = (grid.saturating_sub(1) as f32) * extent;
        let mut cells = Vec::with_capacity((grid * grid) as usize);
        let mut fibers = Vec::with_capacity((grid * grid) as usize);
        for j in 0..grid {
            for i in 0..grid {
                let xz = cell_xz(i, j, grid, extent);
                let n = skyrmion_n(xz[0], xz[1], span, 0.0);
                let (orb_color, ring_color) = field_colors(n);
                let q = quat_align_z(n);
                cells.push(Cell {
                    xz,
                    pos: Vec3::new(xz[0], ocean_y(xz[0], xz[1], n.y, 0.0, span), xz[1]),
                    orb_color,
                    ring_color,
                    q,
                    u: n,
                });
                fibers.push(GpuFiber {
                    points: vec![Vec3::ZERO; n_samples as usize],
                    color: ring_color,
                });
            }
        }
        let mut lat = Self {
            fibers,
            particles: Vec::new(),
            fabric: Vec::new(),
            orb_scale: if args.fluid {
                args.orb_scale * 0.82
            } else {
                args.orb_scale
            },
            fluid: args.fluid,
            cells,
            mote_cell: Vec::new(),
            mote_phase: Vec::new(),
            fluid_xz: Vec::new(),
            fluid_y: Vec::new(),
            n_samples,
            ring_radius: args.ring_radius,
            multiply: args.multiply,
            span,
            time: 0.0,
            grid,
        };
        lat.rebuild_rings();
        if args.fluid {
            lat.seed_fluid(args.particles);
            lat.rebuild_fabric();
        } else if args.dirty_particles {
            lat.seed_motes(4);
        }
        lat
    }

    pub fn rebuild_rings(&mut self) {
        let n = self.n_samples as usize;
        let r = self.ring_radius;
        for (cell, fiber) in self.cells.iter().zip(self.fibers.iter_mut()) {
            let normal = (cell.q * Vec3::Z).normalize_or_zero();
            let normal = if normal.length_squared() < 0.5 {
                Vec3::Y
            } else {
                normal
            };
            let tangent = normal.any_orthonormal_vector();
            let bitan = normal.cross(tangent);
            for (k, p) in fiber.points.iter_mut().enumerate() {
                let theta = k as f32 / n as f32 * TAU;
                let (s, c) = theta.sin_cos();
                *p = cell.pos + (tangent * c + bitan * s) * r;
            }
            fiber.color = cell.ring_color;
        }
    }

    /// Skyrmion field + small traveling waves. Software fact: changes
    /// centerline bytes so `write_live_fibers` cannot hash-skip.
    pub fn tick_rings(&mut self, dtheta: f32) {
        // Slow clock: dtheta=0.012 → Δt≈0.029 / frame (~1.7 units/s at 60 Hz).
        self.time += dtheta * 2.4;
        let span = self.span;
        let t = self.time;
        let blend = HEIGHT_BLEND;
        let multiply = self.multiply;
        for cell in &mut self.cells {
            let n = skyrmion_n(cell.xz[0], cell.xz[1], span, t);
            let follow = quat_align_z(n);
            let eased = cell.q.slerp(follow, 0.06);
            let e = exp_theta_u(dtheta * 0.08, n);
            cell.q = match multiply {
                Multiply::Left => (e * eased).normalize(),
                Multiply::Right => (eased * e).normalize(),
            };
            cell.u = n;
            let target = ocean_y(cell.xz[0], cell.xz[1], n.y, t, span)
                + speaker_pump(cell.xz[0], cell.xz[1], t, span);
            cell.pos.y += (target - cell.pos.y) * blend;
            let (orb, ring) = field_colors(n);
            cell.orb_color = orb;
            cell.ring_color = ring;
        }
        self.rebuild_rings();
        if self.fluid {
            self.resample_fluid();
            self.rebuild_fabric();
        }
    }

    fn seed_motes(&mut self, per_ring: u32) {
        let per = per_ring.max(1) as usize;
        let n = self.cells.len() * per;
        self.mote_cell.clear();
        self.mote_phase.clear();
        self.particles.clear();
        self.mote_cell.reserve(n);
        self.mote_phase.reserve(n);
        self.particles.reserve(n);
        for (ci, _) in self.cells.iter().enumerate() {
            for k in 0..per {
                self.mote_cell.push(ci as u32);
                self.mote_phase.push((k as f32 + 0.5) / per as f32);
                self.particles
                    .push(GpuParticle::new(Vec3::ZERO, Vec3::ZERO, 0.22));
            }
        }
        self.resample_motes();
    }

    pub fn advance_motes(&mut self, dphase: f32) {
        if self.particles.is_empty() || self.fluid {
            return;
        }
        for ph in &mut self.mote_phase {
            *ph = (*ph + dphase).rem_euclid(1.0);
        }
        self.resample_motes();
    }

    pub fn resample_motes(&mut self) {
        if self.particles.is_empty() {
            return;
        }
        for i in 0..self.particles.len() {
            let fi = self.mote_cell[i] as usize % self.fibers.len();
            let (pos, vel) = sample_fiber(&self.fibers[fi], self.mote_phase[i]);
            let n = skyrmion_n(
                self.cells[fi].xz[0],
                self.cells[fi].xz[1],
                self.span,
                self.time,
            );
            let hue = (n.z.atan2(n.x) / TAU).rem_euclid(1.0).max(0.001);
            self.particles[i] = GpuParticle::new(pos, vel, 0.16).with_hue(hue);
        }
    }

    pub fn orb_instances(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        self.cells.iter().map(|c| (c.pos, c.orb_color))
    }

    fn seed_fluid(&mut self, n_particles: u32) {
        let n = n_particles.max(4) as usize;
        let side = (n as f32).sqrt().round().max(2.0) as usize;
        self.fluid_xz.clear();
        self.fluid_y.clear();
        self.particles.clear();
        self.fluid_xz.reserve(side * side);
        self.fluid_y.reserve(side * side);
        self.particles.reserve(side * side);
        let span = self.span.max(1e-3);
        for j in 0..side {
            for i in 0..side {
                let u = i as f32 / (side - 1) as f32;
                let v = j as f32 / (side - 1) as f32;
                let xz = [(u - 0.5) * span, (v - 0.5) * span];
                self.fluid_xz.push(xz);
                self.fluid_y.push(0.0);
                self.particles
                    .push(GpuParticle::new(Vec3::ZERO, Vec3::ZERO, 0.28));
            }
        }
        self.resample_fluid();
    }

    fn resample_fluid(&mut self) {
        let span = self.span;
        let t = self.time;
        let blend = HEIGHT_BLEND;
        for i in 0..self.particles.len() {
            let xz = self.fluid_xz[i];
            let n = skyrmion_n(xz[0], xz[1], span, t);
            let target = ocean_y(xz[0], xz[1], n.y, t, span)
                + 0.45 * speaker_pump(xz[0], xz[1], t, span)
                + 0.055;
            let y0 = self.fluid_y[i];
            let y = y0 + (target - y0) * blend;
            self.fluid_y[i] = y;
            let vel = Vec3::new(0.0, (y - y0) * 20.0, 0.0);
            let hue = (n.z.atan2(n.x) / TAU).rem_euclid(1.0).max(0.001);
            self.particles[i] =
                GpuParticle::new(Vec3::new(xz[0], y, xz[1]), vel, 0.30).with_hue(hue);
        }
    }

    fn rebuild_fabric(&mut self) {
        let g = self.grid.max(2) as usize;
        self.fabric.clear();
        self.fabric.reserve((g - 1) * (g - 1) * 6);
        let lift = 0.038;
        for j in 0..g - 1 {
            for i in 0..g - 1 {
                let ia = i + j * g;
                let ib = i + 1 + j * g;
                let ic = i + 1 + (j + 1) * g;
                let id = i + (j + 1) * g;
                let a = fabric_pt(&self.cells[ia], lift);
                let b = fabric_pt(&self.cells[ib], lift);
                let c = fabric_pt(&self.cells[ic], lift);
                let d = fabric_pt(&self.cells[id], lift);
                let col = glass_color(self.cells[ia].orb_color);
                push_tri(&mut self.fabric, a, b, c, col, 0.14);
                push_tri(&mut self.fabric, a, c, d, col, 0.14);
            }
        }
    }
}

fn fabric_pt(cell: &Cell, lift: f32) -> Vec3 {
    Vec3::new(cell.pos.x, cell.pos.y + lift, cell.pos.z)
}

fn glass_color(orb: Vec3) -> Vec3 {
    orb.lerp(Vec3::new(0.82, 0.90, 0.98), 0.55) * 0.85
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

pub fn camera_distance(args: &Args) -> f32 {
    let span = (args.grid.saturating_sub(1) as f32) * args.cell_extent;
    if args.fluid {
        // Edge-on, in the sheet: silhouette above and speakers below.
        (span * 0.32).max(2.0)
    } else {
        (args.grid as f32 * args.cell_extent * 0.95).max(2.8)
    }
}

pub fn hud(args: &Args) -> Vec<HudVert> {
    let mut hud = Vec::<HudVert>::new();
    hud_text(
        &mut hud,
        -0.92,
        0.88,
        0.016,
        &format!(
            "GRADIENT / STRUCTURE  {}  grid={}  {} (Model)",
            args.preset.as_str(),
            args.grid,
            if args.fluid {
                "speakers + glass + particle bed"
            } else {
                "skyrmion field"
            }
        ),
        [0.85, 0.90, 1.0, 0.88],
    );
    hud
}

fn cell_xz(i: u32, j: u32, grid: u32, extent: f32) -> [f32; 2] {
    let origin = (grid.saturating_sub(1) as f32) * 0.5;
    [(i as f32 - origin) * extent, (j as f32 - origin) * extent]
}

/// Néel + weaker offset meron. Model of a Stokes-skyrmion texture on XZ.
/// n.y is out-of-plane (Y-up). Ring normals follow n.
fn skyrmion_n(x: f32, z: f32, span: f32, t: f32) -> Vec3 {
    let s = span.max(1e-3);
    let n1 = baby_skyrmion(x - 0.16 * s, z + 0.04 * s, 0.40 * s, 1.0, 0.20 * t);
    let n2 = baby_skyrmion(
        x + 0.18 * s,
        z - 0.14 * s,
        0.36 * s,
        -1.0,
        0.5 * PI - 0.12 * t,
    );
    let n = (n1 + n2 * 0.72).normalize_or_zero();
    if n.length_squared() < 0.5 {
        Vec3::Y
    } else {
        n
    }
}

fn baby_skyrmion(x: f32, z: f32, r0: f32, vorticity: f32, helicity: f32) -> Vec3 {
    let rho = (x * x + z * z).sqrt();
    let phi = z.atan2(x);
    let r0 = r0.max(1e-4);
    // Θ = π at the core, → 0 at infinity (skyrmion pointing −Y at center).
    let theta = PI * (-(rho / r0).powi(2)).exp();
    let psi = vorticity * phi + helicity;
    let (s, c) = theta.sin_cos();
    let (sp, cp) = psi.sin_cos();
    Vec3::new(s * cp, c, s * sp)
}

const HEIGHT_BLEND: f32 = 0.10;

/// Spectrum of traveling waves + skyrmion swell. Model, not a PDE solve.
fn ocean_y(x: f32, z: f32, n_y: f32, t: f32, span: f32) -> f32 {
    let s = span.max(1.0);
    // (kx, kz, ω, amp) — long swell first; high-k terms stay small (no chatter).
    let modes = [
        (2.0, 1.15, 0.55, 0.22),
        (3.2, -0.85, 0.42, 0.13),
        (5.0, 0.40, 0.70, 0.07),
        (8.0, -1.10, 0.95, 0.035),
        (13.0, 0.70, 1.25, 0.018),
    ];
    let mut y = 0.0;
    for &(kx, kz, omega, amp) in &modes {
        y += amp * (TAU * kx / s * x + TAU * kz / s * z - omega * t).sin();
    }
    y += 0.10 * n_y;
    y * (s * 0.20)
}

/// Slow speaker-array drive. Kept well below the swell frequencies.
fn speaker_pump(x: f32, z: f32, t: f32, span: f32) -> f32 {
    let s = span.max(1.0);
    0.010 * s * (1.85 * t + 2.399_963 * (x + z) / (s * 0.22).max(0.05)).sin()
}

fn field_colors(n: Vec3) -> (Vec3, Vec3) {
    let hue = (n.z.atan2(n.x) / TAU).rem_euclid(1.0);
    let val = 0.42 + 0.50 * (0.5 + 0.5 * n.y);
    let orb = hsv_to_rgb(hue, 0.82, val.clamp(0.28, 1.0));
    let ring = hsv_to_rgb(hue, 0.70, (val * 1.12).clamp(0.35, 1.0));
    (orb, ring)
}

fn quat_align_z(n: Vec3) -> Quat {
    let n = n.normalize_or_zero();
    if n.length_squared() < 0.5 {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_arc(Vec3::Z, n)
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Vec3 {
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
        4 => Vec3::new(t, p, v),
        _ => Vec3::new(v, p, q),
    }
}

fn exp_theta_u(theta: f32, u: Vec3) -> Quat {
    let (s, c) = theta.sin_cos();
    Quat::from_xyzw(u.x * s, u.y * s, u.z * s, c)
}

fn sample_fiber(fiber: &GpuFiber, phase: f32) -> (Vec3, Vec3) {
    let n = fiber.points.len().max(2);
    let x = phase.rem_euclid(1.0) * n as f32;
    let i0 = (x.floor() as usize) % n;
    let i1 = (i0 + 1) % n;
    let t = x.fract();
    let a = fiber.points[i0];
    let b = fiber.points[i1];
    (a.lerp(b, t), (b - a) * n as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skyrmion_n_is_unit() {
        for x in [-2.0, 0.0, 1.5] {
            for z in [-1.0, 0.5, 2.0] {
                let n = skyrmion_n(x, z, 6.0, 0.3);
                assert!((n.length() - 1.0).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn rainbow_stays_in_unit() {
        let n = skyrmion_n(0.4, -0.2, 5.0, 0.0);
        let (o, r) = field_colors(n);
        assert!(o.min_element() >= 0.0 && o.max_element() <= 1.0);
        assert!(r.min_element() >= 0.0 && r.max_element() <= 1.0);
    }
}
