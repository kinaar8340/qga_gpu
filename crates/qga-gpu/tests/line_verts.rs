use glam::Vec3;
use qga_gpu::{Camera, GpuContext, LineVert, Renderer, VisualState};

#[test]
fn mixed_line_verts_headless_smoke() {
    let mut gpu = match GpuContext::init_headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skip init_headless: {e}");
            return;
        }
    };
    let mut renderer = Renderer::new(&gpu).expect("Renderer::new");
    let cyan = [0.20, 0.60, 1.00, 1.0];
    let magenta = [0.85, 0.25, 0.80, 1.0];
    let verts = [
        LineVert {
            pos: [-0.5, 0.0, 0.0],
            pad: 0.0,
            color: cyan,
        },
        LineVert {
            pos: [0.5, 0.0, 0.0],
            pad: 0.0,
            color: cyan,
        },
        LineVert {
            pos: [0.0, -0.5, 0.0],
            pad: 0.0,
            color: magenta,
        },
        LineVert {
            pos: [0.0, 0.5, 0.0],
            pad: 0.0,
            color: magenta,
        },
    ];
    renderer.update_line_verts(&gpu, &verts);
    let camera = Camera::orbit(Vec3::ZERO, 4.2);
    let vis = VisualState::default();
    renderer
        .render(&mut gpu, &camera, &vis, 0.0, false)
        .expect("render");
}
