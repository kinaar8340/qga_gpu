//! Coincident-addressed topological volume. **Model**, not Theorem.
//!
//! Core-memory analog on the existing orb lattice: \(X_i\) / \(Y_j\) half-select,
//! \(Z\) inhibit, \(S\) sense. The displayed quantity is a Stokes-skyrmion
//! library id, not an RGB triple. Observables (Zhang et al., arXiv:2608.29551,
//! **not a port of that chip**):
//!
//! - \(\mathbf{n}(\mathbf{r})=\mathbf{S}/|\mathbf{S}|\)
//! - discrete \(N_\mathrm{sk}=\frac{1}{4\pi}\int\mathbf{n}\cdot(\partial_x\mathbf{n}\times\partial_y\mathbf{n})\,dA\)
//! - library {Néel, Bloch, anti, bimeron}
//!
//! Software fact of this binary: FrameUniforms stay 256 bytes. The lattice is
//! a packed ocean manifold with a Rankine whirlpool: swirl \(\propto 1/r\)
//! outside a core. Free-surface height is Bernoulli \(\eta=-v_\theta^2\) mapped
//! with the same \(\Omega\) as the swirl (VEL deepens the hole as \(\Omega^2\)).
//! Swell stays outside the core so the funnel is a smooth \(\eta(x,z)\). Inner
//! cells alpha \(> 0.5\), outer cells alpha \(< 0.5\). Class I icosahedral
//! geodesic polyhedra (\(T=n^2\), \(F=20n^2\), default 2v → 80 triangular
//! faces) stamp RGB shells on top. Rings stay hidden.
//! Not a copy of qga-app scenes, not `QgaPixel`, not a ferrite plane.

use crate::args::Args;
use glam::{Quat, Vec3};
use qga_gpu::{hud_quad, hud_stroke, hud_text, GpuFiber, GpuParticle, HudVert, LineStyle, VisualState};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// One present per wave step. Rings stay hidden; only on-spheres draw.
pub const PULSE_PERIOD: u32 = 1;
pub const LIVE_FIBERS: u32 = 0;
pub const SENSE_N: u32 = 28;
pub const HALF_GAIN: f32 = 0.32;
pub const GEO_ALPHA: f32 = 0.6;
/// Packed so neighbouring orbs almost tile (diameter ≈ cell extent).
const PACK: f32 = 0.51;
/// Radial opacity. Innermost > 0.5, outermost < 0.5.
const ALPHA_INNER: f32 = 0.86;
const ALPHA_OUTER: f32 = 0.08;
/// Was 2 cells/frame; 30% slower → 1.4.
const PIXEL_SPEED: f32 = 1.4;
/// Rankine \(\Omega\) (rad / frame) at `speed = 1`.
const WHIRL_OMEGA: f32 = 0.055;
/// Lattice gravity (cells / frame²). Software fact of this Bernoulli map:
/// \(\eta(0)=-\Omega^2 a^2/g\). Sized so 64³ at PIXEL_SPEED is a readable hole
/// and SPEED_MAX clamps to the tank floor.
const WHIRL_G: f32 = 0.028;
const WHIRL_ETA_CLAMP: f32 = 0.72;
const SHOCK_DECAY: f32 = 26.0;
const SHOCK_SPEED: f32 = 1.15;
/// Face-boundary heatmap rings stay off. Bounce is unchanged.
const SHOCKWAVES: bool = false;

const GEO_PIXEL_RED: &str = "geo_pixel_red";
const GEO_PIXEL_GREEN: &str = "geo_pixel_green";
const GEO_PIXEL_BLUE: &str = "geo_pixel_blue";
pub const GEO_MIN: u32 = 1;
pub const GEO_MAX: u32 = 96;
pub const SPEED_MIN: f32 = 0.25;
pub const SPEED_MAX: f32 = 4.0;
/// Class I icosahedral frequency \(n=b\), \(c=0\). \(T=n^2\), \(F=20T\), \(V=10T+2\), \(E=30T\).
pub const GEO_FREQ: u32 = 2;
/// \(F=20n^2\) for the default Class I frequency.
pub const GEO_FACES: u32 = 20 * GEO_FREQ * GEO_FREQ;

const _: () = assert!(std::mem::size_of::<CoreCell>() == 32);
const _: () = assert!(PULSE_PERIOD >= 1 && PULSE_PERIOD <= 8);
const _: () = assert!(LIVE_FIBERS == 0);

/// Same shifts as graphics `QgaPixel`. Library occupies the first unused pair.
const FIELD_BIT: u32 = 0b1;
const SECTION_SHIFT: u32 = 1;
const SECTION_MASK: u32 = 0b11;
const LAYER_SHIFT: u32 = 3;
const LAYER_MASK: u32 = 0xff;
const LIBRARY_SHIFT: u32 = 11;
const LIBRARY_MASK: u32 = 0b11;

/// Two bits: {Néel-I, Bloch, anti, bimeron}. Model of the photonic library.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Library {
    Neel = 0,
    Bloch = 1,
    Anti = 2,
    Bimeron = 3,
}

#[allow(dead_code)]
impl Library {
    pub fn from_bits(bits: u32) -> Self {
        match bits & LIBRARY_MASK {
            0 => Self::Neel,
            1 => Self::Bloch,
            2 => Self::Anti,
            _ => Self::Bimeron,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Neel => "NEEL",
            Self::Bloch => "BLOCH",
            Self::Anti => "ANTI",
            Self::Bimeron => "BIMERON",
        }
    }

}

/// Write-cycle drive. Coincidence is X∧Y with inhibit quiet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    HalfX,
    HalfY,
    Coincident,
    Inhibit,
}

#[allow(dead_code)]
impl Drive {
    pub fn from_pulse(pulse: u32) -> Self {
        match pulse % 4 {
            0 => Self::HalfX,
            1 => Self::HalfY,
            2 => Self::Coincident,
            _ => Self::Inhibit,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HalfX => "HALF-X",
            Self::HalfY => "HALF-Y",
            Self::Coincident => "COINCIDENT",
            Self::Inhibit => "INHIBIT",
        }
    }

    pub fn x_hot(self) -> bool {
        matches!(self, Self::HalfX | Self::Coincident | Self::Inhibit)
    }

    pub fn y_hot(self) -> bool {
        matches!(self, Self::HalfY | Self::Coincident | Self::Inhibit)
    }

    pub fn inhibit(self) -> bool {
        matches!(self, Self::Inhibit)
    }

    /// Texture covering. Software fact of this Model: half-select must not
    /// reach |N_sk|≈1. Coincidence with inhibit off does.
    pub fn gain(self) -> f32 {
        match self {
            Self::Coincident => 1.0,
            Self::HalfX | Self::HalfY => HALF_GAIN,
            Self::Inhibit => 0.0,
        }
    }
}

/// 32-byte cell. CPU store; GpuParticle is the lossy witness.
/// packed: field:1 | section:2 | layer:8 | library:2
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CoreCell {
    pub theta: f32,
    pub phi: f32,
    pub psi: f32,
    pub persist: f32,
    pub n: [f32; 3],
    packed: u32,
}

#[allow(dead_code)]
impl CoreCell {
    fn blank(layer: u8) -> Self {
        let mut c = Self {
            theta: 0.0,
            phi: 0.0,
            psi: 0.0,
            persist: 0.0,
            n: [0.0, 1.0, 0.0],
            packed: 0,
        };
        c.set_layer(layer);
        c
    }

    pub fn field_bit(self) -> u32 {
        self.packed & FIELD_BIT
    }

    pub fn set_field(&mut self, bit: u32) {
        self.packed = (self.packed & !FIELD_BIT) | (bit & FIELD_BIT);
    }

    pub fn section_bits(self) -> u32 {
        (self.packed >> SECTION_SHIFT) & SECTION_MASK
    }

    pub fn layer(self) -> u8 {
        ((self.packed >> LAYER_SHIFT) & LAYER_MASK) as u8
    }

