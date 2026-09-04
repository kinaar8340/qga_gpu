//! Photonic fabric loom. **Model**, not Theorem, not a fabricated device.
//!
//! Three layers, three knobs:
//! - Warp/weft Cartesian grid: shared frame (`retain_static_fibers`).
//! - Latitude stitches: the program (`write_live_fibers`). Cell data is
//!   (θ, φ, ψ, w); the 3D tube is the inverse-Hopf circle over p=(θ,φ),
//!   not a 2D squiggle.
//! - Sparkle dust: fill field (`write_particles`).
//!
//! Inverse stereographic lifts the chart onto S² (Smith video backward).
//! Inverse Hopf sends each enabled sample to a circle in R³. Sampling
//! *latitudes* (not every grid crossing) is what makes nested tori instead
//! of a warped cage.
//!
//! Elliptic default: cells near θ=π/4, π/2, 3π/4. Needles (high θ) first,
//! trunk last. Not a copy of inner_cone mosaic / hull.

use crate::args::{Args, Flux, Multiply};
use crate::hopf::{hsv_to_rgb, orbit, stereo_vis};
use glam::{Quat, Vec3};
use qga_gpu::{hud_text, FaceVert, GpuFiber, GpuParticle, HudVert};
use std::collections::BTreeSet;
use std::f32::consts::{PI, TAU};

const VIS: f32 = 1.75;
const CHART_SCALE: f32 = 1.15;
const CHART_Y: f32 = -4.55;
const CHART_L: f32 = 2.70;
const ETCH: f32 = 0.028;
const ELL: f32 = 2.0;
const HOPF_U: Vec3 = Vec3::X;
const PHI_BINS: i32 = 48;
const RIVER_PHI: f32 = 0.0;

/// Needles first (south / high θ), then equator, then inner trunk.
const LATITUDES: [f32; 3] = [3.0 * PI / 4.0, PI / 2.0, PI / 4.0];
const LAT_HUE: [f32; 3] = [0.08, 0.50, 0.78];

struct Stitch {
    uv: [f32; 2],
    n: Vec3,
    phi: f32,
    psi: f32,
    lat: u8,
}

pub struct LoomBraid {
    pub static_fibers: Vec<GpuFiber>,
    pub live: Vec<GpuFiber>,
    pub particles: Vec<GpuParticle>,
    pub fabric: Vec<FaceVert>,
    stitches: Vec<Stitch>,
    n_grid: u32,
    n_samples: u32,
    mosaic: u32,
    multiply: Multiply,
    flux: Flux,
    lambda: f32,
    phase: f32,
    grow: f32,
    ell: f32,
    mote_fiber: Vec<u32>,
    mote_phase: Vec<f32>,
    fluid: bool,
    fluid_pos: Vec<Vec3>,
}

impl LoomBraid {
    pub fn new(args: &Args) -> Self {
        let n_grid = args.grid.max(2);
        let n_samples = args.fiber_samples.max(8);
        let mosaic = args.mosaic.max(1);
        let grow = if args.dirty_fibers { 0.0 } else { 1.0 };
        let mut loom = Self {
            static_fibers: chart_static(n_grid, mosaic),
            live: Vec::new(),
            particles: Vec::new(),
            fabric: chart_disk(mosaic),
            stitches: Vec::new(),
            n_grid,
            n_samples,
            mosaic,
            multiply: args.multiply,
            flux: args.flux,
            lambda: args.lambda.clamp(0.0, 1.0),
            phase: 0.0,
            grow,
            ell: ELL,
            mote_fiber: Vec::new(),
            mote_phase: Vec::new(),
            fluid: args.fluid,
            fluid_pos: Vec::new(),
        };
        loom.rebuild_live();
        if args.fluid {
            loom.seed_fluid(args.particles);
        } else if args.particles > 0 {
            loom.seed_motes(args.particles);
        }
        loom
    }

    /// Grow outer needles, then rotate φ (transmission-line). Live bytes change.
    pub fn tick_braid(&mut self, frame: u32) {
        let a = frame as f32 * 0.011;
        self.phase = a * 1.65;
        self.grow = (frame as f32 / 90.0).clamp(0.0, 1.0);
        self.rebuild_live();
    }

