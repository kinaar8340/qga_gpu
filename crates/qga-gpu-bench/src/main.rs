//! 4090 QGA sculpture bench. Software fact of this binary.
//! Hopf fibers: glam unit-quaternion orbits (Model). No qga-math.

mod args;
mod hopf;
mod record;
mod scene;
mod scene_gradient;
mod stats;

use anyhow::{Context, Result};
use args::{Args, Capture, Preset, Scene};
use glam::{Mat4, Vec3};
use hopf::HopfField;
use qga_gpu::{Camera, GpuContext, GpuFiber, GpuParticle, Renderer, UploadStats, VisualState};
use scene_gradient::GradientLattice;
use stats::FrameTimer;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = match args::parse() {
        Ok(a) => a,
        Err(0) => return,
        Err(code) => std::process::exit(code),
    };
    if let Err(e) = run(args) {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    if args.headless {
        run_headless(args)
    } else {
        run_windowed(args)
    }
}

enum LiveScene {
    Hopf(HopfField),
    Gradient(GradientLattice),
}

fn vis_from(args: &Args) -> VisualState {
    VisualState {
        glow: if args.glow { 0.55 } else { 0.18 },
        pulse: 0.45,
        tube_radius: args.tube_radius,
        ..VisualState::default()
    }
}

fn camera_from(args: &Args) -> Camera {
    let dist = match args.scene {
        Scene::Hopf => 48.0,
        Scene::Gradient => scene_gradient::camera_distance(args),
    };
    let mut cam = Camera::orbit(Vec3::ZERO, dist);
    match args.scene {
        Scene::Hopf => {
            cam.yaw = 1.12;
            cam.pitch = 0.48;
        }
        Scene::Gradient => {
            if args.fluid {
                cam.yaw = 0.22;
                cam.pitch = 0.08;
                cam.near = 0.02;
            } else {
                cam.yaw = 0.78;
                cam.pitch = 0.62;
            }
        }
    }
    cam.aspect = args.width as f32 / args.height.max(1) as f32;
    cam.cinematic = args.cinematic;
    cam
}

fn live_from(args: &Args) -> LiveScene {
    match args.scene {
        Scene::Hopf => LiveScene::Hopf(HopfField::new(
            args.fibers,
            args.fiber_samples,
            args.particles,
            args.orbs,
            args.multiply,
        )),
        Scene::Gradient => LiveScene::Gradient(GradientLattice::new(args)),
    }
}

fn queue_orbs_hopf(renderer: &mut Renderer, hopf: &HopfField) {
    let scale = hopf.orb_scale();
    for (pos, color) in hopf.orb_centers() {
        renderer.draw_geodesic_orb(
            Mat4::from_translation(pos) * Mat4::from_scale(Vec3::splat(scale)),
            color,
            1,
        );
    }
}

fn queue_orbs_gradient(renderer: &mut Renderer, lat: &GradientLattice) {
    let scale = lat.orb_scale;
    for (pos, color) in lat.orb_instances() {
        renderer.draw_geodesic_orb(
            Mat4::from_translation(pos) * Mat4::from_scale(Vec3::splat(scale)),
            color,
            1,
        );
    }
}

fn tick_live(args: &Args, live: &mut LiveScene, frame: u32) {
    match live {
        LiveScene::Hopf(hopf) => {
            if args.dirty_fibers {
                hopf.tick_generator(frame);
            }
            if args.dirty_particles {
                hopf.advance_motes(0.008);
            }
        }
        LiveScene::Gradient(lat) => {
            if args.dirty_rings {
                lat.tick_rings(0.012);
            }
            if args.dirty_particles {
                lat.advance_motes(0.008);
            }
        }
    }
}

fn upload_live(
    gpu: &GpuContext,
    renderer: &mut Renderer,
    args: &Args,
    live: &LiveScene,
) -> Result<()> {
    match live {
        LiveScene::Hopf(hopf) => {
            renderer.write_live_fibers(gpu, &hopf.fibers, args.tube_radius)?;
            renderer.write_particles(gpu, &hopf.particles)?;
            queue_orbs_hopf(renderer, hopf);
        }
        LiveScene::Gradient(lat) => {
            if args.dirty_rings {
                renderer.write_live_fibers(gpu, &lat.fibers, args.ring_tube)?;
            }
            if args.fluid {
                renderer.update_faces(gpu, &lat.fabric);
            }
            if args.dirty_particles {
                renderer.write_particles(gpu, &lat.particles)?;
            }
            queue_orbs_gradient(renderer, lat);
        }
    }
    Ok(())
}