    pub fn set_layer(&mut self, layer: u8) {
        let bits = (layer as u32) & LAYER_MASK;
        self.packed = (self.packed & !(LAYER_MASK << LAYER_SHIFT)) | (bits << LAYER_SHIFT);
    }

    pub fn library(self) -> Library {
        Library::from_bits(self.packed >> LIBRARY_SHIFT)
    }

    pub fn set_library(&mut self, lib: Library) {
        let bits = (lib as u32) & LIBRARY_MASK;
        self.packed = (self.packed & !(LIBRARY_MASK << LIBRARY_SHIFT)) | (bits << LIBRARY_SHIFT);
    }

    pub fn packed(self) -> u32 {
        self.packed
    }

    /// HL projection of n: hue = azimuth ψ, lightness = S3. Not a palette.
    pub fn rgb_preview(self) -> Vec3 {
        rgb_preview(Vec3::from(self.n), self.persist)
    }
}

pub struct Sense {
    pub x_sel: u32,
    pub y_sel: u32,
    pub z_sel: u32,
    pub z_inhibit: bool,
    pub library: Library,
    pub drive: Drive,
    pub n_sk: f32,
    pub n: Vec3,
    pub r_wave: f32,
    pub n_on: u32,
}

pub struct CoreVolume {
    #[allow(dead_code)]
    pub static_fibers: Vec<GpuFiber>,
    pub live: Vec<GpuFiber>,
    #[allow(dead_code)]
    pub particles: Vec<GpuParticle>,
    pub orb_scale: f32,
    cells: Vec<CoreCell>,
    pos: Vec<Vec3>,
    grid: u32,
    planes: u32,
    ring_radius: f32,
    extent: f32,
    pitch: f32,
    pulse: u32,
    r_wave: f32,
    #[allow(dead_code)]
    k_wave: u32,
    geo_verts: Vec<Vec3>,
    geo_edges: Vec<[u32; 2]>,
    geo_tris: Vec<[u32; 3]>,
    glow: f32,
    speed: f32,
    pixels: Vec<GeoPixel>,
    shocks: Vec<Shock>,
    sense: Sense,
}

fn spawn_pixel(i: u32, grid: u32, planes: u32, speed: f32) -> GeoPixel {
    let (name, color) = match i % 3 {
        0 => (GEO_PIXEL_RED, Vec3::new(1.0, 0.08, 0.05)),
        1 => (GEO_PIXEL_GREEN, Vec3::new(0.06, 1.0, 0.12)),
        _ => (GEO_PIXEL_BLUE, Vec3::new(0.10, 0.28, 1.0)),
    };
    let g = grid.max(1) as f32;
    let p = planes.max(1) as f32;
    let r = geodesic_radius_cells(grid, planes);
    let inner = |t: f32, max: f32| {
        let span = (max - 2.0 * r).max(0.0);
        (r + t.clamp(0.0, 1.0) * span).clamp(r, (max - r).max(r))
    };
    let (x, y, z) = if i < 3 {
        // RGB triad starts in separate octants so the 2v shells do not coincide.
        let seeds = [[0.18, 0.22, 0.28], [0.78, 0.70, 0.24], [0.28, 0.76, 0.78]];
        let s = seeds[i as usize];
        (
            inner(s[0], (g - 1.0).max(0.0)),
            inner(s[1], (g - 1.0).max(0.0)),
            inner(s[2], (p - 1.0).max(0.0)),
        )
    } else {
        let gx = grid.max(1);
        let pz = planes.max(1);
        let sx = ((i.wrapping_mul(17) + 3) % gx) as f32 / g.max(1.0);
        let sy = ((i.wrapping_mul(29) + 5) % gx) as f32 / g.max(1.0);
        let sz = ((i.wrapping_mul(13) + 7) % pz) as f32 / p.max(1.0);
        (
            inner(sx, (g - 1.0).max(0.0)),
            inner(sy, (g - 1.0).max(0.0)),
            inner(sz, (p - 1.0).max(0.0)),
        )
    };
    let a = i as f32 * 1.618;
    let vx = (a.sin() + 0.15).signum() * speed * (0.65 + 0.35 * (a * 0.7).cos().abs());
    let vy = ((a * 1.3).cos() - 0.10).signum() * speed * (0.65 + 0.35 * a.sin().abs());
    let vz = ((a * 0.9).sin()).signum().max(0.15) * speed * (0.55 + 0.45 * (a * 1.1).cos().abs());
    GeoPixel {
        name,
        color,
        x,
        y,
        z,
        vx,
        vy,
        vz,
        angle: i as f32 * 0.37,
        spin_axis: Vec3::new(a.cos(), (a * 1.3).sin(), (a * 0.6).cos()).normalize_or_zero(),
        spin_rate: 0.045 + 0.02 * (i as f32 * 0.5).sin(),
    }
}

#[derive(Clone, Copy)]
struct GeoPixel {
    name: &'static str,
    color: Vec3,
    x: f32,
    y: f32,
    z: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    angle: f32,
    spin_axis: Vec3,
    spin_rate: f32,
}

#[derive(Clone, Copy)]
struct Shock {
    face: Face,
    u: u32,
    v: u32,
    age: u32,
    color: Vec3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SliderKind {
    Count,
    Speed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    NegX,
    PosX,
    NegY,
    PosY,
    NegZ,
    PosZ,
}

impl CoreVolume {
    pub fn new(args: &Args) -> Self {
        let grid = args.grid.max(1);
        let planes = args.planes.max(1);
        let extent = args.cell_extent;
        // Cubic cells. Null space is orb diameter vs extent, not a squat Z pitch.
        let pitch = extent;
        let ring_radius = args.ring_radius;
        let n_cells = (grid * grid * planes) as usize;
        let k_wave = planes / 2;

        let mut cells = Vec::with_capacity(n_cells);
        let mut pos = Vec::with_capacity(n_cells);
        for k in 0..planes {
            for j in 0..grid {
                for i in 0..grid {
                    let p = cell_pos(i, j, k, grid, planes, extent, pitch);
                    pos.push(p);
                    cells.push(CoreCell::blank(k as u8));
                }
            }
        }

        let (geo_verts, geo_edges, geo_tris) = class_i_icosahedral(GEO_FREQ);
        debug_assert_eq!(geo_tris.len() as u32, GEO_FACES);
        let mut vol = Self {
            static_fibers: Vec::new(),
            live: Vec::new(),
            particles: Vec::new(),
            orb_scale: (extent * PACK).max(0.02),
            cells,
            pos,
            grid,
            planes,
            ring_radius,
            extent,
            pitch,
            pulse: 0,
            r_wave: 0.0,
            k_wave,
            geo_verts,
            geo_edges,
            geo_tris,
            glow: 0.0,
            speed: PIXEL_SPEED,
            pixels: Vec::new(),
            shocks: Vec::new(),
            sense: Sense {
                x_sel: 0,
                y_sel: 0,
                z_sel: k_wave,
                z_inhibit: false,
                library: Library::Neel,
                drive: Drive::Coincident,
                n_sk: 0.0,
                n: Vec3::Y,
                r_wave: 0.0,
                n_on: 0,
            },
        };
        assert_eq!(vol.live.len(), LIVE_FIBERS as usize);
        vol.set_pixel_count(3);
        vol.apply_cycle();
        vol
    }

    pub fn pixel_count(&self) -> u32 {
        self.pixels.len() as u32
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn set_pixel_count(&mut self, n: u32) {
        let n = n.clamp(GEO_MIN, GEO_MAX) as usize;
        if n < self.pixels.len() {
            self.pixels.truncate(n);
            return;
        }
        while self.pixels.len() < n {
            let i = self.pixels.len() as u32;
            self.pixels
                .push(spawn_pixel(i, self.grid, self.planes, self.speed));
        }
    }

    pub fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(SPEED_MIN, SPEED_MAX);
        let old = self.speed.max(1e-4);
        let s = speed / old;
        for p in &mut self.pixels {
            p.vx *= s;
            p.vy *= s;
            p.vz *= s;
        }
        self.speed = speed;
    }