    fn rebuild_live(&mut self) {
        self.stitches = collect_stitches(
            self.n_grid,
            self.mosaic,
            self.flux,
            self.lambda,
            self.phase,
            self.grow,
        );
        let n = self.n_samples as usize;
        let multiply = self.multiply;
        let ell = self.ell;
        self.live.resize(
            self.stitches.len(),
            GpuFiber {
                points: vec![Vec3::ZERO; n],
                color: Vec3::ONE,
            },
        );
        for (idx, stitch) in self.stitches.iter().enumerate() {
            if self.live[idx].points.len() != n {
                self.live[idx].points = vec![Vec3::ZERO; n];
            }
            let q0 = hopf_section(stitch.n);
            let mut prev = Vec3::ZERO;
            for (s, p) in self.live[idx].points.iter_mut().enumerate() {
                let theta = s as f32 / n as f32 * TAU + stitch.psi;
                let q = orbit(q0, theta, HOPF_U, multiply);
                let mut pt = stereo_vis(q, VIS);
                if s > 0 {
                    pt = spiral_etch(pt, pt - prev, theta, ell, ETCH);
                }
                prev = pt;
                *p = pt + tile_world3(stitch, self.mosaic);
            }
            self.live[idx].color = stitch_color(stitch);
        }
    }

    fn seed_motes(&mut self, n_particles: u32) {
        let n_particles = n_particles as usize;
        let n_live = self.live.len().max(1);
        self.mote_fiber.clear();
        self.mote_phase.clear();
        self.particles.clear();
        for i in 0..n_particles {
            let fi = (i % n_live) as u32;
            let lane = i / n_live;
            let lanes = (n_particles / n_live).max(1);
            let phase = (lane as f32 + 0.5) / lanes as f32;
            self.mote_fiber.push(fi);
            self.mote_phase.push(phase.fract());
            self.particles
                .push(GpuParticle::new(Vec3::ZERO, Vec3::ZERO, 0.16));
        }
        self.resample_motes();
    }

    fn seed_fluid(&mut self, n_particles: u32) {
        let n = n_particles.max(8) as usize;
        self.fluid_pos.clear();
        self.particles.clear();
        for i in 0..n {
            let t = (i as f32 + 0.5) / n as f32;
            let ga = i as f32 * 2.399_963_2;
            let r = (t.sqrt() * 0.92 + 0.08) * VIS * 1.15;
            let y = (t - 0.5) * VIS * 1.6;
            let (s, c) = ga.sin_cos();
            let pos = Vec3::new(r * c, y, r * s);
            self.fluid_pos.push(pos);
            self.particles
                .push(GpuParticle::new(pos, Vec3::ZERO, 0.22));
        }
        self.resample_fluid();
    }

    pub fn advance_motes(&mut self, dphase: f32) {
        if self.fluid {
            self.resample_fluid();
            return;
        }
        if self.particles.is_empty() || self.live.is_empty() {
            return;
        }
        for ph in &mut self.mote_phase {
            *ph = (*ph + dphase).rem_euclid(1.0);
        }
        self.resample_motes();
    }

    fn resample_motes(&mut self) {
        let n_live = self.live.len();
        if n_live == 0 {
            return;
        }
        for i in 0..self.particles.len() {
            let fi = self.mote_fiber[i] as usize % n_live;
            let (pos, vel) = sample_fiber(&self.live[fi], self.mote_phase[i]);
            let hue = stitch_hue(&self.stitches[fi], self.mote_phase[i]);
            self.particles[i] = GpuParticle::new(pos, vel, 0.13).with_hue(hue);
        }
    }

    fn resample_fluid(&mut self) {
        let t = self.phase * 0.15;
        for i in 0..self.particles.len() {
            let p = self.fluid_pos[i];
            let hue = (p.z.atan2(p.x) / TAU + p.y * 0.12 + t)
                .rem_euclid(1.0)
                .max(0.001);
            let vel = Vec3::new(-p.z, 0.02, p.x) * 0.04;
            self.particles[i] = GpuParticle::new(p, vel, 0.20).with_hue(hue);
        }
    }

