//! Frame timing + JSON record. Software fact of this binary.

use crate::args::{Args, Capture, Scene};
use qga_gpu::UploadStats;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
pub struct BenchRecord {
    pub gitsha: String,
    pub scene: String,
    pub preset: String,
    pub grid: u32,
    pub rings: u32,
    pub width: u32,
    pub height: u32,
    pub fibers: u32,
    pub fiber_samples: u32,
    pub particles: u32,
    pub orbs: u32,
    pub frames: u32,
    pub glow: bool,
    pub dirty_particles: bool,
    pub dirty_fibers: bool,
    pub multiply: String,
    pub tube_radius: f32,
    pub ms_wall: f64,
    pub ms_mean: f64,
    pub ms_min: f64,
    pub ms_max: f64,
    pub hz: f64,
    pub write_buffer_calls: u64,
    pub ring_copies: u64,
    pub static_uploads: u64,
    pub static_skipped: u64,
    pub live_skipped: u64,
    pub particle_skipped: u64,
    pub particle_grows: u64,
    pub particle_fallbacks: u64,
    pub fiber_reallocs: u64,
    pub vram_estimate_bytes: u64,
    pub capture_bytes: usize,
    pub claims: &'static str,
    pub not_a_proof_of: &'static str,
}

pub struct FrameTimer {
    start: Instant,
    last: Instant,
    samples: Vec<f64>,
}

impl FrameTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last: now,
            samples: Vec::new(),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        self.samples
            .push(now.duration_since(self.last).as_secs_f64() * 1.0e3);
        self.last = now;
    }

    pub fn summary(&self) -> (f64, f64, f64, f64, f64) {
        let wall_ms = self.start.elapsed().as_secs_f64() * 1.0e3;
        if self.samples.is_empty() {
            return (0.0, 0.0, 0.0, 0.0, wall_ms);
        }
        let n = self.samples.len() as f64;
        let sum: f64 = self.samples.iter().copied().sum();
        let mean = sum / n;
        let min = self.samples.iter().copied().fold(f64::INFINITY, f64::min);
        let max = self
            .samples
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let hz = if wall_ms > 0.0 {
            n * 1.0e3 / wall_ms
        } else {
            0.0
        };
        (mean, min, max, hz, wall_ms)
    }
}

pub fn vram_estimate(args: &Args) -> u64 {
    const R: u64 = 32;
    let particles = R * u64::from(args.particles);
    let live = R * u64::from(args.fibers) * u64::from(args.fiber_samples);
    let orbs = R * u64::from(args.orbs);
    let ring = particles * 3;
    let px = u64::from(args.width) * u64::from(args.height);
    let color = px * 4;
    let depth = px * 4;
    let resolve = if args.glow { color } else { 0 };
    let capture = match args.capture {
        Capture::FirstLast | Capture::All => {
            let bpr = u64::from(args.width) * 4;
            let padded = bpr.div_ceil(256) * 256;
            padded * u64::from(args.height)
        }
        Capture::None => 0,
    };
    particles + live + orbs + ring + color + depth + resolve + capture
}

pub fn gitsha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

pub fn write_record(path: &Path, rec: &BenchRecord) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(rec)?;
    fs::write(path, json)?;
    Ok(())
}

/// Uncompressed BGR BMP. No extra crate. Gitignored under results/.
pub fn write_bgra_bmp(path: &Path, width: u32, height: u32, bgra: &[u8]) -> anyhow::Result<()> {
    let row = ((width * 3 + 3) / 4) * 4;
    let pixel_bytes = row * height;
    let file_size = 54 + pixel_bytes;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = fs::File::create(path)?;
    let mut hdr = [0u8; 54];
    hdr[0] = b'B';
    hdr[1] = b'M';
    hdr[2..6].copy_from_slice(&file_size.to_le_bytes());
    hdr[10] = 54;
    hdr[14] = 40;
    hdr[18..22].copy_from_slice(&width.to_le_bytes());
    hdr[22..26].copy_from_slice(&height.to_le_bytes());
    hdr[26] = 1;
    hdr[28] = 24;
    hdr[34..38].copy_from_slice(&pixel_bytes.to_le_bytes());
    f.write_all(&hdr)?;
    let mut rowbuf = vec![0u8; row as usize];
    for y in 0..height {
        let src_y = height - 1 - y;
        let src = (src_y * width * 4) as usize;
        for x in 0..width as usize {
            let i = src + x * 4;
            let o = x * 3;
            rowbuf[o] = bgra[i];
            rowbuf[o + 1] = bgra[i + 1];
            rowbuf[o + 2] = bgra[i + 2];
        }
        f.write_all(&rowbuf)?;
    }
    Ok(())
}