    pub fn nudge_count(&mut self, d: i32) {
        let n = (self.pixel_count() as i32 + d).clamp(GEO_MIN as i32, GEO_MAX as i32) as u32;
        self.set_pixel_count(n);
    }

    pub fn nudge_speed(&mut self, d: f32) {
        self.set_speed(self.speed + d);
    }

    pub fn is_pulse(frame: u32) -> bool {
        frame > 0 && frame % PULSE_PERIOD == 0
    }

    pub fn pulse_write(&mut self) {
        self.pulse = self.pulse.wrapping_add(1);
        self.apply_cycle();
    }

    pub fn sense(&self) -> &Sense {
        &self.sense
    }

    pub fn grid(&self) -> u32 {
        self.grid
    }

    pub fn planes(&self) -> u32 {
        self.planes
    }

    fn apply_cycle(&mut self) {
        for cell in &mut self.cells {
            cell.persist = 0.0;
        }

        let glow = 0.5 + 0.5 * (self.pulse as f32 * 0.085).sin();
        self.glow = glow;

        let xmax = self.grid.saturating_sub(1) as f32;
        let zmax = xmax;
        let ymax = self.planes.saturating_sub(1) as f32;
        let r_cells = geodesic_radius_cells(self.grid, self.planes);
        let xmin = r_cells.min(xmax * 0.45);
        let zmin = xmin;
        let ymin = r_cells.min(ymax * 0.45);
        let mut hits: Vec<(Face, u32, u32, Vec3)> = Vec::new();
        if self.pulse > 0 {
        for p in &mut self.pixels {
            let (nx, nvx, hit_x) = bounce_axis_margin(p.x, p.vx, xmin, xmax - xmin);
            let (ny, nvy, hit_z) = bounce_axis_margin(p.y, p.vy, zmin, zmax - zmin);
            let (nz, nvz, hit_y) = bounce_axis_margin(p.z, p.vz, ymin, ymax - ymin);
            p.x = nx;
            p.y = ny;
            p.z = nz;
            p.vx = nvx;
            p.vy = nvy;
            p.vz = nvz;
            p.angle += p.spin_rate;
            let iu = ny.round().clamp(0.0, zmax) as u32;
            let iv = nz.round().clamp(0.0, ymax) as u32;
            let ix = nx.round().clamp(0.0, xmax) as u32;
            if SHOCKWAVES {
                if hit_x {
                    let face = if nx <= 0.5 { Face::NegX } else { Face::PosX };
                    hits.push((face, iu, iv, p.color));
                }
                if hit_z {
                    let face = if ny <= 0.5 { Face::NegZ } else { Face::PosZ };
                    hits.push((face, ix, iv, p.color));
                }
                if hit_y {
                    let face = if nz <= 0.5 { Face::NegY } else { Face::PosY };
                    hits.push((face, ix, iu, p.color));
                }
            }
        }
        }
        if SHOCKWAVES {
            for (face, u, v, color) in hits {
                self.shocks.push(Shock {
                    face,
                    u,
                    v,
                    age: 0,
                    color,
                });
            }
            for s in &mut self.shocks {
                s.age = s.age.saturating_add(1);
            }
            self.shocks.retain(|s| (s.age as f32) < SHOCK_DECAY * 2.8);
            if self.shocks.len() > 12 {
                let n = self.shocks.len() - 12;
                self.shocks.drain(0..n);
            }
            self.paint_shocks();
        } else {
            self.shocks.clear();
        }

        let verts = self.geo_verts.clone();
        let edges = self.geo_edges.clone();
        let tris = self.geo_tris.clone();
        let r_world = r_cells * self.extent;
        let bodies = self.pixels.clone();
        for p in &bodies {
            let rot = Quat::from_axis_angle(p.spin_axis, p.angle);
            let c = self.lattice_world(p.x, p.y, p.z);
            self.stamp_geodesic(&verts, &edges, &tris, c, rot, r_world, p.color);
        }

        let n_stokes = stokes_n(0.0, 0.0, self.ring_radius.max(1e-4), Library::Neel, 1.0);
        let n_sk = discrete_n_sk(Library::Neel, 1.0, self.ring_radius.max(1e-4));
        let red = self.pixels.first().copied();
        self.r_wave = 0.0;
        self.sense = Sense {
            x_sel: red.map(|p| p.x.round() as u32).unwrap_or(0),
            y_sel: red.map(|p| p.y.round() as u32).unwrap_or(0),
            z_sel: red.map(|p| p.z.round() as u32).unwrap_or(0),
            z_inhibit: false,
            library: Library::Neel,
            drive: Drive::Coincident,
            n_sk,
            n: n_stokes,
            r_wave: 0.0,
            n_on: self.pixels.len() as u32,
        };
    }

