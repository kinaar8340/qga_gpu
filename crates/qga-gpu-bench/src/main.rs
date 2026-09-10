//! 4090 QGA sculpture bench. Software fact of this binary.
//! Hopf fibers: glam unit-quaternion orbits (Model). No qga-math.

mod args;
mod hopf;
mod record;
mod scene;
mod scene_core;
mod scene_gradient;
mod scene_hold;
mod scene_loom;
mod stats;

use anyhow::{Context, Result};
use args::{Args, Capture, Preset, Scene};
use glam::{Mat4, Vec3};
use hopf::HopfField;
use qga_gpu::{Camera, GpuContext, GpuFiber, GpuParticle, Renderer, UploadStats, VisualState};
use scene_core::{CoreVolume, SliderKind};
use scene_gradient::GradientLattice;
use scene_hold::HoldLattice;
use scene_loom::LoomBraid;
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
    Hold(HoldLattice),
    Loom(LoomBraid),
    Core(CoreVolume),
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
        Scene::Hold => scene_hold::camera_distance(args),
        Scene::Loom => scene_loom::camera_distance(args),
        Scene::Core => scene_core::camera_distance(args),
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
        Scene::Hold => {
            cam.yaw = 0.28;
            cam.pitch = 0.22;
            cam.near = 0.02;
        }
        Scene::Loom => {
            cam.yaw = 0.48;
            cam.pitch = 0.36;
            cam.near = 0.05;
        }
        Scene::Core => {
            cam.yaw = 0.58;
            cam.pitch = 0.48;
            cam.near = 0.04;
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
        Scene::Hold => LiveScene::Hold(HoldLattice::new(args)),
        Scene::Loom => LiveScene::Loom(LoomBraid::new(args)),
        Scene::Core => LiveScene::Core(CoreVolume::new(args)),
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

fn queue_orbs_loom(renderer: &mut Renderer, loom: &LoomBraid) {
    let scale = loom.orb_scale();
    for (pos, color) in loom.orb_centers() {
        renderer.draw_geodesic_orb(
            Mat4::from_translation(pos) * Mat4::from_scale(Vec3::splat(scale)),
            color,
            1,
        );
    }
}

fn queue_orbs_core(renderer: &mut Renderer, vol: &CoreVolume) {
    for (pos, color, scale, alpha) in vol.orb_instances() {
        renderer.draw_geodesic_orb_alpha(
            Mat4::from_translation(pos) * Mat4::from_scale(Vec3::splat(scale)),
            color,
            alpha,
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
        LiveScene::Hold(lat) => {
            if HoldLattice::is_pulse(frame) {
                lat.pulse_correction();
            }
        }
        LiveScene::Loom(loom) => {
            if args.dirty_fibers {
                loom.tick_braid(frame);
            }
            if args.dirty_particles {
                loom.advance_motes(0.008);
            }
        }
        LiveScene::Core(vol) => {
            if CoreVolume::is_pulse(frame) {
                vol.pulse_write();
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
        LiveScene::Hold(lat) => {
            renderer.retain_meshes(gpu, &scene_hold::meshes(), 1)?;
            renderer.retain_static_fibers(gpu, &lat.static_fibers, 0.0)?;
            renderer.write_live_fibers(gpu, &lat.live, 0.0)?;
            renderer.write_particles(gpu, &lat.particles)?;
        }
        LiveScene::Loom(loom) => {
            renderer.retain_static_fibers(gpu, &loom.static_fibers, args.tube_radius * 0.42)?;
            renderer.write_live_fibers(gpu, &loom.live, args.tube_radius)?;
            if args.dirty_particles {
                renderer.write_particles(gpu, &loom.particles)?;
            }
            queue_orbs_loom(renderer, loom);
        }
        LiveScene::Core(vol) => {
            queue_orbs_core(renderer, vol);
            renderer.write_hud(gpu, &vol.hud())?;
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
        LiveScene::Hold(lat) => {
            if args.record.is_none() {
                renderer.write_hud(gpu, &scene_hold::hud(args))?;
            }
            renderer.retain_meshes(gpu, &scene_hold::meshes(), 1)?;
            renderer.update_faces(gpu, &lat.faces());
            renderer.retain_static_fibers(gpu, &lat.static_fibers, 0.0)?;
            renderer.write_live_fibers(gpu, &lat.live, 0.0)?;
            renderer.upload_hubs(gpu, &lat.hubs)?;
            renderer.write_particles(gpu, &lat.particles)?;
        }
        LiveScene::Loom(loom) => {
            renderer.write_hud(gpu, &scene_loom::hud(args, loom.live.len() as u32))?;
            renderer.update_faces(gpu, &loom.fabric);
            renderer.retain_static_fibers(gpu, &loom.static_fibers, args.tube_radius * 0.42)?;
            renderer.write_live_fibers(gpu, &loom.live, args.tube_radius)?;
            if args.dirty_particles {
                renderer.write_particles(gpu, &loom.particles)?;
            } else {
                renderer.write_particles(gpu, &[] as &[GpuParticle])?;
            }
        }
        LiveScene::Core(vol) => {
            renderer.update_line_segments(gpu, &vol.frame_edges(), CoreVolume::frame_style());
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
    live: &LiveScene,
) -> Result<()> {
    if let LiveScene::Core(vol) = live {
        vol.print_sense();
    }
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
    if args.scene == Scene::Core {
        anyhow::ensure!(
            s.static_uploads == 0,
            "core rings are hidden; static_uploads={} expected 0",
            s.static_uploads
        );
        anyhow::ensure!(
            s.live_fiber_writes == 0,
            "core separator rings must stay hidden (live_fiber_writes={})",
            s.live_fiber_writes
        );
        anyhow::ensure!(
            s.particle_fallbacks == 0,
            "particle_fallbacks={} expected 0 on the core wave",
            s.particle_fallbacks
        );
        if matches!(args.preset, Preset::FourK90 | Preset::RingQga) {
            anyhow::ensure!(
                s.particle_grows == grows_after_warmup,
                "particle_grows={} after warmup {} (expected no further grows on {})",
                s.particle_grows,
                grows_after_warmup,
                args.preset.as_str()
            );
        }
        return Ok(());
    }
    anyhow::ensure!(
        s.static_uploads == 1,
        "static fiber buffers were written {} times; expected static_uploads == 1",
        s.static_uploads
    );
    if args.scene == Scene::Hold {
        let pulses = u64::from(frames / scene_hold::PULSE_PERIOD);
        anyhow::ensure!(
            s.live_fiber_writes >= pulses.saturating_sub(2) && s.live_fiber_writes <= pulses + 2,
            "live_fiber_writes={} expected ≈ {pulses} (pulse every {} frames, not {frames})",
            s.live_fiber_writes,
            scene_hold::PULSE_PERIOD
        );
        anyhow::ensure!(
            s.particle_skipped > s.ring_copies,
            "particle_skipped={} should dwarf ring_copies={} (hash-skip path)",
            s.particle_skipped,
            s.ring_copies
        );
        anyhow::ensure!(
            s.particle_fallbacks == 0,
            "particle_fallbacks={} expected 0 on the hold pulse (not a dirty ocean)",
            s.particle_fallbacks
        );
    } else if args.scene == Scene::Core {
        // One drive phase per present. Skip-path is hold's job.
        let pulses = u64::from(frames.saturating_sub(1) / scene_core::PULSE_PERIOD);
        anyhow::ensure!(
            s.live_fiber_writes >= pulses.saturating_sub(2) && s.live_fiber_writes <= pulses + 3,
            "live_fiber_writes={} expected ≈ {pulses} (core raster, not {frames} still frames)",
            s.live_fiber_writes
        );
        anyhow::ensure!(
            s.particle_fallbacks == 0,
            "particle_fallbacks={} expected 0 on the core raster",
            s.particle_fallbacks
        );
    } else if args.dirty_particles {
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
    if !matches!(args.scene, Scene::Hold | Scene::Core) && (args.dirty_fibers || args.dirty_rings) {
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
    let mut vis = vis_from(&args);
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
        let time = i as f32 * dt;
        if args.scene == Scene::Hold {
            scene_hold::breathe(&mut vis, time, args.tube_radius);
        }
        if let LiveScene::Core(vol) = &live {
            vol.breathe(&mut vis);
        }
        upload_live(&gpu, &mut renderer, &args, &live)?;
        let grab = grab_frame(args.capture, i, n);
        let captured = renderer.render(&mut gpu, &camera, &vis, time, grab)?;
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
        &live,
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
    slider_drag: Option<SliderKind>,
}

impl App {
    fn cursor_ndc(&self) -> [f32; 2] {
        self.cursor_ndc_at(self.cursor[0], self.cursor[1])
    }

    fn cursor_ndc_at(&self, x: f32, y: f32) -> [f32; 2] {
        let (w, h) = self
            .window
            .as_ref()
            .map(|w| {
                let s = w.inner_size();
                (s.width.max(1) as f32, s.height.max(1) as f32)
            })
            .unwrap_or((1.0, 1.0));
        [2.0 * x / w - 1.0, 1.0 - 2.0 * y / h]
    }

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
            slider_drag: None,
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
        if self.args.scene == Scene::Hold {
            scene_hold::breathe(&mut self.vis, self.time, self.args.tube_radius);
        }
        if let LiveScene::Core(vol) = &self.live {
            vol.breathe(&mut self.vis);
        }
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
                &self.live,
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
                &self.live,
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
                    if self.lmb {
                        let ndc = self.cursor_ndc();
                        if let LiveScene::Core(vol) = &mut self.live {
                            self.slider_drag = CoreVolume::hit_slider(ndc);
                            if let Some(kind) = self.slider_drag {
                                vol.apply_slider(kind, ndc[0]);
                            }
                        }
                    } else {
                        self.slider_drag = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let x = position.x as f32;
                let y = position.y as f32;
                if self.lmb {
                    let ndc_x = self.cursor_ndc_at(x, y)[0];
                    if let (Some(kind), LiveScene::Core(vol)) =
                        (self.slider_drag, &mut self.live)
                    {
                        vol.apply_slider(kind, ndc_x);
                    } else {
                        self.camera
                            .orbit_delta(x - self.cursor[0], y - self.cursor[1]);
                    }
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
                        KeyCode::BracketLeft | KeyCode::Comma => {
                            if let LiveScene::Core(vol) = &mut self.live {
                                vol.nudge_count(-1);
                            }
                        }
                        KeyCode::BracketRight | KeyCode::Period => {
                            if let LiveScene::Core(vol) = &mut self.live {
                                vol.nudge_count(1);
                            }
                        }
                        KeyCode::Minus => {
                            if let LiveScene::Core(vol) = &mut self.live {
                                vol.nudge_speed(-0.15);
                            }
                        }
                        KeyCode::Equal => {
                            if let LiveScene::Core(vol) = &mut self.live {
                                vol.nudge_speed(0.15);
                            }
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
