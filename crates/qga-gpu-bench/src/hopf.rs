//! Hopf-fiber field and flux motes for the bench binary.
//!
//! **Model**, not Theorem. Unit-quaternion orbits
//! `q(θ) = exp(θ u) * q0` (left) or `q0 * exp(θ u)` (right)
//! for a purely imaginary unit `u`, sampled with `glam::Quat` only.
//! Centerline xyz is stereographic projection S³ → R³
//! `(qx, qy, qz) * vis / max(1 + qw, ε)`, then a 3-shell world scale.
//! Same samples / tube as the preset; the shells spread the field so
//! additive overlap does not clip RGB. Color is RGB phase: fiber albedo
//! from the Hopf map S³ → S² (azimuth hue), motes from that hue plus
//! S¹ phase along the fiber.
//! This is a graphics generator. It is not a claim about the QGA Z-map,
//! and it does not import `qga-math`.

use crate::args::Multiply;
use glam::{Quat, Vec3};
use qga_gpu::{GpuFiber, GpuParticle};
use std::f32::consts::{FRAC_PI_2, TAU};

/// Stereographic scale. Same 64-sample tubes as the preset; larger volume
/// so neighboring fibers leave gaps for RGB. Software fact of this binary.
const VIS: f32 = 4.6;
const STEREO_FLOOR: f32 = 0.16;
const SHELLS: u32 = 3;

pub struct HopfField {
    pub fibers: Vec<GpuFiber>,
    pub particles: Vec<GpuParticle>,
    q0: Vec<Quat>,
    mote_fiber: Vec<u32>,
    mote_phase: Vec<f32>,
    multiply: Multiply,
    u: Vec3,
    n_orbs: u32,
    n_samples: u32,
}

impl HopfField {
    pub fn new(
        n_fibers: u32,
        n_samples: u32,
        n_particles: u32,
        n_orbs: u32,
        multiply: Multiply,
    ) -> Self {
        let n_fibers = n_fibers.max(1);
        let n_samples = n_samples.max(2);
        let mut fibers = Vec::with_capacity(n_fibers as usize);
        let mut q0 = Vec::with_capacity(n_fibers as usize);
        for i in 0..n_fibers {
            let q = hopf_q0(i, n_fibers);
            q0.push(q);
            fibers.push(GpuFiber {
                points: vec![Vec3::ZERO; n_samples as usize],
                color: fiber_rgb(q),
            });
        }
        let mut field = Self {
            fibers,
            particles: Vec::new(),
            q0,
            mote_fiber: Vec::new(),
            mote_phase: Vec::new(),
            multiply,
            u: Vec3::Z,
            n_orbs,
            n_samples,
        };
        field.rebuild_fibers();
        field.seed_motes(n_particles);
        field
    }

    pub fn set_generator(&mut self, u: Vec3) {
        self.u = u.normalize_or_zero();
        if self.u.length_squared() < 0.5 {
            self.u = Vec3::Z;
        }
    }

    /// Rotate the 1-parameter subgroup generator. Software fact: changes
    /// centerline bytes so `write_live_fibers` cannot hash-skip.
    pub fn tick_generator(&mut self, frame: u32) {
        let a = frame as f32 * 0.011;
        let (s, c) = a.sin_cos();
        self.set_generator(Vec3::new(s * 0.55, c, 0.35 + 0.20 * (a * 0.5).sin()));
        self.rebuild_fibers();
    }

    pub fn rebuild_fibers(&mut self) {
        let n = self.n_samples as usize;
        let u = self.u;
        let multiply = self.multiply;
        for (fi, (fiber, &q0)) in self.fibers.iter_mut().zip(self.q0.iter()).enumerate() {
            let shell = shell_scale(fi as u32);
            for (k, p) in fiber.points.iter_mut().enumerate() {
                let theta = k as f32 / n as f32 * TAU;
                *p = stereo(orbit(q0, theta, u, multiply)) * shell;
            }
        }
    }

    fn seed_motes(&mut self, n_particles: u32) {
        let n_particles = n_particles as usize;
        let n_fibers = self.fibers.len().max(1);
        self.mote_fiber.clear();
        self.mote_phase.clear();
        self.particles.clear();
        self.mote_fiber.reserve(n_particles);
        self.mote_phase.reserve(n_particles);
        self.particles.reserve(n_particles);
        for i in 0..n_particles {
            let fi = (i % n_fibers) as u32;
            let lane = i / n_fibers;
            let lanes = (n_particles / n_fibers).max(1);
            let phase = (lane as f32 + 0.5) / lanes as f32;
            self.mote_fiber.push(fi);
            self.mote_phase.push(phase.fract());
            self.particles
                .push(GpuParticle::new(Vec3::ZERO, Vec3::ZERO, 0.35));
        }
        self.resample_motes();
    }

    pub fn advance_motes(&mut self, dphase: f32) {
        for ph in &mut self.mote_phase {
            *ph = (*ph + dphase).rem_euclid(1.0);
        }
        self.resample_motes();
    }