    fn paint_shocks(&mut self) {
        let g = self.grid;
        let p = self.planes;
        let shocks = self.shocks.clone();
        for sh in &shocks {
            let decay = (-(sh.age as f32) / SHOCK_DECAY).exp();
            if decay < 0.03 {
                continue;
            }
            let r_front = sh.age as f32 * SHOCK_SPEED;
            let face_span = g.max(p) as f32;
            let expand_fade = (1.0 - (r_front / (face_span * 0.95)).clamp(0.0, 1.0)).powf(1.35);
            match sh.face {
                Face::NegX | Face::PosX => {
                    let i = if sh.face == Face::NegX { 0 } else { g - 1 };
                    for j in 0..g {
                        for k in 0..p {
                            let d = hypot2(j as f32 - sh.u as f32, k as f32 - sh.v as f32);
                            self.add_shock(i, j, k, shock_amp(d, r_front, decay) * expand_fade, sh.color);
                            let inward = if sh.face == Face::NegX { 1 } else { i.saturating_sub(1) };
                            if inward != i {
                                self.add_shock(
                                    inward,
                                    j,
                                    k,
                                    shock_amp(d, r_front, decay) * expand_fade * 0.45,
                                    sh.color,
                                );
                            }
                        }
                    }
                }
                Face::NegZ | Face::PosZ => {
                    let j = if sh.face == Face::NegZ { 0 } else { g - 1 };
                    for i in 0..g {
                        for k in 0..p {
                            let d = hypot2(i as f32 - sh.u as f32, k as f32 - sh.v as f32);
                            self.add_shock(i, j, k, shock_amp(d, r_front, decay) * expand_fade, sh.color);
                            let inward = if sh.face == Face::NegZ { 1 } else { j.saturating_sub(1) };
                            if inward != j {
                                self.add_shock(
                                    i,
                                    inward,
                                    k,
                                    shock_amp(d, r_front, decay) * expand_fade * 0.45,
                                    sh.color,
                                );
                            }
                        }
                    }
                }
                Face::NegY | Face::PosY => {
                    let k = if sh.face == Face::NegY { 0 } else { p - 1 };
                    for i in 0..g {
                        for j in 0..g {
                            let d = hypot2(i as f32 - sh.u as f32, j as f32 - sh.v as f32);
                            self.add_shock(i, j, k, shock_amp(d, r_front, decay) * expand_fade, sh.color);
                            let inward = if sh.face == Face::NegY { 1 } else { k.saturating_sub(1) };
                            if inward != k {
                                self.add_shock(
                                    i,
                                    j,
                                    inward,
                                    shock_amp(d, r_front, decay) * expand_fade * 0.45,
                                    sh.color,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn lattice_world(&self, x: f32, y: f32, z: f32) -> Vec3 {
        let ox = (self.grid.saturating_sub(1) as f32) * 0.5;
        let oy = (self.planes.saturating_sub(1) as f32) * 0.5;
        Vec3::new(
            (x - ox) * self.extent,
            (z - oy) * self.pitch,
            (y - ox) * self.extent,
        )
    }

    fn stamp(&mut self, p: Vec3, persist: f32, color: Vec3) {
        let ox = (self.grid.saturating_sub(1) as f32) * 0.5;
        let oy = (self.planes.saturating_sub(1) as f32) * 0.5;
        let i = (p.x / self.extent + ox).round() as i32;
        let j = (p.z / self.extent + ox).round() as i32;
        let k = (p.y / self.pitch + oy).round() as i32;
        if i < 0 || j < 0 || k < 0 {
            return;
        }
        let (i, j, k) = (i as u32, j as u32, k as u32);
        if i >= self.grid || j >= self.grid || k >= self.planes {
            return;
        }
        let idx = index(i, j, k, self.grid);
        if idx >= self.cells.len() {
            return;
        }
        let mut cell = self.cells[idx];
        if persist >= cell.persist {
            cell.persist = persist;
            cell.n = color.into();
            cell.set_layer(k as u8);
            self.cells[idx] = cell;
        }
    }

    /// Rasterize a Class I geodesic polyhedron: vertices, chord struts, and
    /// filled planar triangular faces (the convex hull, not the spherical cap).
    fn stamp_geodesic(
        &mut self,
        verts: &[Vec3],
        edges: &[[u32; 2]],
        tris: &[[u32; 3]],
        center: Vec3,
        rot: Quat,
        r_world: f32,
        color: Vec3,
    ) {
        let xform = |v: Vec3| center + rot * (v * r_world);
        for v in verts {
            self.stamp(xform(*v), 2.0, color);
        }
        let e = self.extent.max(1e-4);
        for &[ia, ib] in edges {
            let pa = xform(verts[ia as usize]);
            let pb = xform(verts[ib as usize]);
            let steps = ((pa - pb).length() / e * 1.35).ceil().clamp(1.0, 16.0) as u32;
            for s in 0..=steps {
                let t = s as f32 / steps as f32;
                self.stamp(pa.lerp(pb, t), 1.9, color);
            }
        }
        let panel = color * 0.38;
        for &[ia, ib, ic] in tris {
            self.stamp_tri(
                xform(verts[ia as usize]),
                xform(verts[ib as usize]),
                xform(verts[ic as usize]),
                1.7,
                panel,
            );
        }
    }

    fn stamp_tri(&mut self, a: Vec3, b: Vec3, c: Vec3, persist: f32, color: Vec3) {
        let e = self.extent.max(1e-4);
        let longest = (b - a).length().max((c - b).length()).max((a - c).length());
        let n = ((longest / e) * 1.55).ceil().clamp(2.0, 20.0) as usize;
        let nf = n as f32;
        for i in 0..=n {
            for j in 0..=(n - i) {
                let u = i as f32 / nf;
                let v = j as f32 / nf;
                let w = 1.0 - u - v;
                self.stamp(a * w + b * u + c * v, persist, color);
            }
        }
    }

    fn add_shock(&mut self, i: u32, j: u32, k: u32, amp: f32, color: Vec3) {
        if amp < 0.02 {
            return;
        }
        let idx = index(i, j, k, self.grid);
        if idx >= self.cells.len() {
            return;
        }
        let mut cell = self.cells[idx];
        if cell.persist >= 1.5 {
            return;
        }
        let old = Vec3::from(cell.n);
        let acc = old + color * amp;
        cell.n = acc.into();
        cell.persist = acc.max_element().min(1.2);
        cell.set_layer(k as u8);
        self.cells[idx] = cell;
    }

    pub fn orb_instances(&self) -> Vec<(Vec3, Vec3, f32, f32)> {
        let g = 0.70 + 0.45 * self.glow;
        let n = self.cells.len();
        let mut out = Vec::with_capacity(n);
        let grid = self.grid;
        for idx in 0..n {
            let k = idx as u32 / (grid * grid);
            let rem = idx as u32 % (grid * grid);
            let j = rem / grid;
            let i = rem % grid;
            let (offset, ocean_rgb, alpha) = ocean_field(
                i,
                j,
                k,
                self.grid,
                self.planes,
                self.pulse,
                self.speed,
                self.extent,
            );
            let base = self.pos[idx];
            let cell = self.cells[idx];
            if cell.persist >= 1.5 {
                out.push((
                    base + offset,
                    Vec3::from(cell.n) * g,
                    self.orb_scale,
                    alpha.max(GEO_ALPHA),
                ));
            } else {
                out.push((base + offset, ocean_rgb, self.orb_scale, alpha));
            }
        }
        out
    }

    /// Bloom on the geodesic polyhedron. `glow_effect=true`.
    pub fn breathe(&self, vis: &mut VisualState) {
        vis.glow = 0.0;
        vis.pulse = 0.28 + 0.22 * self.glow;
        vis.aperture = 1.0;
        vis.zener = 2.4;
    }

    /// Axis-aligned shell around the lattice. `#00FF00`.
    pub fn frame_edges(&self) -> Vec<[Vec3; 2]> {
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for &p in &self.pos {
            mn = mn.min(p);
            mx = mx.max(p);
        }
        // Half a cell, not orb radius: outline tracks resolution, not density.
        let pad = self.extent * 0.5;
        mn -= Vec3::splat(pad);
        mx += Vec3::splat(pad);
        box_edges(mn, mx)
    }

    pub fn frame_style() -> LineStyle {
        LineStyle {
            color: Vec3::new(0.0, 1.0, 0.0),
            width: 0.004,
            depth_bias: 0.0002,
            opacity: 1.0,
        }
    }

    /// Bottom-left HUD sliders. NDC. Count then velocity.
    pub fn slider_count_rect() -> [f32; 4] {
        [-0.92, -0.78, -0.18, -0.70]
    }

    pub fn slider_speed_rect() -> [f32; 4] {
        [-0.92, -0.92, -0.18, -0.84]
    }

    pub fn hit_slider(ndc: [f32; 2]) -> Option<SliderKind> {
        let [x, y] = ndc;
        let c = Self::slider_count_rect();
        let s = Self::slider_speed_rect();
        if x >= c[0] && x <= c[2] && y >= c[1] && y <= c[3] {
            Some(SliderKind::Count)
        } else if x >= s[0] && x <= s[2] && y >= s[1] && y <= s[3] {
            Some(SliderKind::Speed)
        } else {
            None
        }
    }

    pub fn apply_slider(&mut self, kind: SliderKind, ndc_x: f32) {
        let r = match kind {
            SliderKind::Count => Self::slider_count_rect(),
            SliderKind::Speed => Self::slider_speed_rect(),
        };
        let t = ((ndc_x - r[0]) / (r[2] - r[0])).clamp(0.0, 1.0);
        match kind {
            SliderKind::Count => {
                let n = GEO_MIN + ((GEO_MAX - GEO_MIN) as f32 * t).round() as u32;
                self.set_pixel_count(n);
            }
            SliderKind::Speed => {
                self.set_speed(SPEED_MIN + (SPEED_MAX - SPEED_MIN) * t);
            }
        }
    }

    pub fn hud(&self) -> Vec<HudVert> {
        let mut hud = Vec::new();
        let n = self.pixel_count();
        let v = self.speed;
        slider_bar(
            &mut hud,
            Self::slider_count_rect(),
            (n - GEO_MIN) as f32 / (GEO_MAX - GEO_MIN) as f32,
            [0.95, 0.35, 0.28, 0.95],
        );
        slider_bar(
            &mut hud,
            Self::slider_speed_rect(),
            (v - SPEED_MIN) / (SPEED_MAX - SPEED_MIN),
            [0.30, 0.75, 1.0, 0.95],
        );
        hud_text(
            &mut hud,
            -0.92,
            -0.66,
            0.014,
            &format!("N GEOS {n}  {GEO_FREQ}v F={GEO_FACES}   VEL {v:.2}"),
            [0.85, 0.90, 1.0, 0.90],
        );
        hud
    }

    pub fn print_sense(&self) {
        let s = self.sense();
        let idx = (s.x_sel + s.y_sel * self.grid() + s.z_sel * self.grid() * self.grid()) as usize;
        let cell = self.cells.get(idx).copied().unwrap_or_else(|| CoreCell::blank(0));
        let n_shell = self.cells.iter().filter(|c| c.persist >= 1.5).count();
        println!(
            "sense n={} speed={:.2} {}v F={} shell={} {}x{}x{} N_sk={:+.3} red=({},{},{}) (Model)",
            self.pixels.len(),
            self.speed,
            GEO_FREQ,
            GEO_FACES,
            n_shell,
            self.grid(),
            self.grid(),
            self.planes(),
            s.n_sk,
            s.x_sel,
            s.y_sel,
            s.z_sel,
        );
        let _ = (s.z_inhibit, s.n, s.drive.as_str(), cell.rgb_preview(), cell.packed());
    }
}

pub fn camera_distance(args: &Args) -> f32 {
    let span = (args.grid.saturating_sub(1) as f32) * args.cell_extent;
    let stack = (args.planes.saturating_sub(1) as f32) * args.cell_extent;
    (span * 0.95 + stack * 0.85).max(3.2)
}

fn slider_bar(hud: &mut Vec<HudVert>, r: [f32; 4], t: f32, fill: [f32; 4]) {
    let [x0, y0, x1, y1] = r;
    hud_quad(hud, x0, y0, x1, y1, [0.08, 0.10, 0.14, 0.70]);
    hud_stroke(hud, x0, y0, x1, y0, 0.004, [0.55, 0.60, 0.70, 0.80]);
    hud_stroke(hud, x0, y1, x1, y1, 0.004, [0.55, 0.60, 0.70, 0.80]);
    let t = t.clamp(0.0, 1.0);
    let xf = x0 + (x1 - x0) * t;
    if xf > x0 + 0.004 {
        hud_quad(hud, x0 + 0.006, y0 + 0.01, xf, y1 - 0.01, fill);
    }
}

fn box_edges(mn: Vec3, mx: Vec3) -> Vec<[Vec3; 2]> {
    let c = [
        Vec3::new(mn.x, mn.y, mn.z),
        Vec3::new(mx.x, mn.y, mn.z),
        Vec3::new(mx.x, mn.y, mx.z),
        Vec3::new(mn.x, mn.y, mx.z),
        Vec3::new(mn.x, mx.y, mn.z),
        Vec3::new(mx.x, mx.y, mn.z),
        Vec3::new(mx.x, mx.y, mx.z),
        Vec3::new(mn.x, mx.y, mx.z),
    ];
    vec![
        [c[0], c[1]],
        [c[1], c[2]],
        [c[2], c[3]],
        [c[3], c[0]],
        [c[4], c[5]],
        [c[5], c[6]],
        [c[6], c[7]],
        [c[7], c[4]],
        [c[0], c[4]],
        [c[1], c[5]],
        [c[2], c[6]],
        [c[3], c[7]],
    ]
}

/// Visible EM spectrum → RGB. 400–700 nm. No magenta wrap (that is not EM).
/// Ends are dimmed so UV/IR stay out. Software fact of this projection.
fn wavelength_rgb(nm: f32) -> Vec3 {
    let nm = nm.clamp(400.0, 700.0);
    let rgb = if nm < 440.0 {
        let t = (nm - 400.0) / 40.0;
        Vec3::new(0.30 * (1.0 - t), 0.0, 1.0)
    } else if nm < 490.0 {
        let t = (nm - 440.0) / 50.0;
        Vec3::new(0.0, t, 1.0)
    } else if nm < 510.0 {
        let t = (nm - 490.0) / 20.0;
        Vec3::new(0.0, 1.0, 1.0 - t)
    } else if nm < 580.0 {
        let t = (nm - 510.0) / 70.0;
        Vec3::new(t, 1.0, 0.0)
    } else if nm < 645.0 {
        let t = (nm - 580.0) / 65.0;
        Vec3::new(1.0, 1.0 - t, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    let fade = if nm < 420.0 {
        0.35 + 0.65 * (nm - 400.0) / 20.0
    } else if nm > 680.0 {
        0.35 + 0.65 * (700.0 - nm) / 20.0
    } else {
        1.0
    };
    rgb * fade
}

/// Packed ocean + Rankine whirlpool. Core rotates as a solid body; outside
/// \(v_\theta \propto 1/r\). Height is the Bernoulli Rankine surface from the
/// same \(\Omega\) as the swirl. Swell is gated off inside \(\sim 2.6 a\).
/// Radial alpha: inner \(> 0.5\), outer \(< 0.5\); throat is darker.
fn ocean_field(
    i: u32,
    j: u32,
    k: u32,
    grid: u32,
    planes: u32,
    pulse: u32,
    speed: f32,
    extent: f32,
) -> (Vec3, Vec3, f32) {
    let ox = (grid.saturating_sub(1) as f32) * 0.5;
    let oy = (planes.saturating_sub(1) as f32) * 0.5;
    let dx = i as f32 - ox;
    let dz = j as f32 - ox;
    let dy = k as f32 - oy;
    let rho = (dx * dx + dz * dz).sqrt();
    let rho_s = rho.max(0.28);
    let theta = dz.atan2(dx);
    let r_core = rankine_core_radius(ox);
    let t = pulse as f32;
    let omega = rankine_omega(speed);
    let omega_r = if rho < r_core {
        omega
    } else {
        omega * (r_core / rho_s).powi(2)
    };
    let dtheta = omega_r * t;

    let y_top = (k as f32 / planes.max(1) as f32).clamp(0.0, 1.0);
    let surface = smoother(0.16, 0.92, y_top);
    let swell_w = smoother(r_core * 1.15, r_core * 2.6, rho);
    let q = Vec3::new(i as f32, k as f32, j as f32);
    let kdir = Vec3::new(1.0, 0.07, 0.36).normalize();
    let lambda = (grid.max(planes) as f32 * 0.30).max(6.0);
    let kappa = TAU / lambda;
    let w_omega = 0.09 * speed.max(0.15);
    let wave = kappa * kdir.dot(q) - w_omega * t;
    let k2 = Vec3::new(0.22, 0.0, 1.0).normalize();
    let swell = (kappa * 1.65 * k2.dot(q) - w_omega * 0.61 * t).sin();
    let h_wave = (wave.sin() * 0.22 + swell * 0.10) * swell_w;

    let eta = rankine_eta_cells(rho, r_core, omega).max(-WHIRL_ETA_CLAMP * oy.max(2.0));
    let circ = 2.0 * y_top - 1.0;
    let radial = -0.11 * circ * (r_core / (rho_s + 0.35 * r_core));
    let rho2 = (rho * (1.0 + radial)).max(0.0);
    let theta2 = theta + dtheta;
    let offset = Vec3::new(
        (rho2 * theta2.cos() - dx) * extent,
        (eta * surface + h_wave * 1.15) * extent,
        (rho2 * theta2.sin() - dz) * extent,
    );

    let arm = theta + 1.35 * (rho_s / r_core.max(1.0)).ln() - 1.15 * omega * t;
    let arm_s = (0.5 + 0.5 * arm.sin()).clamp(0.0, 1.0);
    let eta_n = (eta / oy.max(2.0)).clamp(-1.0, 0.0);
    let lift = 0.28 + 0.50 * (1.0 + eta_n * surface).clamp(0.10, 1.0);
    let grey = Vec3::splat(0.02 + 0.10 * lift);
    let foam = h_wave.max(0.0).powf(2.8);
    let peak = ((arm_s - 0.52).max(0.0) / 0.48).powf(1.55);
    let intensity = (peak + foam).clamp(0.0, 1.0);
    let mut rgb = grey.lerp(Vec3::ONE, intensity);
    let throat = (r_core / (rho_s + 0.2 * r_core)).powf(1.8) * surface;
    rgb = rgb.lerp(Vec3::splat(0.015), throat * 0.92);

    let rmax = (ox * ox + oy * oy + ox * ox).sqrt().max(1e-4);
    let r = (Vec3::new(dx, dy, dz).length() / rmax).clamp(0.0, 1.0);
    let mut alpha = ALPHA_INNER + (ALPHA_OUTER - ALPHA_INNER) * r.powf(1.65);
    alpha *= 1.0 - 0.72 * throat;
    (offset, rgb, alpha.clamp(0.03, 0.95))
}

fn rankine_omega(speed: f32) -> f32 {
    WHIRL_OMEGA * speed.max(0.15)
}

fn rankine_core_radius(ox: f32) -> f32 {
    ox.max(3.0) * 0.24
}

/// Rankine–Bernoulli free surface in lattice cells. \(\eta\to 0\) as \(r\to\infty\):
/// \(\eta = -\frac{\Omega^2}{2g}(2a^2-r^2)\) inside, \(-\frac{\Omega^2 a^4}{2g r^2}\)
/// outside. Same \(\Omega\) as the swirl.
fn rankine_eta_cells(rho: f32, a: f32, omega: f32) -> f32 {
    let a = a.max(1e-4);
    let a2 = a * a;
    let o2 = omega * omega;
    let g = WHIRL_G.max(1e-6);
    if rho <= a {
        -(o2 / (2.0 * g)) * (2.0 * a2 - rho * rho)
    } else {
        -(o2 * a2 * a2) / (2.0 * g * rho.max(1e-4) * rho.max(1e-4))
    }
}

fn smoother(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Class I icosahedral geodesic polyhedron. Frequency \(n=b\), \(c=0\):
/// \(T=n^2\), \(F=20T\), \(V=10T+2\), \(E=30T\). Vertices projected to \(S^2\);
/// faces are the planar triangles of the convex polyhedron.
fn class_i_icosahedral(freq: u32) -> (Vec<Vec3>, Vec<[u32; 2]>, Vec<[u32; 3]>) {
    let n = freq.max(1);
    let t = (1.0 + 5.0_f32.sqrt()) * 0.5;
    let raw = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let base: Vec<Vec3> = raw
        .iter()
        .map(|p| Vec3::from_array(*p).normalize())
        .collect();
    let faces: [[u32; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let mut verts = Vec::new();
    let mut id_of: HashMap<(i32, i32, i32), u32> = HashMap::new();
    let mut intern = |p: Vec3| -> u32 {
        let p = p.normalize();
        let k = (
            (p.x * 1e5).round() as i32,
            (p.y * 1e5).round() as i32,
            (p.z * 1e5).round() as i32,
        );
        if let Some(&i) = id_of.get(&k) {
            return i;
        }
        let i = verts.len() as u32;
        verts.push(p);
        id_of.insert(k, i);
        i
    };
    let mut tris = Vec::new();
    let nf = n as f32;
    for [ia, ib, ic] in faces {
        let a = base[ia as usize];
        let b = base[ib as usize];
        let c = base[ic as usize];
        let mut row: Vec<Vec<u32>> = Vec::with_capacity(n as usize + 1);
        for i in 0..=n {
            let mut col = Vec::with_capacity((n - i + 1) as usize);
            for j in 0..=n - i {
                let u = i as f32 / nf;
                let v = j as f32 / nf;
                let w = 1.0 - u - v;
                col.push(intern(a * w + b * u + c * v));
            }
            row.push(col);
        }
        for i in 0..n as usize {
            for j in 0..(n as usize - i) {
                let v00 = row[i][j];
                let v10 = row[i + 1][j];
                let v01 = row[i][j + 1];
                tris.push([v00, v10, v01]);
                if j + 1 < n as usize - i {
                    let v11 = row[i + 1][j + 1];
                    tris.push([v10, v11, v01]);
                }
            }
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashMap::new();
    for [a, b, c] in &tris {
        for e in [[*a, *b], [*b, *c], [*c, *a]] {
            let key = (e[0].min(e[1]), e[0].max(e[1]));
            if seen.insert(key, ()).is_none() {
                edges.push([key.0, key.1]);
            }
        }
    }
    (verts, edges, tris)
}

fn bounce_axis(p: f32, v: f32, max: f32) -> (f32, f32, bool) {
    bounce_axis_margin(p, v, 0.0, max)
}

fn bounce_axis_margin(p: f32, v: f32, min: f32, max: f32) -> (f32, f32, bool) {
    let max = max.max(min);
    let n = p + v;
    if n < min {
        (min, v.abs(), true)
    } else if n > max {
        (max, -v.abs(), true)
    } else {
        (n, v, false)
    }
}

fn geodesic_radius_cells(grid: u32, planes: u32) -> f32 {
    let m = grid.min(planes) as f32;
    // ~18% of the short axis so 2v triangles span several cells on 40³.
    (m * 0.18).clamp(1.5, 8.0)
}

fn hypot2(a: f32, b: f32) -> f32 {
    (a * a + b * b).sqrt()
}

fn shock_amp(d: f32, r_front: f32, decay: f32) -> f32 {
    let w = 1.65;
    let ring = (-((d - r_front) / w).powi(2)).exp();
    let fill = (1.0 - (d / (r_front + 1.2)).clamp(0.0, 1.0)).powf(1.6) * 0.18;
    // Opacity falls off the crest and with distance from the impact.
    let skirt = (1.0 - (d / (r_front + 3.0)).clamp(0.0, 1.0)).powf(1.25);
    (ring + fill) * decay * skirt
}

fn index(i: u32, j: u32, k: u32, grid: u32) -> usize {
    (i + j * grid + k * grid * grid) as usize
}

fn cell_pos(i: u32, j: u32, k: u32, grid: u32, planes: u32, extent: f32, pitch: f32) -> Vec3 {
    let ox = (grid.saturating_sub(1) as f32) * 0.5;
    let oy = (planes.saturating_sub(1) as f32) * 0.5;
    Vec3::new(
        (i as f32 - ox) * extent,
        (k as f32 - oy) * pitch,
        (j as f32 - ox) * extent,
    )
}

/// Baby-skyrmion Stokes field. Model of Zhang et al. eq. (1):
/// LG00 + e^{-iΔφ} LG_{0,±1}. Δφ selects Néel/Bloch; reversed ℓ is anti;
/// H/V (bimeron) is a global Stokes rotation.
pub fn stokes_n(x: f32, z: f32, r0: f32, lib: Library, gain: f32) -> Vec3 {
    let rho = (x * x + z * z).sqrt();
    let phi = z.atan2(x);
    let r0 = r0.max(1e-4);
    let (vorticity, helicity, bimeron) = match lib {
        Library::Neel => (1.0, 0.0, false),
        Library::Bloch => (1.0, FRAC_PI_2, false),
        Library::Anti => (-1.0, 0.0, false),
        Library::Bimeron => (1.0, 0.0, true),
    };
    let theta = PI * (-(rho / r0).powi(2)).exp() * gain.clamp(0.0, 1.0);
    let psi = vorticity * phi + helicity;
    let (st, ct) = theta.sin_cos();
    let (sp, cp) = psi.sin_cos();
    let mut n = Vec3::new(st * cp, ct, st * sp);
    if bimeron {
        n = Vec3::new(n.y, n.x, n.z);
    }
    let n = n.normalize_or_zero();
    if n.length_squared() < 0.5 {
        Vec3::Y
    } else {
        n
    }
}

/// Discrete N_sk on a local patch. Software fact of this sampler, not a theorem.
pub fn discrete_n_sk(lib: Library, gain: f32, r0: f32) -> f32 {
    let n_side = SENSE_N as usize;
    let extent = 3.2 * r0.max(1e-4);
    let dx = (2.0 * extent) / n_side as f32;
    let mut acc = 0.0;
    for j in 0..n_side.saturating_sub(1) {
        for i in 0..n_side.saturating_sub(1) {
            let x = -extent + (i as f32 + 0.5) * dx;
            let z = -extent + (j as f32 + 0.5) * dx;
            let n00 = stokes_n(x, z, r0, lib, gain);
            let n10 = stokes_n(x + dx, z, r0, lib, gain);
            let n01 = stokes_n(x, z + dx, r0, lib, gain);
            let nx = (n10 - n00) / dx;
            let nz = (n01 - n00) / dx;
            acc += n00.dot(nx.cross(nz)) * dx * dx;
        }
    }
    acc / (4.0 * PI)
}

pub fn rgb_preview(n: Vec3, persist: f32) -> Vec3 {
    let hue = (n.z.atan2(n.x) / TAU).rem_euclid(1.0);
    let light = 0.22 + 0.70 * (0.5 + 0.5 * n.y);
    hsv(hue, 0.80, light.clamp(0.18, 1.0)) * persist.clamp(0.12, 1.0)
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
        4 => Vec3::new(t, p, v),
        _ => Vec3::new(v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::parse_from;

    fn core_args() -> Args {
        parse_from([
            "--headless",
            "--scene",
            "core",
            "--preset",
            "smoke",
            "--frames",
            "32",
            "--no-capture",
        ])
        .unwrap()
    }

    #[test]
    fn cell_is_32_bytes() {
        assert_eq!(std::mem::size_of::<CoreCell>(), 32);
        assert_eq!(std::mem::size_of::<CoreCell>() % 8, 0);
    }

    #[test]
    fn library_bits_do_not_clobber_field_section_layer() {
        let mut c = CoreCell::blank(7);
        c.set_field(1);
        c.set_library(Library::Bimeron);
        assert_eq!(c.field_bit(), 1);
        assert_eq!(c.layer(), 7);
        assert_eq!(c.section_bits(), 0);
        assert_eq!(c.library(), Library::Bimeron);
        c.set_library(Library::Anti);
        assert_eq!(c.field_bit(), 1);
        assert_eq!(c.layer(), 7);
        assert_eq!(c.library(), Library::Anti);
        c.set_layer(3);
        assert_eq!(c.library(), Library::Anti);
        assert_eq!(c.field_bit(), 1);
    }

    #[test]
    fn coincident_n_sk_is_near_unity() {
        let r0 = 0.09;
        let neel = discrete_n_sk(Library::Neel, 1.0, r0);
        let bloch = discrete_n_sk(Library::Bloch, 1.0, r0);
        let anti = discrete_n_sk(Library::Anti, 1.0, r0);
        let bim = discrete_n_sk(Library::Bimeron, 1.0, r0);
        assert!(neel > 0.70, "Néel N_sk={neel}");
        assert!(bloch > 0.70, "Bloch N_sk={bloch}");
        assert!(anti < -0.70, "anti N_sk={anti}");
        assert!(bim.abs() > 0.70, "bimeron N_sk={bim}");
    }

    #[test]
    fn half_select_does_not_reach_unity() {
        let r0 = 0.09;
        for lib in [Library::Neel, Library::Bloch, Library::Anti, Library::Bimeron] {
            let n = discrete_n_sk(lib, HALF_GAIN, r0);
            assert!(
                n.abs() < 0.45,
                "half-select |N_sk|={n} for {:?}",
                lib
            );
        }
    }

    #[test]
    fn inhibit_blanks_topology() {
        let r0 = 0.09;
        for lib in [Library::Neel, Library::Bloch, Library::Anti, Library::Bimeron] {
            let n = discrete_n_sk(lib, 0.0, r0);
            assert!(n.abs() < 0.12, "inhibit |N_sk|={n} for {:?}", lib);
        }
    }

    #[test]
    fn rgb_preview_is_stokes_projection() {
        let n = stokes_n(0.09, 0.0, 0.09, Library::Neel, 1.0);
        let rgb = rgb_preview(n, 1.0);
        assert!(rgb.min_element() >= 0.0 && rgb.max_element() <= 1.0);
        let n2 = stokes_n(0.0, 0.09, 0.09, Library::Neel, 1.0);
        let rgb2 = rgb_preview(n2, 1.0);
        assert!(
            (rgb - rgb2).length() > 0.02,
            "azimuth should move the HL hue"
        );
    }

    #[test]
    fn lattice_is_stacked_planes() {
        let vol = CoreVolume::new(&core_args());
        assert_eq!(vol.grid(), 8);
        assert_eq!(vol.planes(), 4);
        assert!(vol.static_fibers.is_empty(), "rings stay hidden");
        assert!(vol.live.is_empty(), "separator rings stay hidden");
        assert!(vol.particles.is_empty());
        assert_eq!(vol.cells.len(), 256);
        assert_eq!(vol.orb_instances().len(), 256, "ocean fills every cell");
        assert!((vol.pitch - vol.extent).abs() < 1e-6);
        let xs: Vec<f32> = vol
            .frame_edges()
            .iter()
            .flat_map(|[a, b]| [a.x, b.x])
            .collect();
        let span = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - xs.iter().copied().fold(f32::INFINITY, f32::min);
        let expect = vol.grid() as f32 * vol.extent;
        assert!(
            (span - expect).abs() < 1e-4,
            "frame span {span} should be grid×extent {expect}"
        );
        assert!((vol.orb_scale - vol.extent * PACK).abs() < 1e-4);
    }

    #[test]
    fn pulse_cadence_skips_frame_zero() {
        assert!(!CoreVolume::is_pulse(0));
        assert!(CoreVolume::is_pulse(1));
        assert!(CoreVolume::is_pulse(8));
        let n = (0..32).filter(|&i| CoreVolume::is_pulse(i)).count();
        assert_eq!(n, 31);
    }

    #[test]
    fn geodesic_stamps_polyhedron_on_ocean() {
        let mut vol = CoreVolume::new(&core_args());
        assert_eq!(vol.orb_instances().len(), vol.cells.len());
        vol.pulse_write();
        let n_geo = vol.cells.iter().filter(|c| c.persist >= 1.5).count();
        assert!(
            n_geo >= 12,
            "Class I geodesic polyhedra should stamp a triangular shell, got {n_geo}"
        );
        assert_eq!(vol.geo_tris.len() as u32, GEO_FACES);
        assert_eq!(vol.pixels[0].name, GEO_PIXEL_RED);
        assert_eq!(vol.pixels[1].name, GEO_PIXEL_GREEN);
        assert_eq!(vol.pixels[2].name, GEO_PIXEL_BLUE);
        assert_eq!(vol.pixel_count(), 3);
        assert!((vol.speed() - PIXEL_SPEED).abs() < 1e-4);
        vol.set_pixel_count(9);
        assert_eq!(vol.pixel_count(), 9);
        vol.set_speed(2.0);
        assert!((vol.speed() - 2.0).abs() < 1e-4);
        assert!(vol.pixels[0].color.x > vol.pixels[0].color.y);
        assert!(vol.pixels[1].color.y > vol.pixels[1].color.x);
        assert!(vol.pixels[2].color.z > vol.pixels[2].color.x);
        assert!(vol.live.is_empty());
        assert_eq!(vol.frame_edges().len(), 12);
        assert!((CoreVolume::frame_style().color - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn geodesic_progresses_across_the_lattice() {
        let mut vol = CoreVolume::new(&core_args());
        vol.pulse_write();
        let a = (vol.sense().x_sel, vol.sense().y_sel, vol.sense().z_sel);
        vol.pulse_write();
        let b = (vol.sense().x_sel, vol.sense().y_sel, vol.sense().z_sel);
        assert_ne!(a, b, "geodesic center must step each pulse");
    }

    #[test]
    fn bounce_does_not_paint_face_shockwave() {
        let mut vol = CoreVolume::new(&core_args());
        for _ in 0..80 {
            vol.pulse_write();
        }
        assert!(vol.shocks.is_empty(), "face shockwaves stay hidden");
    }

    #[test]
    fn radial_opacity_is_inner_solid_outer_thin() {
        let vol = CoreVolume::new(&core_args());
        let orbs = vol.orb_instances();
        let mut inner_a = 0.0;
        let mut outer_a = 1.0;
        let mut inner_d = f32::MAX;
        let mut outer_d = 0.0_f32;
        for (p, _, _, a) in &orbs {
            let d = p.length();
            if d < inner_d {
                inner_d = d;
                inner_a = *a;
            }
            if d > outer_d {
                outer_d = d;
                outer_a = *a;
            }
        }
        assert!(inner_a > 0.5, "innermost alpha {inner_a} should be > 0.5");
        assert!(outer_a < 0.5, "outermost alpha {outer_a} should be < 0.5");
        assert!(inner_a > outer_a);
    }

    #[test]
    fn ocean_gradient_travels_against_the_wave() {
        let (_, rgb0, _) = ocean_field(4, 4, 2, 8, 4, 0, PIXEL_SPEED, 0.22);
        let (_, rgb1, _) = ocean_field(4, 4, 2, 8, 4, 24, PIXEL_SPEED, 0.22);
        assert!(
            (rgb0 - rgb1).length() > 0.02,
            "spiral arms must move colour"
        );
    }

    #[test]
    fn ocean_is_grey_with_white_intensity() {
        for pulse in [0u32, 8, 24] {
            let (_, rgb, _) = ocean_field(12, 4, 10, 16, 16, pulse, PIXEL_SPEED, 0.22);
            let chroma = (rgb.x - rgb.y).abs() + (rgb.y - rgb.z).abs();
            assert!(
                chroma < 0.04,
                "manifold should be grey/white, got {rgb:?} at t={pulse}"
            );
            assert!(rgb.min_element() >= 0.0 && rgb.max_element() <= 1.0);
        }
        let (_, bright, _) = ocean_field(14, 2, 15, 16, 16, 0, PIXEL_SPEED, 0.22);
        let (_, dark, _) = ocean_field(8, 8, 15, 16, 16, 0, PIXEL_SPEED, 0.22);
        assert!(
            bright.max_element() > dark.max_element()
                || (bright - dark).length() > 0.02,
            "white intensity should contrast the grey field"
        );
    }

    #[test]
    fn whirlpool_funnel_is_lower_on_the_axis() {
        let grid = 16u32;
        let planes = 16u32;
        let cx = grid / 2;
        let top = planes - 1;
        let (axis, _, _) = ocean_field(cx, cx, top, grid, planes, 0, PIXEL_SPEED, 0.22);
        let (rim, _, _) = ocean_field(1, cx, top, grid, planes, 0, PIXEL_SPEED, 0.22);
        assert!(
            axis.y < rim.y - 0.05,
            "axis funnel {axis:?} should sit below the rim {rim:?}"
        );
    }

    #[test]
    fn whirlpool_swirls_over_time() {
        let grid = 16u32;
        let planes = 16u32;
        let cx = grid / 2;
        let (a, _, _) = ocean_field(cx + 4, cx, planes / 2, grid, planes, 0, PIXEL_SPEED, 0.22);
        let (b, _, _) = ocean_field(cx + 4, cx, planes / 2, grid, planes, 24, PIXEL_SPEED, 0.22);
        let da = Vec3::new(a.x, 0.0, a.z);
        let db = Vec3::new(b.x, 0.0, b.z);
        assert!(
            (db - da).length() > 0.02,
            "Rankine swirl should rotate a mid-radius cell"
        );
    }

    #[test]
    fn rankine_eta_is_bernoulli_in_omega() {
        let a = 8.0;
        let e1 = rankine_eta_cells(0.0, a, 0.10);
        let e2 = rankine_eta_cells(0.0, a, 0.20);
        assert!(
            (e2 / e1 - 4.0).abs() < 0.02,
            "η(0) must scale as Ω², got {} / {}",
            e2,
            e1
        );
        let ea = rankine_eta_cells(a, a, 0.10);
        assert!((e1 / ea - 2.0).abs() < 0.02, "η(0) = 2 η(a) for Rankine");
        let e_out = rankine_eta_cells(2.0 * a, a, 0.10);
        assert!((e_out / ea - 0.25).abs() < 0.02, "outside η ∝ 1/r²");
        assert!(
            (rankine_eta_cells(a, a, 0.10) - rankine_eta_cells(a + 1e-4, a, 0.10)).abs() < 1e-3
        );
    }

    #[test]
    fn vel_deepens_the_funnel() {
        let grid = 16u32;
        let planes = 16u32;
        let cx = grid / 2;
        let top = planes - 1;
        let (slow, _, _) = ocean_field(cx, cx, top, grid, planes, 0, 0.6, 0.22);
        let (fast, _, _) = ocean_field(cx, cx, top, grid, planes, 0, 2.4, 0.22);
        assert!(
            fast.y < slow.y - 0.02,
            "faster Ω should deepen Bernoulli η, slow={slow:?} fast={fast:?}"
        );
    }

    #[test]
    fn swell_stays_out_of_the_core() {
        let grid = 16u32;
        let planes = 16u32;
        let cx = grid / 2;
        let top = planes - 1;
        let ox = (grid - 1) as f32 * 0.5;
        let oy = (planes - 1) as f32 * 0.5;
        let dx = cx as f32 - ox;
        let dz = cx as f32 - ox;
        let rho = (dx * dx + dz * dz).sqrt();
        let a = rankine_core_radius(ox);
        let omega = rankine_omega(PIXEL_SPEED);
        let eta = rankine_eta_cells(rho, a, omega).max(-WHIRL_ETA_CLAMP * oy);
        let y_top = top as f32 / planes as f32;
        let surface = smoother(0.16, 0.92, y_top);
        let (off, _, _) = ocean_field(cx, cx, top, grid, planes, 0, PIXEL_SPEED, 0.22);
        let expect = eta * surface * 0.22;
        assert!(
            (off.y - expect).abs() < 0.02,
            "core height should be Rankine η only, got {} expect {expect}",
            off.y
        );
    }

    #[test]
    fn class_i_counts_match_formula() {
        let t = |n: u32| n * n;
        for n in [1u32, 2, 3, 4] {
            let (v, e, f) = class_i_icosahedral(n);
            assert_eq!(f.len() as u32, 20 * t(n), "{n}v faces");
            assert_eq!(v.len() as u32, 10 * t(n) + 2, "{n}v verts");
            assert_eq!(e.len() as u32, 30 * t(n), "{n}v edges");
            assert!(v.iter().all(|p| (p.length() - 1.0).abs() < 1e-4));
        }
        assert_eq!(GEO_FACES, 80);
    }

    #[test]
    fn class_i_polyhedron_covers_a_shell_on_64() {
        let args = parse_from([
            "--headless",
            "--scene",
            "core",
            "--preset",
            "4090",
            "--frames",
            "2",
            "--no-capture",
        ])
        .unwrap();
        assert_eq!(args.grid, 64);
        assert_eq!(args.planes, 64);
        let mut vol = CoreVolume::new(&args);
        vol.set_pixel_count(1);
        vol.pulse_write();
        let geos = vol.cells.iter().filter(|c| c.persist >= 1.5).count();
        assert!(
            geos >= 80,
            "2v geodesic surface on 64³ should cover many cells, got {geos}"
        );
        let r = geodesic_radius_cells(64, 64);
        assert!(r > 6.0 && r <= 12.0, "radius {r} should read as a polyhedron");
        let orbs = vol.orb_instances();
        assert_eq!(orbs.len(), 64 * 64 * 64);
    }

    #[test]
    fn visible_spectrum_has_no_magenta() {
        let violet = wavelength_rgb(410.0);
        let green = wavelength_rgb(530.0);
        let red = wavelength_rgb(650.0);
        assert!(violet.z > violet.x && violet.z > violet.y);
        assert!(green.y > green.x && green.y > green.z);
        assert!(red.x > red.y && red.x > red.z);
        assert!(wavelength_rgb(500.0).x < 0.15);
    }

    #[test]
    fn analytical_gain_matches_covering() {
        // Θ_0 = gπ, Θ_∞ = 0 → N_sk = Q (1 − cos(gπ)) / 2.
        let g = HALF_GAIN;
        let expect = (1.0 - (g * PI).cos()) * 0.5;
        assert!((expect - 0.232).abs() < 0.02 || expect > 0.15 && expect < 0.40);
        let full = (1.0 - PI.cos()) * 0.5;
        assert!((full - 1.0).abs() < 1e-5);
    }
}