    pub fn orb_centers(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        let center = (chart_to_world(0.0, 0.0), hsv_to_rgb(0.33, 0.70, 0.62));
        std::iter::once(center).chain(self.stitches.iter().map(|s| {
            (
                chart_to_world(s.uv[0], s.uv[1]),
                stitch_color(s),
            )
        }))
    }

    pub fn orb_scale(&self) -> f32 {
        (0.55 / (self.n_grid as f32).sqrt()).clamp(0.028, 0.07)
    }
}

fn collect_stitches(
    n_grid: u32,
    mosaic: u32,
    flux: Flux,
    lambda: f32,
    phase: f32,
    grow: f32,
) -> Vec<Stitch> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::<(u8, i32)>::new();
    let m = mosaic.max(1);
    let n_phi = n_grid.max(8);
    for ty in 0..m {
        for tx in 0..m {
            let off = tile_offset_uv(tx, ty, m);
            // Force samples onto latitudes. Cartesian crossings are the
            // static frame, not live tubes.
            for (k, &th) in LATITUDES.iter().enumerate() {
                if !lat_open(k, grow) {
                    continue;
                }
                for i in 0..n_phi {
                    let phi = i as f32 / n_phi as f32 * TAU + phase;
                    push_stitch(
                        &mut out,
                        &mut seen,
                        k as u8,
                        th,
                        phi,
                        lambda,
                        phase,
                        off,
                    );
                }
            }
            if flux == Flux::Hyperbolic {
                let n_th = (n_grid / 2).max(6);
                for i in 0..n_th {
                    let t = (i as f32 + 0.5) / n_th as f32;
                    let th = PI / 6.0 + t * (2.0 * PI / 3.0);
                    if !lat_open(0, grow) {
                        continue;
                    }
                    push_stitch(
                        &mut out,
                        &mut seen,
                        255,
                        th,
                        RIVER_PHI + phase,
                        lambda,
                        phase,
                        off,
                    );
                }
            }
        }
    }
    out
}

fn push_stitch(
    out: &mut Vec<Stitch>,
    seen: &mut BTreeSet<(u8, i32)>,
    lat: u8,
    th: f32,
    phi: f32,
    lambda: f32,
    phase: f32,
    off: [f32; 2],
) {
    let bin = phi_bin(phi);
    if !seen.insert((lat, bin)) {
        return;
    }
    let psi = (1.0 - lambda) * phi + lambda * phase;
    let uv_local = sph_to_gamma(th, phi);
    let n = inv_stereo_s2(uv_local[0], uv_local[1]);
    out.push(Stitch {
        uv: [uv_local[0] + off[0], uv_local[1] + off[1]],
        n,
        phi,
        psi,
        lat,
    });
}

fn lat_open(k: usize, grow: f32) -> bool {
    grow + 1e-4 >= k as f32 / 3.0
}

fn phi_bin(phi: f32) -> i32 {
    let t = (phi / TAU).rem_euclid(1.0) * PHI_BINS as f32;
    t.round() as i32 % PHI_BINS
}

fn tile_offset_uv(tx: u32, ty: u32, m: u32) -> [f32; 2] {
    if m <= 1 {
        return [0.0, 0.0];
    }
    let span = 2.0 * CHART_L + 0.55;
    let o = (m as f32 - 1.0) * 0.5;
    [(tx as f32 - o) * span, (ty as f32 - o) * span]
}

fn tile_world3(s: &Stitch, mosaic: u32) -> Vec3 {
    if mosaic <= 1 {
        return Vec3::ZERO;
    }
    Vec3::new(s.uv[0] * 0.22, 0.0, s.uv[1] * 0.22)
}

fn sph_to_gamma(theta: f32, phi: f32) -> [f32; 2] {
    let rho = (theta * 0.5).tan();
    let (s, c) = phi.sin_cos();
    [rho * c, rho * s]
}

