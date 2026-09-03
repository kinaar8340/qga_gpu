//! Parametric tessellation. Software fact: cones/spheres/tori are generated once.

use crate::types::{FaceVert, GpuFiber};
use glam::Vec3;

const TAU: f32 = std::f32::consts::TAU;
const PI: f32 = std::f32::consts::PI;

pub fn sphere_faces(radius: f32, color: Vec3, n_lat: u32, n_lon: u32) -> Vec<FaceVert> {
    let n_lat = n_lat.max(3);
    let n_lon = n_lon.max(3);
    let mut faces = Vec::new();
    for j in 0..n_lat {
        let v0 = j as f32 / n_lat as f32;
        let v1 = (j + 1) as f32 / n_lat as f32;
        let th0 = v0 * PI;
        let th1 = v1 * PI;
        for i in 0..n_lon {
            let u0 = i as f32 / n_lon as f32;
            let u1 = (i + 1) as f32 / n_lon as f32;
            let ph0 = u0 * TAU;
            let ph1 = u1 * TAU;
            let p00 = sphere_point(radius, th0, ph0);
            let p10 = sphere_point(radius, th0, ph1);
            let p01 = sphere_point(radius, th1, ph0);
            let p11 = sphere_point(radius, th1, ph1);
            push_quad(&mut faces, p00, p10, p11, p01, color, 1.0);
        }
    }
    faces
}

pub fn cone_faces(radius: f32, height: f32, color: Vec3, n: u32) -> Vec<FaceVert> {
    let n = n.max(3);
    let apex = Vec3::new(0.0, height, 0.0);
    let mut faces = Vec::new();
    for i in 0..n {
        let a0 = i as f32 / n as f32 * TAU;
        let a1 = (i + 1) as f32 / n as f32 * TAU;
        let b0 = Vec3::new(radius * a0.cos(), 0.0, radius * a0.sin());
        let b1 = Vec3::new(radius * a1.cos(), 0.0, radius * a1.sin());
        push_tri(&mut faces, apex, b1, b0, color, 1.0);
        // Base in XZ, facing −Y.
        push_tri(&mut faces, Vec3::ZERO, b0, b1, color, 0.85);
    }
    faces
}

/// Closed centerline of a torus in the XZ plane (Y-up, horizontal ring).
pub fn torus_centerline(major: f32, minor: f32, n: u32, color: Vec3) -> GpuFiber {
    let n = n.max(8);
    let _minor = minor;
    let mut points = Vec::with_capacity(n as usize);
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        points.push(Vec3::new(major * a.cos(), 0.0, major * a.sin()));
    }
    GpuFiber { points, color }
}

/// Lat/lon wire edges of a sphere. Pair list for `update_line_segments`.
pub fn sphere_edges(radius: f32, n_lat: u32, n_lon: u32) -> Vec<[Vec3; 2]> {
    let n_lat = n_lat.max(3);
    let n_lon = n_lon.max(3);
    let mut edges = Vec::new();
    for j in 1..n_lat {
        let th = j as f32 / n_lat as f32 * PI;
        for i in 0..n_lon {
            let ph0 = i as f32 / n_lon as f32 * TAU;
            let ph1 = (i + 1) as f32 / n_lon as f32 * TAU;
            edges.push([sphere_point(radius, th, ph0), sphere_point(radius, th, ph1)]);
        }
    }
    for i in 0..n_lon {
        let ph = i as f32 / n_lon as f32 * TAU;
        for j in 0..n_lat {
            let th0 = j as f32 / n_lat as f32 * PI;
            let th1 = (j + 1) as f32 / n_lat as f32 * PI;
            edges.push([sphere_point(radius, th0, ph), sphere_point(radius, th1, ph)]);
        }
    }
    edges
}

pub fn cone_edges(radius: f32, height: f32, n: u32) -> Vec<[Vec3; 2]> {
    let n = n.max(3);
    let apex = Vec3::new(0.0, height, 0.0);
    let mut edges = Vec::new();
    let mut ring = Vec::with_capacity(n as usize);
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        ring.push(Vec3::new(radius * a.cos(), 0.0, radius * a.sin()));
    }
    for i in 0..n as usize {
        edges.push([apex, ring[i]]);
        edges.push([ring[i], ring[(i + 1) % n as usize]]);
    }
    edges
}

pub fn torus_edges(major: f32, minor: f32, n_major: u32, n_minor: u32) -> Vec<[Vec3; 2]> {
    let n_major = n_major.max(8);
    let n_minor = n_minor.max(4);
    let mut edges = Vec::new();
    let point = |i: u32, j: u32| {
        let u = i as f32 / n_major as f32 * TAU;
        let v = j as f32 / n_minor as f32 * TAU;
        let cx = major * u.cos();
        let cz = major * u.sin();
        Vec3::new(
            cx + minor * v.cos() * u.cos(),
            minor * v.sin(),
            cz + minor * v.cos() * u.sin(),
        )
    };
    for i in 0..n_major {
        for j in 0..n_minor {
            let a = point(i, j);
            edges.push([a, point((i + 1) % n_major, j)]);
            edges.push([a, point(i, (j + 1) % n_minor)]);
        }
    }
    edges
}