    pub fn resample_motes(&mut self) {
        let n_fibers = self.fibers.len();
        for i in 0..self.particles.len() {
            let fi = self.mote_fiber[i] as usize % n_fibers;
            let (pos, vel) = sample_fiber(&self.fibers[fi], self.mote_phase[i]);
            let hue = s1_hue(self.q0[fi], self.mote_phase[i]);
            self.particles[i] = GpuParticle::new(pos, vel, 0.14).with_hue(hue);
        }
    }

    /// Stereographic image of each fiber's `q0`, first `n_orbs` fibers.
    pub fn orb_centers(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        let n = self.n_orbs as usize;
        self.q0
            .iter()
            .copied()
            .take(n)
            .enumerate()
            .map(|(i, q)| (stereo(q) * shell_scale(i as u32), fiber_rgb(q)))
    }

    pub fn orb_scale(&self) -> f32 {
        (1.4 / (self.n_orbs.max(1) as f32).sqrt()).clamp(0.03, 0.12)
    }
}

fn hopf_q0(i: u32, n: u32) -> Quat {
    // Hopf coordinates on S³ ≅ unit quaternions. Model, not Theorem.
    // z1 = cos α · e^{i β}, z2 = sin α · e^{i γ}
    // q.w = Re z1, q.x = Im z1, q.y = Re z2, q.z = Im z2
    let t = (i as f32 + 0.5) / n.max(1) as f32;
    let alpha = t.sqrt() * FRAC_PI_2;
    let ga = 2.399_963_2;
    let beta = i as f32 * ga;
    let gamma = i as f32 * ga * 1.618_034;
    let (sa, ca) = alpha.sin_cos();
    let (sb, cb) = beta.sin_cos();
    let (sg, cg) = gamma.sin_cos();
    Quat::from_xyzw(ca * sb, sa * cg, sa * sg, ca * cb).normalize()
}

/// exp(θ u) = cos θ + u sin θ for unit imaginary u (glam xyz + real w).
fn exp_theta_u(theta: f32, u: Vec3) -> Quat {
    let (s, c) = theta.sin_cos();
    Quat::from_xyzw(u.x * s, u.y * s, u.z * s, c)
}

fn orbit(q0: Quat, theta: f32, u: Vec3, multiply: Multiply) -> Quat {
    let e = exp_theta_u(theta, u);
    match multiply {
        Multiply::Left => e * q0,
        Multiply::Right => q0 * e,
    }
}

fn stereo(q: Quat) -> Vec3 {
    let den = (1.0 + q.w).max(STEREO_FLOOR);
    Vec3::new(q.x, q.y, q.z) * (VIS / den)
}

/// Nested Hopf shells. Same sample count per tube; world radius grows.
fn shell_scale(i: u32) -> f32 {
    0.70 + (i % SHELLS) as f32 * 0.85
}

/// Hopf map S³ → S². Model, not Theorem.
/// z1 = w + i x, z2 = y + i z → (2 z1 conj(z2), |z1|² − |z2|²).
fn hopf_s2(q: Quat) -> Vec3 {
    let w = q.w;
    let x = q.x;
    let y = q.y;
    let z = q.z;
    Vec3::new(
        2.0 * (w * y + x * z),
        2.0 * (x * y - w * z),
        w * w + x * x - y * y - z * z,
    )
    .normalize_or_zero()
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

fn fiber_rgb(q: Quat) -> Vec3 {
    let n = hopf_s2(q);
    let hue = n.y.atan2(n.x) / TAU;
    let val = 0.58 + 0.22 * n.z.clamp(-1.0, 1.0);
    hsv_to_rgb(hue, 0.90, val)
}

/// Mote hue = Hopf S² azimuth + S¹ phase along the fiber. Stays in (0, 1]
/// so the particle shader takes the RGB wheel, not the hue=0 speed mix.
fn s1_hue(q: Quat, phase: f32) -> f32 {
    let n = hopf_s2(q);
    let h = (n.y.atan2(n.x) / TAU + phase).rem_euclid(1.0);
    if h < 1e-4 {
        1.0
    } else {
        h
    }
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
    fn left_orbit_closes_at_tau() {
        let q0 = hopf_q0(7, 64);
        let a = orbit(q0, 0.0, Vec3::Z, Multiply::Left);
        let b = orbit(q0, TAU, Vec3::Z, Multiply::Left);
        assert!((a.x - b.x).abs() < 1e-5);
        assert!((a.w - b.w).abs() < 1e-5);
    }

    #[test]
    fn stereo_is_finite() {
        for i in 0..128 {
            let p = stereo(hopf_q0(i, 128));
            assert!(p.is_finite());
            assert!(p.length() < 80.0);
        }
    }

    #[test]
    fn shells_are_distinct() {
        assert!((shell_scale(0) - shell_scale(1)).abs() > 0.5);
        assert!((shell_scale(1) - shell_scale(2)).abs() > 0.5);
        assert!((shell_scale(0) - shell_scale(3)).abs() < 1e-6);
    }

    #[test]
    fn hopf_s2_is_unit_and_rgb_in_range() {
        for i in 0..64 {
            let q = hopf_q0(i, 64);
            let n = hopf_s2(q);
            assert!((n.length() - 1.0).abs() < 1e-4);
            let c = fiber_rgb(q);
            assert!(c.min_element() >= 0.0 && c.max_element() <= 1.0);
        }
    }
}