/// Inverse stereographic Γ = u + i v → n ∈ S².
pub(crate) fn inv_stereo_s2(u: f32, v: f32) -> Vec3 {
    let r2 = u * u + v * v;
    let d = (1.0 + r2).max(1e-8);
    Vec3::new(2.0 * u / d, 2.0 * v / d, (1.0 - r2) / d)
}

/// Hopf section: one S³ lift of n ∈ S². Fiber is left S¹ of quaternion i.
pub(crate) fn hopf_section(n: Vec3) -> Quat {
    let n = n.normalize_or_zero();
    let n = if n.length_squared() < 0.5 {
        Vec3::Z
    } else {
        n
    };
    let nz = n.z.clamp(-1.0, 1.0);
    let ca = ((1.0 + nz) * 0.5).max(0.0).sqrt();
    let sa = ((1.0 - nz) * 0.5).max(0.0).sqrt();
    let psi = n.y.atan2(n.x);
    let (s, c) = psi.sin_cos();
    Quat::from_xyzw(ca * s, sa, 0.0, ca * c).normalize()
}

fn chart_to_world(u: f32, v: f32) -> Vec3 {
    Vec3::new(u * CHART_SCALE, CHART_Y, v * CHART_SCALE)
}

fn stitch_color(s: &Stitch) -> Vec3 {
    let base = if s.lat < 3 {
        LAT_HUE[s.lat as usize]
    } else {
        0.18
    };
    let hue = (base + 0.07 * s.psi / TAU).rem_euclid(1.0);
    hsv_to_rgb(hue, 0.88, 0.78)
}

fn stitch_hue(s: &Stitch, phase: f32) -> f32 {
    let h = (s.phi / TAU + phase).rem_euclid(1.0);
    if h < 1e-4 {
        1.0
    } else {
        h
    }
}

fn chart_static(n_grid: u32, mosaic: u32) -> Vec<GpuFiber> {
    let mut out = grid_lines(n_grid, mosaic);
    out.push(circle_gamma(1.0, 0.0, 0.0, 64, hsv_to_rgb(0.55, 0.28, 0.42)));
    out
}

fn grid_lines(n: u32, mosaic: u32) -> Vec<GpuFiber> {
    let n = n.max(2);
    let col = hsv_to_rgb(0.58, 0.10, 0.28);
    let pts = (n + 1).max(8);
    let m = mosaic.max(1);
    let mut out = Vec::with_capacity(2 * (n as usize + 1) * m as usize * m as usize);
    for ty in 0..m {
        for tx in 0..m {
            let off = tile_offset_uv(tx, ty, m);
            for k in 0..=n {
                let t = k as f32 / n as f32;
                let x = (t - 0.5) * 2.0 * CHART_L;
                let mut vpts = Vec::with_capacity(pts as usize);
                let mut hpts = Vec::with_capacity(pts as usize);
                for s in 0..pts {
                    let y = (s as f32 / (pts - 1) as f32 - 0.5) * 2.0 * CHART_L;
                    vpts.push(chart_to_world(x + off[0], y + off[1]));
                    hpts.push(chart_to_world(y + off[0], x + off[1]));
                }
                out.push(GpuFiber {
                    points: vpts,
                    color: col,
                });
                out.push(GpuFiber {
                    points: hpts,
                    color: col,
                });
            }
        }
    }
    out
}

fn circle_gamma(rho: f32, cu: f32, cv: f32, n: u32, color: Vec3) -> GpuFiber {
    let n = n.max(8) as usize;
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let th = i as f32 / n as f32 * TAU;
        let (s, c) = th.sin_cos();
        points.push(chart_to_world(cu + rho * c, cv + rho * s));
    }
    GpuFiber { points, color }
}

