use glam::Vec3;
use qga_gpu::{cone_edges, cone_faces, sphere_edges, sphere_faces, torus_centerline, torus_edges};

#[test]
fn sphere_faces_sit_on_radius() {
    let r = 1.25;
    let faces = sphere_faces(r, Vec3::ONE, 8, 12);
    assert!(!faces.is_empty());
    for v in &faces {
        let p = Vec3::from_array(v.pos);
        let len = p.length();
        assert!((len - r).abs() < 1e-4, "len={len}");
    }
}

#[test]
fn torus_centerline_is_horizontal_xz() {
    let f = torus_centerline(1.05, 0.03, 32, Vec3::new(1.0, 0.85, 0.2));
    assert_eq!(f.points.len(), 32);
    for p in &f.points {
        let y = p.y;
        assert!(y.abs() < 1e-5, "y={y}");
        let radial = (p.x * p.x + p.z * p.z).sqrt();
        assert!((radial - 1.05).abs() < 1e-4, "xz={radial}");
    }
}

#[test]
fn cone_apex_at_height() {
    let faces = cone_faces(0.8, 1.2, Vec3::ONE, 12);
    assert!(!faces.is_empty());
    let mut saw_apex = false;
    for v in &faces {
        let p = Vec3::from_array(v.pos);
        if (p - Vec3::new(0.0, 1.2, 0.0)).length() < 1e-5 {
            saw_apex = true;
        }
    }
    assert!(saw_apex);
}

#[test]
fn geodesic_edges_are_pairs() {
    assert!(!sphere_edges(1.0, 6, 8).is_empty());
    assert!(!cone_edges(0.6, 0.9, 8).is_empty());
    assert!(!torus_edges(1.0, 0.05, 16, 6).is_empty());
}

#[test]
fn mesh_tessellate_once_shapes() {
    use qga_gpu::Mesh;
    let sphere = Mesh::sphere(0.4).tessellate(1);
    assert!(!sphere.faces.is_empty());
    let cone = Mesh::cone(0.5, 0.8).tessellate(1);
    assert!(!cone.faces.is_empty());
    let torus = Mesh::torus(1.05, 0.03).tessellate(1);
    assert!(!torus.fibers.is_empty());
    assert_eq!(torus.faces.len(), 0);
}