fn sphere_point(radius: f32, theta: f32, phi: f32) -> Vec3 {
    Vec3::new(
        radius * theta.sin() * phi.cos(),
        radius * theta.cos(),
        radius * theta.sin() * phi.sin(),
    )
}

fn push_quad(out: &mut Vec<FaceVert>, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: Vec3, alpha: f32) {
    push_tri(out, a, b, c, color, alpha);
    push_tri(out, a, c, d, color, alpha);
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

/// Parametric primitive. Tessellated once on first upload. Y-up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeshKind {
    Sphere { radius: f32 },
    Cone { radius: f32, height: f32 },
    Torus { major: f32, minor: f32 },
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub kind: MeshKind,
    pub color: Vec3,
    pub rot_x: f32,
    pub rot_z: f32,
}

impl Mesh {
    pub fn sphere(radius: f32) -> Self {
        Self {
            kind: MeshKind::Sphere { radius },
            color: Vec3::new(0.75, 0.82, 0.95),
            rot_x: 0.0,
            rot_z: 0.0,
        }
    }

    /// Apex at origin, base in the XZ plane, height along +Y.
    pub fn cone(radius: f32, height: f32) -> Self {
        Self {
            kind: MeshKind::Cone { radius, height },
            color: Vec3::new(0.8, 0.8, 0.8),
            rot_x: 0.0,
            rot_z: 0.0,
        }
    }

    /// Major ring in the XZ plane (horizontal separator, Y-up).
    pub fn torus(major: f32, minor: f32) -> Self {
        Self {
            kind: MeshKind::Torus { major, minor },
            color: Vec3::new(0.95, 0.85, 0.2),
            rot_x: 0.0,
            rot_z: 0.0,
        }
    }

    pub fn rotated_x(mut self, a: f32) -> Self {
        self.rot_x += a;
        self
    }

    pub fn rotated_z(mut self, a: f32) -> Self {
        self.rot_z += a;
        self
    }

    pub fn colored(mut self, rgb: [f32; 3]) -> Self {
        self.color = Vec3::from_array(rgb);
        self
    }

    fn map_point(&self, p: Vec3) -> Vec3 {
        rot_z(rot_x(p, self.rot_x), self.rot_z)
    }

    pub fn is_torus(&self) -> bool {
        matches!(self.kind, MeshKind::Torus { .. })
    }

    /// CPU tessellation for a lod. Software fact: lod 0/1/2 = coarse/med/fine.
    pub fn tessellate(&self, lod: u32) -> Tessellated {
        let (n_lat, n_lon, n_cone, n_torus) = match lod {
            0 => (6, 8, 8, 24),
            2 => (16, 24, 24, 96),
            _ => (10, 14, 16, 64),
        };
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        let mut fibers = Vec::new();
        match self.kind {
            MeshKind::Sphere { radius } => {
                faces.extend(
                    sphere_faces(radius, self.color, n_lat, n_lon)
                        .into_iter()
                        .map(|mut v| {
                            let p = self.map_point(Vec3::from_array(v.pos));
                            let n = self.map_point(Vec3::from_array(v.nrm)).normalize_or_zero();
                            v.pos = p.into();
                            v.nrm = n.into();
                            v
                        })
                        .collect::<Vec<_>>(),
                );
                edges.extend(
                    sphere_edges(radius, n_lat, n_lon)
                        .into_iter()
                        .map(|[a, b]| [self.map_point(a), self.map_point(b)]),
                );
            }
            MeshKind::Cone { radius, height } => {
                faces.extend(
                    cone_faces(radius, height, self.color, n_cone)
                        .into_iter()
                        .map(|mut v| {
                            let p = self.map_point(Vec3::from_array(v.pos));
                            let n = self.map_point(Vec3::from_array(v.nrm)).normalize_or_zero();
                            v.pos = p.into();
                            v.nrm = n.into();
                            v
                        })
                        .collect::<Vec<_>>(),
                );
                edges.extend(
                    cone_edges(radius, height, n_cone)
                        .into_iter()
                        .map(|[a, b]| [self.map_point(a), self.map_point(b)]),
                );
            }
            MeshKind::Torus { major, minor } => {
                let mut f = torus_centerline(major, minor, n_torus, self.color);
                f.points = f.points.into_iter().map(|p| self.map_point(p)).collect();
                fibers.push(f);
                edges.extend(
                    torus_edges(major, minor, n_torus, 6)
                        .into_iter()
                        .map(|[a, b]| [self.map_point(a), self.map_point(b)]),
                );
            }
        }
        Tessellated {
            faces,
            edges,
            fibers,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Tessellated {
    pub faces: Vec<FaceVert>,
    pub edges: Vec<[Vec3; 2]>,
    pub fibers: Vec<GpuFiber>,
}

fn rot_x(p: Vec3, a: f32) -> Vec3 {
    let (s, c) = a.sin_cos();
    Vec3::new(p.x, c * p.y - s * p.z, s * p.y + c * p.z)
}

fn rot_z(p: Vec3, a: f32) -> Vec3 {
    let (s, c) = a.sin_cos();
    Vec3::new(c * p.x - s * p.y, s * p.x + c * p.y, p.z)
}