pub fn print_report(
    args: &Args,
    frames: u32,
    last_bytes: usize,
    s: UploadStats,
    timer: &FrameTimer,
) {
    let (mean, min, max, hz, wall) = timer.summary();
    let wb = s.write_buffer_calls;
    let rc = s.ring_copies;
    let su = s.static_uploads;
    let ss = s.static_skipped;
    let ls = s.live_skipped;
    let ps = s.particle_skipped;
    let pg = s.particle_grows;
    let pf = s.particle_fallbacks;
    let fr = s.fiber_reallocs;
    match args.scene {
        Scene::Gradient => println!(
            "done scene=gradient preset={} frames={frames} {}x{} grid={} orbs={} rings={} samples={} particles={}",
            args.preset.as_str(),
            args.width,
            args.height,
            args.grid,
            args.orbs,
            args.fibers,
            args.fiber_samples,
            args.particles
        ),
        Scene::Hopf => println!(
            "done scene=hopf preset={} frames={frames} {}x{} fibers={} samples={} particles={} orbs={}",
            args.preset.as_str(),
            args.width,
            args.height,
            args.fibers,
            args.fiber_samples,
            args.particles,
            args.orbs
        ),
    }
    println!(
        "ms_mean={mean:.3} ms_min={min:.3} ms_max={max:.3} hz={hz:.1} wall_ms={wall:.1} capture_bytes={last_bytes}"
    );
    println!(
        "write_buffer={wb} ring_copies={rc} static_uploads={su} static_skipped={ss} live_skipped={ls} particle_skipped={ps} particle_grows={pg} particle_fallbacks={pf} fiber_reallocs={fr}"
    );
    println!("claims=Software fact  not_a_proof_of=inner_cone mosaic / qga-app cosmos");
}

pub fn record(
    args: &Args,
    frames: u32,
    last_bytes: usize,
    s: UploadStats,
    timer: &FrameTimer,
) -> BenchRecord {
    let (mean, min, max, hz, wall) = timer.summary();
    BenchRecord {
        gitsha: gitsha(),
        scene: args.scene.as_str().to_string(),
        preset: args.preset.as_str().to_string(),
        grid: args.grid,
        rings: if args.scene == Scene::Gradient {
            args.fibers
        } else {
            0
        },
        width: args.width,
        height: args.height,
        fibers: args.fibers,
        fiber_samples: args.fiber_samples,
        particles: args.particles,
        orbs: args.orbs,
        frames,
        glow: args.glow,
        dirty_particles: args.dirty_particles,
        dirty_fibers: args.dirty_fibers,
        multiply: args.multiply.as_str().to_string(),
        tube_radius: args.tube_radius,
        ms_wall: wall,
        ms_mean: mean,
        ms_min: min,
        ms_max: max,
        hz,
        write_buffer_calls: s.write_buffer_calls,
        ring_copies: s.ring_copies,
        static_uploads: s.static_uploads,
        static_skipped: s.static_skipped,
        live_skipped: s.live_skipped,
        particle_skipped: s.particle_skipped,
        particle_grows: s.particle_grows,
        particle_fallbacks: s.particle_fallbacks,
        fiber_reallocs: s.fiber_reallocs,
        vram_estimate_bytes: vram_estimate(args),
        capture_bytes: last_bytes,
        claims: "Software fact",
        not_a_proof_of: "inner_cone mosaic / qga-app cosmos",
    }
}