fn warmup(gpu: &GpuContext, renderer: &mut Renderer, args: &Args, live: &LiveScene) -> Result<u64> {
    match live {
        LiveScene::Hopf(hopf) => {
            renderer.retain_meshes(gpu, &scene::sculpture_meshes(), 1)?;
            renderer.upload_hubs(gpu, &[scene::observer_hub()])?;
            renderer.write_hud(
                gpu,
                &scene::hud(args.preset.as_str(), args.fibers, args.particles),
            )?;
            renderer.write_live_fibers(gpu, &hopf.fibers, args.tube_radius)?;
            renderer.write_particles(gpu, &hopf.particles)?;
            renderer.retain_meshes(gpu, &scene::sculpture_meshes(), 1)?;
        }
        LiveScene::Gradient(lat) => {
            if args.record.is_none() {
                renderer.write_hud(gpu, &scene_gradient::hud(args))?;
            }
            // Count one static retain so static_uploads == 1, then clear so
            // dirty rings live on the live slot only.
            renderer.retain_static_fibers(gpu, &lat.fibers, args.ring_tube)?;
            if args.dirty_rings {
                renderer.retain_static_fibers(gpu, &[] as &[GpuFiber], args.ring_tube)?;
                renderer.write_live_fibers(gpu, &lat.fibers, args.ring_tube)?;
            }
            if args.fluid {
                renderer.update_faces(gpu, &lat.fabric);
            }
            if args.dirty_particles {
                renderer.write_particles(gpu, &lat.particles)?;
            } else {
                renderer.write_particles(gpu, &[] as &[GpuParticle])?;
            }
        }
    }
    Ok(renderer.upload_stats().particle_grows)
}

fn grab_frame(capture: Capture, i: u32, n: u32) -> bool {
    match capture {
        Capture::None => false,
        Capture::FirstLast => i == 0 || i + 1 == n,
        Capture::All => true,
    }
}

fn maybe_save_capture(
    args: &Args,
    i: u32,
    n: u32,
    frame: &qga_gpu::CapturedFrame,
) -> Result<usize> {
    let name = if i == 0 {
        format!("{}-frame0.bmp", args.preset.as_str())
    } else {
        format!("{}-frame{}.bmp", args.preset.as_str(), n.saturating_sub(1))
    };
    let path = args.out_dir.join(name);
    stats::write_bgra_bmp(&path, frame.width, frame.height, &frame.bgra)?;
    Ok(frame.bgra.len())
}

fn finish(
    args: &Args,
    frames: u32,
    last_bytes: usize,
    stats: UploadStats,
    timer: &FrameTimer,
    grows_after_warmup: u64,
) -> Result<()> {
    stats::print_report(args, frames, last_bytes, stats, timer);
    if args.headless && args.record.is_none() {
        assert_headless(args, frames, stats, grows_after_warmup)?;
    }
    let rec = stats::record(args, frames, last_bytes, stats, timer);
    stats::write_record(&args.json, &rec)?;
    println!("json {}", args.json.display());
    Ok(())
}

fn assert_headless(
    args: &Args,
    frames: u32,
    s: UploadStats,
    grows_after_warmup: u64,
) -> Result<()> {
    anyhow::ensure!(
        s.static_uploads == 1,
        "static fiber buffers were written {} times; expected static_uploads == 1",
        s.static_uploads
    );
    if args.dirty_particles {
        anyhow::ensure!(
            s.particle_skipped == 0,
            "dirty particles must not hash-skip ({})",
            s.particle_skipped
        );
        let landed = s.ring_copies + s.particle_fallbacks;
        anyhow::ensure!(
            landed >= u64::from(frames),
            "ring_copies={} fallbacks={} expected >= {frames} dirty writes",
            s.ring_copies,
            s.particle_fallbacks
        );
    }
    if args.dirty_fibers || args.dirty_rings {
        anyhow::ensure!(
            s.live_skipped < u64::from(frames),
            "dirty fibers/rings hashed-skip every frame (live_skipped={})",
            s.live_skipped
        );
    }
    if matches!(args.preset, Preset::FourK90 | Preset::RingQga) {
        anyhow::ensure!(
            s.particle_grows == grows_after_warmup,
            "particle_grows={} after warmup {} (expected no further grows on {})",
            s.particle_grows,
            grows_after_warmup,
            args.preset.as_str()
        );
    }
    Ok(())
}

