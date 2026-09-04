//! Tiny binary: 1 sphere, 2 cones, separator torus, 4k particles.
//! Smoke only (`make demo-tiny` / `make ring`). Public demo is `make demo`.
//! Software fact: no QGA scene graph.

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use qga_gpu::{
    hud_text, Camera, GpuContext, GpuHub, GpuParticle, HudVert, Mesh, Renderer, UploadStats,
    VisualState,
};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const N_PARTICLES: usize = 4096;

struct Args {
    headless: bool,
    frames: u32,
    /// Mutate particles every frame so the ring is exercised (no hash skip).
    dirty_particles: bool,
}

fn parse_args() -> Args {
    let mut headless = false;
    let mut frames = 0;
    let mut dirty_particles = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--headless" => headless = true,
            "--dirty-particles" => dirty_particles = true,
            "--frames" => {
                frames = it.next().and_then(|s| s.parse().ok()).unwrap_or(8);
            }
            _ => {}
        }
    }
    if headless && frames == 0 {
        frames = 8;
    }
    Args {
        headless,
        frames,
        dirty_particles,
    }
}

fn scene_meshes() -> Vec<Mesh> {
    vec![
        Mesh::sphere(0.35).colored([0.75, 0.82, 0.95]),
        Mesh::cone(0.55, 0.95).colored([0.20, 0.60, 1.00]),
        Mesh::cone(0.42, 0.75)
            .rotated_x(std::f32::consts::FRAC_PI_2)
            .colored([1.00, 0.40, 0.20]),
        Mesh::torus(1.05, 0.03).colored([0.95, 0.85, 0.20]),
    ]
}

fn nudge_particles(parts: &mut [GpuParticle], tick: u32) {
    let dy = 1.0e-4 * (tick as f32 + 1.0);
    for p in parts {
        p.pos[1] += dy;
    }
}

fn report_stats(
    frames: u32,
    last_bytes: usize,
    s: UploadStats,
    dirty_particles: bool,
) -> Result<()> {
    let wb = s.write_buffer_calls;
    let rc = s.ring_copies;
    let su = s.static_uploads;
    let ss = s.static_skipped;
    let ls = s.live_skipped;
    let ps = s.particle_skipped;
    let pg = s.particle_grows;
    let pf = s.particle_fallbacks;
    println!(
        "done frames={frames} capture_bytes={last_bytes} write_buffer={wb} ring_copies={rc} static_uploads={su} static_skipped={ss} live_skipped={ls} particle_skipped={ps} particle_grows={pg} particle_fallbacks={pf}"
    );
    anyhow::ensure!(
        su == 1,
        "static fiber buffers were written {su} times; expected static_uploads == 1"
    );
    if dirty_particles {
        anyhow::ensure!(ps == 0, "dirty particles must not hash-skip ({ps})");
        // Fallbacks counted, not fatal. This 4090 first+last headless sat at
        // pf == 0 (ring reclaimed before CPU lapped map_async). Do not add a
        // 4th slot on that result. Windowed FIFO should sit at pf == 0.
        let landed = rc + pf;
        anyhow::ensure!(
            landed >= u64::from(frames),
            "ring_copies={rc} fallbacks={pf} expected >= {frames} dirty writes"
        );
    }
    Ok(())
}

fn spawn_particles() -> Vec<GpuParticle> {
    let mut parts = Vec::with_capacity(N_PARTICLES);
    for i in 0..N_PARTICLES {
        let a = i as f32 * 2.399963; // golden angle
        let r = ((i as f32 + 0.5) / N_PARTICLES as f32).sqrt() * 1.35;
        let y = ((i as f32 * 0.017).sin()) * 0.08;
        parts.push(
            GpuParticle::new(
                Vec3::new(r * a.cos(), y, r * a.sin()),
                Vec3::new(-a.sin(), 0.02, a.cos()) * 0.15,
                0.35,
            )
            .with_hue((i % 5) as f32 * 0.18 + 0.05),
        );
    }
    parts
}

fn upload_static(gpu: &GpuContext, renderer: &mut Renderer) -> Result<()> {
    renderer.retain_meshes(gpu, &scene_meshes(), 1)?;
    renderer.upload_hubs(
        gpu,
        &[GpuHub::new(Vec3::ZERO, 0.10, Vec3::new(1.0, 0.85, 0.35))],
    )?;
    renderer.draw_geodesic_orb(
        Mat4::from_translation(Vec3::new(1.6, 0.2, 0.0)) * Mat4::from_scale(Vec3::splat(0.18)),
        Vec3::new(0.55, 1.0, 0.45),
        1,
    );
    let mut hud = Vec::<HudVert>::new();
    hud_text(
        &mut hud,
        -0.92,
        0.88,
        0.018,
        "QGA-GPU DEMO",
        [0.92, 0.95, 1.0, 0.92],
    );
    renderer.write_hud(gpu, &hud)?;
    Ok(())
}