fn chart_disk(mosaic: u32) -> Vec<FaceVert> {
    let segs = 48u32;
    let mut faces = Vec::new();
    let y = CHART_Y - 0.03;
    let col = Vec3::new(0.18, 0.24, 0.32);
    let nrm = Vec3::Y;
    let m = mosaic.max(1);
    let radius = CHART_L * CHART_SCALE * 1.02;
    for ty in 0..m {
        for tx in 0..m {
            let off = tile_offset_uv(tx, ty, m);
            let origin = chart_to_world(off[0], off[1]);
            let origin = Vec3::new(origin.x, y, origin.z);
            for i in 0..segs {
                let a0 = i as f32 / segs as f32 * TAU;
                let a1 = (i + 1) as f32 / segs as f32 * TAU;
                let b0 = origin + Vec3::new(radius * a0.cos(), 0.0, radius * a0.sin());
                let b1 = origin + Vec3::new(radius * a1.cos(), 0.0, radius * a1.sin());
                for p in [origin, b0, b1] {
                    faces.push(FaceVert {
                        pos: p.into(),
                        alpha: 0.11,
                        color: col.into(),
                        pad: 0.0,
                        nrm: nrm.into(),
                        pad2: 0.0,
                    });
                }
            }
        }
    }
    faces
}

fn spiral_etch(p: Vec3, tangent: Vec3, theta: f32, ell: f32, amp: f32) -> Vec3 {
    let t = tangent.normalize_or_zero();
    let n1 = if t.length_squared() < 0.5 {
        Vec3::X
    } else {
        t.any_orthonormal_vector()
    };
    let n2 = t.cross(n1);
    let (s, c) = (ell * theta).sin_cos();
    p + (n1 * c + n2 * s) * amp
}

/// u ∈ [0, 1) → power-exponent azimuth. n = 1 is identity.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn power_exponent_u(u: f32, n: f32) -> f32 {
    let n = n.clamp(0.08, 4.0);
    let x = 2.0 * u.rem_euclid(1.0) - 1.0;
    let y = x.signum() * x.abs().powf(n);
    (y + 1.0) * 0.5
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

pub fn camera_distance(_args: &Args) -> f32 {
    12.0
}