fn run_headless(args: Args) -> Result<()> {
    let mut gpu =
        GpuContext::init_headless_extent(args.width, args.height).context("init_headless")?;
    println!("{}", gpu.report());
    let mut renderer = Renderer::new(&gpu)?;
    let mut camera = camera_from(&args);
    let vis = vis_from(&args);
    let mut live = live_from(&args);
    let grows_after_warmup = warmup(&gpu, &mut renderer, &args, &live)?;

    let mut writer = match args.record.as_ref() {
        Some(path) => Some(record::Mp4Writer::spawn(path, args.width, args.height, 30)?),
        None => None,
    };
    if writer.is_some() {
        println!(
            "record {} {}x{} frames={} (capture Wait; not a ring proof)",
            args.record.as_ref().unwrap().display(),
            args.width,
            args.height,
            args.frames.max(1)
        );
    }

    let mut last_bytes = 0usize;
    let n = args.frames.max(1);
    let mut timer = FrameTimer::new();
    let dt = 1.0 / 30.0;
    for i in 0..n {
        tick_live(&args, &mut live, i);
        camera.tick_cinematic(dt);
        upload_live(&gpu, &mut renderer, &args, &live)?;
        let grab = grab_frame(args.capture, i, n);
        let captured = renderer.render(&mut gpu, &camera, &vis, i as f32 * dt, grab)?;
        if let Some(frame) = captured {
            last_bytes = frame.bgra.len();
            if let Some(w) = writer.as_mut() {
                w.write_bgra(&frame.bgra)?;
            } else {
                last_bytes = maybe_save_capture(&args, i, n, &frame)?;
            }
            if i == 0 {
                let nonempty = frame.bgra.iter().any(|&b| b != 0);
                println!(
                    "frame 0 {}x{} bytes={} nonempty={nonempty}",
                    frame.width, frame.height, last_bytes
                );
            }
        }
        timer.tick();
    }
    if let Some(w) = writer {
        let path = w.finish()?;
        println!("mp4 {}", path.display());
    }
    finish(
        &args,
        n,
        last_bytes,
        renderer.upload_stats(),
        &timer,
        grows_after_warmup,
    )
}

struct App {
    args: Args,
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    camera: Camera,
    vis: VisualState,
    live: LiveScene,
    last: Instant,
    time: f32,
    lmb: bool,
    cursor: [f32; 2],
    frames_drawn: u32,
    grows_after_warmup: u64,
    timer: FrameTimer,
    last_bytes: usize,
}

impl App {
    fn new(args: Args) -> Self {
        let live = live_from(&args);
        let camera = camera_from(&args);
        let vis = vis_from(&args);
        Self {
            args,
            window: None,
            gpu: None,
            renderer: None,
            camera,
            vis,
            live,
            last: Instant::now(),
            time: 0.0,
            lmb: false,
            cursor: [0.0, 0.0],
            frames_drawn: 0,
            grows_after_warmup: 0,
            timer: FrameTimer::new(),
            last_bytes: 0,
        }
    }

    fn boot(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let title = format!(
            "qga-gpu-bench ({} {})",
            self.args.scene.as_str(),
            self.args.preset.as_str()
        );
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::PhysicalSize::new(
                self.args.width,
                self.args.height,
            ));
        let window = Arc::new(event_loop.create_window(attrs)?);
        let gpu = GpuContext::init_windowed(window.clone())?;
        log::info!("{}", gpu.report());
        let size = window.inner_size();
        self.camera.aspect = size.width as f32 / size.height.max(1) as f32;
        let mut renderer = Renderer::new(&gpu)?;
        self.grows_after_warmup = warmup(&gpu, &mut renderer, &self.args, &self.live)?;
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.last = Instant::now();
        self.timer = FrameTimer::new();
        Ok(())
    }

    fn tick(&mut self) -> Result<bool> {
        let dt = self.last.elapsed().as_secs_f32().clamp(0.0, 0.05);
        self.last = Instant::now();
        if !self.vis.paused {
            self.time += dt;
            self.camera.tick_cinematic(dt);
        }
        tick_live(&self.args, &mut self.live, self.frames_drawn);
        let gpu = self.gpu.as_mut().context("gpu")?;
        let renderer = self.renderer.as_mut().context("renderer")?;
        upload_live(gpu, renderer, &self.args, &self.live)?;
        let n = self.args.frames;
        let grab = n > 0 && grab_frame(self.args.capture, self.frames_drawn, n);
        let captured = renderer.render(gpu, &self.camera, &self.vis, self.time, grab)?;
        if let Some(frame) = captured {
            self.last_bytes = maybe_save_capture(&self.args, self.frames_drawn, n, &frame)?;
        }
        self.timer.tick();
        self.frames_drawn += 1;
        if self.args.frames > 0 && self.frames_drawn >= self.args.frames {
            finish(
                &self.args,
                self.frames_drawn,
                self.last_bytes,
                renderer.upload_stats(),
                &self.timer,
                self.grows_after_warmup,
            )?;
            return Ok(false);
        }
        Ok(true)
    }

    fn finish_early(&mut self) {
        if let Some(renderer) = self.renderer.as_ref() {
            let _ = finish(
                &self.args,
                self.frames_drawn,
                self.last_bytes,
                renderer.upload_stats(),
                &self.timer,
                self.grows_after_warmup,
            );
        }
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
            WindowEvent::CloseRequested => {
                self.finish_early();
                event_loop.exit();
            }
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
                        KeyCode::Escape => {
                            self.finish_early();
                            event_loop.exit();
                        }
                        KeyCode::Space => self.vis.paused = !self.vis.paused,
                        KeyCode::KeyC => self.camera.cinematic = !self.camera.cinematic,
                        KeyCode::KeyG => {
                            self.vis.glow = if self.vis.glow > 0.4 { 0.18 } else { 0.55 }
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

fn run_windowed(args: Args) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(args);
    event_loop.run_app(&mut app)?;
    Ok(())
}