fn run_headless(frames: u32, dirty_particles: bool) -> Result<()> {
    let mut gpu = GpuContext::init_headless().context("init_headless")?;
    println!("{}", gpu.report());
    let mut renderer = Renderer::new(&gpu)?;
    let camera = Camera::orbit(Vec3::ZERO, 4.2);
    let vis = VisualState {
        glow: 0.9,
        pulse: 0.45,
        tube_radius: 0.03,
        ..VisualState::default()
    };
    upload_static(&gpu, &mut renderer)?;
    let mut particles = spawn_particles();
    renderer.write_particles(&gpu, &particles)?;
    if !dirty_particles {
        // Identical rewrite must no-op.
        renderer.write_particles(&gpu, &particles)?;
    }
    renderer.retain_meshes(&gpu, &scene_meshes(), 1)?;

    let mut last_bytes = 0usize;
    let n = frames.max(1);
    for i in 0..n {
        if dirty_particles {
            nudge_particles(&mut particles, i);
        }
        renderer.write_particles(&gpu, &particles)?;
        // Capture first + last only. Mid-run Wait would hide in-flight map_async.
        let grab = i == 0 || i + 1 == n;
        let captured = renderer.render(&mut gpu, &camera, &vis, i as f32 * 0.016, grab)?;
        if let Some(frame) = captured {
            last_bytes = frame.bgra.len();
            if i == 0 {
                let nonempty = frame.bgra.iter().any(|&b| b != 0);
                let w = frame.width;
                let h = frame.height;
                println!("frame 0 {w}x{h} bytes={last_bytes} nonempty={nonempty}");
            }
        }
    }
    report_stats(n, last_bytes, renderer.upload_stats(), dirty_particles)
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    camera: Camera,
    vis: VisualState,
    particles: Vec<GpuParticle>,
    last: Instant,
    time: f32,
    lmb: bool,
    cursor: [f32; 2],
    dirty_particles: bool,
    frame_limit: u32,
    frames_drawn: u32,
}

impl App {
    fn new(dirty_particles: bool, frame_limit: u32) -> Self {
        Self {
            window: None,
            gpu: None,
            renderer: None,
            camera: Camera::orbit(Vec3::ZERO, 4.2),
            vis: VisualState {
                glow: 0.9,
                pulse: 0.45,
                tube_radius: 0.03,
                ..VisualState::default()
            },
            particles: spawn_particles(),
            last: Instant::now(),
            time: 0.0,
            lmb: false,
            cursor: [0.0, 0.0],
            dirty_particles,
            frame_limit,
            frames_drawn: 0,
        }
    }

    fn boot(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let title = if self.dirty_particles {
            "qga-gpu-demo (dirty particles)"
        } else {
            "qga-gpu-demo"
        };
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32));
        let window = Arc::new(event_loop.create_window(attrs)?);
        let gpu = GpuContext::init_windowed(window.clone())?;
        log::info!("{}", gpu.report());
        let size = window.inner_size();
        self.camera.aspect = size.width as f32 / size.height.max(1) as f32;
        let mut renderer = Renderer::new(&gpu)?;
        upload_static(&gpu, &mut renderer)?;
        renderer.write_particles(&gpu, &self.particles)?;
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.last = Instant::now();
        Ok(())
    }

    /// `Ok(false)` means hit `--frames` and the loop should exit.
    fn tick(&mut self) -> Result<bool> {
        let dt = self.last.elapsed().as_secs_f32().clamp(0.0, 0.05);
        self.last = Instant::now();
        if !self.vis.paused {
            self.time += dt;
            self.camera.tick_cinematic(dt);
        }
        if self.dirty_particles {
            nudge_particles(&mut self.particles, self.frames_drawn);
        }
        let gpu = self.gpu.as_mut().context("gpu")?;
        let renderer = self.renderer.as_mut().context("renderer")?;
        renderer.write_particles(gpu, &self.particles)?;
        renderer.render(gpu, &self.camera, &self.vis, self.time, false)?;
        self.frames_drawn += 1;
        if self.frame_limit > 0 && self.frames_drawn >= self.frame_limit {
            report_stats(
                self.frames_drawn,
                0,
                renderer.upload_stats(),
                self.dirty_particles,
            )?;
            return Ok(false);
        }
        Ok(true)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.boot(event_loop) {
                log::error!("boot: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                    self.camera.aspect = size.width as f32 / size.height.max(1) as f32;
                }
            }
            WindowEvent::RedrawRequested => match self.tick() {
                Ok(true) => {}
                Ok(false) => event_loop.exit(),
                Err(e) => {
                    log::error!("frame: {e:#}");
                    event_loop.exit();
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.lmb = state == ElementState::Pressed;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let x = position.x as f32;
                let y = position.y as f32;
                if self.lmb {
                    self.camera
                        .orbit_delta(x - self.cursor[0], y - self.cursor[1]);
                }
                self.cursor = [x, y];
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                };
                self.camera.zoom(d);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::Space => self.vis.paused = !self.vis.paused,
                        KeyCode::KeyC => self.camera.cinematic = !self.camera.cinematic,
                        KeyCode::KeyG => {
                            self.vis.glow = if self.vis.glow > 0.5 { 0.2 } else { 0.9 }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }
}

fn run_windowed(dirty_particles: bool, frames: u32) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(dirty_particles, frames);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = parse_args();
    if args.headless {
        run_headless(args.frames, args.dirty_particles)
    } else {
        run_windowed(args.dirty_particles, args.frames)
    }
}