pub fn hud(args: &Args, live: u32) -> Vec<HudVert> {
    let mut hud = Vec::<HudVert>::new();
    hud_text(
        &mut hud,
        -0.92,
        0.88,
        0.016,
        &format!(
            "LOOM  {}  {} {}x{}  lam={:.2}  mosaic={}  live={}  (Model)",
            args.flux.as_str(),
            args.preset.as_str(),
            args.grid,
            args.grid,
            args.lambda,
            args.mosaic,
            live
        ),
        [0.88, 0.92, 1.0, 0.90],
    );
    hud
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::parse_from;
    use crate::hopf::hopf_s2;

    #[test]
    fn power_exponent_identity_at_n1() {
        for k in 0..16 {
            let u = k as f32 / 16.0;
            assert!((power_exponent_u(u, 1.0) - u).abs() < 1e-5);
        }
    }

    #[test]
    fn power_exponent_n3_bunches_center() {
        let a = power_exponent_u(0.25, 3.0);
        let b = power_exponent_u(0.50, 3.0);
        let c = power_exponent_u(0.75, 3.0);
        assert!(a > 0.25);
        assert!((b - 0.5).abs() < 1e-5);
        assert!(c < 0.75);
    }

    #[test]
    fn inv_stereo_matched_is_north_pole() {
        let n = inv_stereo_s2(0.0, 0.0);
        assert!((n - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn inv_stereo_unit_circle_is_equator() {
        for k in 0..8 {
            let a = k as f32 / 8.0 * TAU;
            let n = inv_stereo_s2(a.cos(), a.sin());
            assert!(n.z.abs() < 1e-5);
            assert!((n.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn hopf_section_maps_back() {
        for &(u, v) in &[(0.0, 0.0), (0.4, 0.0), (0.0, 0.5), (0.3, -0.25), (0.7, 0.2)] {
            let n = inv_stereo_s2(u, v);
            let q = hopf_section(n);
            let m = hopf_s2(q);
            assert!((m - n).length() < 2e-4, "u={u} v={v} n={n:?} m={m:?}");
        }
    }

    #[test]
    fn hopf_fiber_preserves_base() {
        let n = inv_stereo_s2(0.4, 0.25);
        let q0 = hopf_section(n);
        for k in 0..16 {
            let q = orbit(q0, k as f32 / 16.0 * TAU, HOPF_U, Multiply::Left);
            let m = hopf_s2(q);
            assert!((m - n).length() < 2e-4);
        }
    }

    #[test]
    fn chart_arc_lifts_to_hopf_band_not_spherical_cap() {
        let n_fibers = 10;
        let n_samples = 48;
        let mut pts = Vec::new();
        for i in 0..n_fibers {
            let t = i as f32 / (n_fibers - 1) as f32;
            let phi = 0.30 + t * 0.70;
            let n = inv_stereo_s2(0.55 * phi.cos(), 0.55 * phi.sin());
            let q0 = hopf_section(n);
            for k in 0..n_samples {
                let theta = k as f32 / n_samples as f32 * TAU;
                let q = orbit(q0, theta, HOPF_U, Multiply::Left);
                pts.push(stereo_vis(q, VIS));
            }
        }
        assert!(pts.iter().all(|p| p.is_finite()));
        let rs: Vec<f32> = pts.iter().map(|p| p.length()).collect();
        let rmin = rs.iter().copied().fold(f32::INFINITY, f32::min);
        let rmax = rs.iter().copied().fold(0.0_f32, f32::max);
        let rmean = rs.iter().copied().sum::<f32>() / rs.len() as f32;
        assert!(rmean > 0.15);
        assert!(
            (rmax - rmin) / rmean > 0.30,
            "shell thickness {} / mean {} looks like a cap",
            rmax - rmin,
            rmean
        );
    }

    #[test]
    fn elliptic_16_enables_three_latitudes_not_the_whole_grid() {
        let args = parse_from(["--scene", "loom", "--preset", "smoke", "--grid", "16"]).unwrap();
        let loom = LoomBraid::new(&args);
        assert_eq!(args.flux, Flux::Elliptic);
        assert!(loom.live.len() >= 24, "live={}", loom.live.len());
        assert!(
            loom.live.len() < 16 * 16,
            "emitted a fiber from every cell ({})",
            loom.live.len()
        );
        let mut nz = [Vec::new(), Vec::new(), Vec::new()];
        for s in &loom.stitches {
            assert!(s.lat < 3, "elliptic should not emit river stitches");
            nz[s.lat as usize].push(s.n.z);
        }
        for (k, zs) in nz.iter().enumerate() {
            assert!(!zs.is_empty(), "latitude {k} empty");
            let mean = zs.iter().sum::<f32>() / zs.len() as f32;
            let expect = LATITUDES[k].cos();
            assert!(
                (mean - expect).abs() < 0.08,
                "lat {k} mean nz={mean} expect {expect}"
            );
        }
        let static_n = 2 * (16 + 1) + 1;
        assert_eq!(loom.static_fibers.len(), static_n);
    }

    #[test]
    fn snapped_samples_sit_on_latitudes() {
        let args = parse_from(["--scene", "loom", "--preset", "smoke"]).unwrap();
        let loom = LoomBraid::new(&args);
        for s in &loom.stitches {
            let th = s.n.z.clamp(-1.0, 1.0).acos();
            let lat = LATITUDES[s.lat as usize];
            assert!(
                (th - lat).abs() < 0.02,
                "θ={th} not snapped to {lat} (lat {})",
                s.lat
            );
        }
    }

    #[test]
    fn neighboring_stitches_are_distinct_fibers() {
        let args = parse_from(["--scene", "loom", "--preset", "smoke"]).unwrap();
        let loom = LoomBraid::new(&args);
        let mut keys = BTreeSet::new();
        for s in &loom.stitches {
            assert!(keys.insert((s.lat, phi_bin(s.phi))), "duplicate p");
        }
    }

    #[test]
    fn tick_braid_moves_live_centerlines() {
        let args = parse_from(["--scene", "loom", "--preset", "smoke", "--dirty-fibers"]).unwrap();
        let mut loom = LoomBraid::new(&args);
        assert!(!loom.live.is_empty());
        let before = loom.live[0].points[4];
        loom.tick_braid(17);
        assert!(!loom.live.is_empty());
        let after = loom.live[0].points[4];
        assert!((before - after).length() > 1e-4);
    }
}
