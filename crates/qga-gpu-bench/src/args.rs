//! Hand-rolled CLI. Same dialect as qga-gpu-demo (`--headless`, `--frames`,
//! `--dirty-particles`). Extra flags are long-form only. Unknown flags error.

use std::path::PathBuf;

const PARTICLE_CAP: u32 = 8_388_608;
const FIBER_CAP: u32 = 16_384;
const SAMPLE_CAP: u32 = 256;
const ORB_CAP: u32 = 65_536;
const GRID_CAP: u32 = 96;
const STAGING_BUDGET: u64 = 1_073_741_824;
const RECORD: u64 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Smoke,
    RingQga,
    FourK90,
    Soak,
}

impl Preset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::RingQga => "ring-qga",
            Self::FourK90 => "4090",
            Self::Soak => "soak",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "smoke" => Some(Self::Smoke),
            "ring-qga" => Some(Self::RingQga),
            "4090" => Some(Self::FourK90),
            "soak" => Some(Self::Soak),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    /// Default. Current Hopf field + observer sculpture.
    Hopf,
    /// ngsm lattice. Alias `--scene ngsm`.
    Gradient,
}

impl Scene {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hopf => "hopf",
            Self::Gradient => "gradient",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "hopf" | "sculpture" => Some(Self::Hopf),
            "gradient" | "ngsm" => Some(Self::Gradient),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Multiply {
    Left,
    Right,
}

impl Multiply {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    None,
    FirstLast,
    /// Every frame. Video only — capture Wait hides ring pressure. Not `make bench`.
    All,
}

#[derive(Clone, Debug)]
pub struct Args {
    pub headless: bool,
    pub frames: u32,
    pub width: u32,
    pub height: u32,
    pub preset: Preset,
    pub fibers: u32,
    pub fiber_samples: u32,
    pub particles: u32,
    pub orbs: u32,
    pub tube_radius: f32,
    pub multiply: Multiply,
    pub dirty_particles: bool,
    pub dirty_fibers: bool,
    pub glow: bool,
    pub capture: Capture,
    pub out_dir: PathBuf,
    pub json: PathBuf,
    pub record: Option<PathBuf>,
    pub cinematic: bool,
    pub scene: Scene,
    pub grid: u32,
    pub cell_extent: f32,
    pub orb_scale: f32,
    pub ring_radius: f32,
    pub ring_tube: f32,
    pub dirty_rings: bool,
    /// Speakers (orbs+rings) + glass fabric + rainbow particle bed.
    pub fluid: bool,
}

struct Seed {
    fibers: u32,
    samples: u32,
    particles: u32,
    orbs: u32,
    frames: u32,
    width: u32,
    height: u32,
    glow: bool,
    dirty_particles: bool,
    dirty_fibers: bool,
}

impl Preset {
    fn seed(self) -> Seed {
        match self {
            Self::Smoke => Seed {
                fibers: 256,
                samples: 32,
                particles: 4_096,
                orbs: 64,
                frames: 60,
                width: 1280,
                height: 720,
                glow: false,
                dirty_particles: true,
                dirty_fibers: false,
            },
            Self::RingQga => Seed {
                fibers: 1_024,
                samples: 48,
                particles: 65_536,
                orbs: 256,
                frames: 300,
                width: 1280,
                height: 720,
                glow: false,
                dirty_particles: true,
                dirty_fibers: false,
            },
            Self::FourK90 => Seed {
                fibers: 4_096,
                samples: 64,
                particles: 262_144,
                orbs: 1_024,
                frames: 600,
                width: 1920,
                height: 1080,
                glow: true,
                dirty_particles: true,
                dirty_fibers: true,
            },
            Self::Soak => Seed {
                fibers: 8_192,
                samples: 64,
                particles: 1_048_576,
                orbs: 4_096,
                frames: 1_200,
                width: 2560,
                height: 1440,
                glow: true,
                dirty_particles: true,
                dirty_fibers: true,
            },
        }
    }
}

pub fn usage() -> &'static str {
    "qga-gpu-bench [flags]

  4090 QGA bench (Software fact of this binary, not a theorem).
  Hopf fibers are glam unit-quaternion orbits (Model). No qga-math.
  --scene gradient is a Model inspired by Toshiyuki Nagashima (@ngsm)
  'gradient / structure' — not a port of the p5.js sketch.

  Public demo (make demo): --scene gradient --preset 4090 --grid 64 --fluid
  until Esc. 4096 speakers + 65536 motes. Prints UploadStats on exit.

Mode
  --scene hopf|sculpture|gradient|ngsm   default hopf (CLI; make demo is gradient)
  --headless                 init_headless (frames default from preset)
  --frames N                 exit after N presents (0 = unlimited windowed)
  --width N  --height N      swapchain / offscreen size
  --out DIR                  JSON + optional BMP (default benchmarks/results)

Scene scale (preset first, then overrides)
  --preset smoke|ring-qga|4090|soak   default 4090
  --fibers N                 live centerlines (cap 16384)  [hopf]
  --fiber-samples N          points / fiber (cap 256)
  --particles N              GpuParticle 32 B (cap 8388608)
  --orbs N                   draw_geodesic_orb instances (cap 65536)
  --tube-radius F            live + VisualState (0.002…0.08)
  --multiply left|right      exp(θu)*q0 or q0*exp(θu)  (Model)

Gradient only
  --grid N                   cells on a side (cap 96)
  --cell-extent F            spacing (default 0.22)
  --orb-scale F              geodesic orb scale (default 0.055)
  --ring-radius F            torus major in cell units (default 0.09)
  --ring-tube F              ring tube_radius (default 0.004)
  --dirty-rings              tumble ring quaternions every frame
  --fluid                    speakers (orbs+rings) + glass fabric + particle bed

Dirty / present
  --dirty-particles          advance mote phase every frame (no hash skip)
  --dirty-fibers             rotate Hopf generator u each frame
  --clean                    still lattice (clears preset dirty flags)
  --glow / --no-glow         VisualState.glow on / off
  --capture-first-last       capture frames 0 and N-1 only
  --no-capture               no readback
  --record PATH.mp4          headless, every frame → ffmpeg (not a ring proof)
  --json PATH                BenchRecord (default $OUT/<preset>-<frames>.json)

Hopf presets
  smoke     256 fibers,  4096 particles,   60 frames, 1280x720
  ring-qga 1024 fibers, 65536 particles,  300 frames, 1280x720
  4090     4096 fibers,  262144 particles, 600 frames, 1920x1080, glow
  soak     8192 fibers, 1048576 particles, 1200 frames, 2560x1440, glow

Gradient presets (same frames/size; grid is the scale)
  smoke     grid=8    1280x720   dirty-rings off
  ring-qga  grid=16   1280x720   dirty-rings on
  4090      grid=32   1920x1080  dirty-rings on, glow
  soak      grid=48   2560x1440  dirty-rings on, glow

Default with no flags: windowed --preset 4090 until Esc.
Unknown flags are an error (exit 2).
"
}

fn take_u32(flag: &str, it: &mut impl Iterator<Item = String>) -> Result<u32, i32> {
    let Some(v) = it.next() else {
        eprintln!("missing value for {flag}");
        return Err(2);
    };
    v.parse().map_err(|_| {
        eprintln!("bad {flag} value {v}");
        2
    })
}

fn take_f32(flag: &str, it: &mut impl Iterator<Item = String>) -> Result<f32, i32> {
    let Some(v) = it.next() else {
        eprintln!("missing value for {flag}");
        return Err(2);
    };
    v.parse().map_err(|_| {
        eprintln!("bad {flag} value {v}");
        2
    })
}

pub fn parse() -> Result<Args, i32> {
    parse_from(std::env::args().skip(1))
}

pub fn parse_from<I, S>(argv: I) -> Result<Args, i32>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut headless = false;
    let mut frames: Option<u32> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut preset = Preset::FourK90;
    let mut fibers: Option<u32> = None;
    let mut fiber_samples: Option<u32> = None;
    let mut particles: Option<u32> = None;
    let mut orbs: Option<u32> = None;
    let mut tube_radius: Option<f32> = None;
    let mut multiply = Multiply::Left;
    let mut dirty_particles_flag = false;
    let mut dirty_fibers_flag = false;
    let mut clean = false;
    let mut glow_flag: Option<bool> = None;
    let mut capture_first_last = false;
    let mut no_capture = false;
    let mut out_dir = PathBuf::from("benchmarks/results");
    let mut json: Option<PathBuf> = None;
    let mut record: Option<PathBuf> = None;
    let mut scene = Scene::Hopf;
    let mut grid: Option<u32> = None;
    let mut cell_extent: Option<f32> = None;
    let mut orb_scale: Option<f32> = None;
    let mut ring_radius: Option<f32> = None;
    let mut ring_tube: Option<f32> = None;
    let mut dirty_rings_flag = false;
    let mut fluid = false;

    let mut it = argv.into_iter().map(|s| s.as_ref().to_string());
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{}", usage());
                return Err(0);
            }
            "--headless" => headless = true,
            "--dirty-particles" => dirty_particles_flag = true,
            "--dirty-fibers" => dirty_fibers_flag = true,
            "--dirty-rings" => dirty_rings_flag = true,
            "--fluid" => fluid = true,
            "--clean" => clean = true,
            "--glow" => glow_flag = Some(true),
            "--no-glow" => glow_flag = Some(false),
            "--capture-first-last" => capture_first_last = true,
            "--no-capture" => no_capture = true,
            "--frames" => frames = Some(take_u32("--frames", &mut it)?),
            "--width" => width = Some(take_u32("--width", &mut it)?),
            "--height" => height = Some(take_u32("--height", &mut it)?),
            "--fibers" => fibers = Some(take_u32("--fibers", &mut it)?),
            "--fiber-samples" => fiber_samples = Some(take_u32("--fiber-samples", &mut it)?),
            "--particles" => particles = Some(take_u32("--particles", &mut it)?),
            "--orbs" => orbs = Some(take_u32("--orbs", &mut it)?),
            "--preset" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --preset");
                    return Err(2);
                };
                preset = Preset::parse(&v).ok_or_else(|| {
                    eprintln!("unknown preset {v} (smoke|ring-qga|4090|soak)");
                    2
                })?;
            }
            "--multiply" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --multiply");
                    return Err(2);
                };
                multiply = match v.as_str() {
                    "left" => Multiply::Left,
                    "right" => Multiply::Right,
                    _ => {
                        eprintln!("--multiply wants left|right, got {v}");
                        return Err(2);
                    }
                };
            }
            "--tube-radius" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --tube-radius");
                    return Err(2);
                };
                tube_radius = Some(v.parse().map_err(|_| {
                    eprintln!("bad --tube-radius {v}");
                    2
                })?);
            }
            "--out" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --out");
                    return Err(2);
                };
                out_dir = PathBuf::from(v);
            }
            "--json" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --json");
                    return Err(2);
                };
                json = Some(PathBuf::from(v));
            }
            "--record" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --record");
                    return Err(2);
                };
                record = Some(PathBuf::from(v));
            }
            "--scene" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --scene");
                    return Err(2);
                };
                scene = Scene::parse(&v).ok_or_else(|| {
                    eprintln!("unknown scene {v} (hopf|sculpture|gradient|ngsm)");
                    2
                })?;
            }
            "--grid" => grid = Some(take_u32("--grid", &mut it)?),
            "--cell-extent" => cell_extent = Some(take_f32("--cell-extent", &mut it)?),
            "--orb-scale" => orb_scale = Some(take_f32("--orb-scale", &mut it)?),
            "--ring-radius" => ring_radius = Some(take_f32("--ring-radius", &mut it)?),
            "--ring-tube" => ring_tube = Some(take_f32("--ring-tube", &mut it)?),
            other => {
                eprintln!("unknown flag {other}");
                eprintln!("try --help");
                return Err(2);
            }
        }
    }

    let seed = preset.seed();
    let (g_grid, g_samples, g_dirty_rings, g_glow) = match preset {
        Preset::Smoke => (8, 48, false, false),
        Preset::RingQga => (16, 64, true, false),
        Preset::FourK90 => (32, 64, true, true),
        Preset::Soak => (48, 64, true, true),
    };
    let grid = grid.unwrap_or(g_grid);
    let cell_extent = cell_extent.unwrap_or(0.22);
    let orb_scale = orb_scale.unwrap_or(0.055);
    let ring_radius = ring_radius.unwrap_or(0.09);
    let ring_tube = ring_tube.unwrap_or(0.004);

    let mut fibers = fibers.unwrap_or(seed.fibers);
    let mut fiber_samples = fiber_samples.unwrap_or(seed.samples);
    let particles_opt = particles;
    let mut particles = particles.unwrap_or(seed.particles);
    let mut orbs = orbs.unwrap_or(seed.orbs);
    let width = width.unwrap_or(seed.width).max(1);
    let height = height.unwrap_or(seed.height).max(1);
    let mut tube_radius =
        tube_radius.unwrap_or_else(|| (0.90 / (fibers as f32).sqrt()).clamp(0.008, 0.05));
    let mut dirty_particles = seed.dirty_particles;
    let mut dirty_fibers = seed.dirty_fibers;
    let mut dirty_rings = false;
    let mut glow = glow_flag.unwrap_or(seed.glow);

    if scene == Scene::Gradient {
        let n = grid.saturating_mul(grid);
        fiber_samples = if fiber_samples == seed.samples {
            g_samples
        } else {
            fiber_samples
        };
        fibers = n;
        orbs = n;
        dirty_particles = false;
        dirty_fibers = false;
        dirty_rings = g_dirty_rings;
        glow = glow_flag.unwrap_or(g_glow);
        tube_radius = ring_tube;
        particles = 0;
    }

    if clean {
        dirty_particles = false;
        dirty_fibers = false;
        dirty_rings = false;
    }
    if dirty_particles_flag {
        dirty_particles = true;
    }
    if dirty_fibers_flag {
        dirty_fibers = true;
    }
    if dirty_rings_flag {
        dirty_rings = true;
    }
    if fluid {
        dirty_particles = true;
        dirty_rings = true;
        let side = (grid.saturating_mul(8)).clamp(32, 256);
        particles = particles_opt.unwrap_or(side.saturating_mul(side));
    } else if scene == Scene::Gradient && dirty_particles && particles == 0 {
        particles = 4 * grid.saturating_mul(grid);
    }

    // Hopf record uses a yaw crane. Gradient waves are the motion; keep the
    // camera locked so the silhouette stays edge-on.
    let cinematic = record.is_some() && scene == Scene::Hopf;
    if record.is_some() {
        headless = true;
        if no_capture {
            eprintln!("--record needs frame readback; do not pass --no-capture");
            return Err(2);
        }
    }
    let mut n_frames = frames.unwrap_or(0);
    if headless && n_frames == 0 {
        n_frames = seed.frames;
    }

    let capture = if record.is_some() {
        Capture::All
    } else if no_capture {
        Capture::None
    } else if capture_first_last {
        Capture::FirstLast
    } else if headless {
        Capture::FirstLast
    } else {
        Capture::None
    };

    if scene == Scene::Gradient && grid > GRID_CAP {
        eprintln!("grid {grid} exceeds cap {GRID_CAP}");
        return Err(2);
    }
    if particles > PARTICLE_CAP {
        eprintln!(
            "particles {particles} exceeds cap {PARTICLE_CAP} (~256 MiB VB at 32 B/record); refusing before wgpu OOM"
        );
        return Err(2);
    }
    if fibers > FIBER_CAP {
        eprintln!("fibers {fibers} exceeds cap {FIBER_CAP}");
        return Err(2);
    }
    if fiber_samples > SAMPLE_CAP || fiber_samples < 2 {
        eprintln!("fiber-samples {fiber_samples} out of range 2…{SAMPLE_CAP}");
        return Err(2);
    }
    if orbs > ORB_CAP {
        eprintln!("orbs {orbs} exceeds cap {ORB_CAP}");
        return Err(2);
    }
    if !(0.002..=0.08).contains(&tube_radius) {
        eprintln!("tube-radius {tube_radius} out of range 0.002…0.08");
        return Err(2);
    }

    let staging = RECORD
        * (u64::from(particles) + u64::from(fibers) * u64::from(fiber_samples) + u64::from(orbs));
    if staging > STAGING_BUDGET {
        eprintln!(
            "CPU staging budget {staging} B exceeds 1 GiB (32 × (particles + fibers × samples + orbs)); refusing before wgpu OOM"
        );
        return Err(2);
    }

    let json = json.unwrap_or_else(|| {
        let name = if scene == Scene::Gradient {
            format!("{}-{}-{}.json", scene.as_str(), preset.as_str(), n_frames)
        } else {
            format!("{}-{}.json", preset.as_str(), n_frames)
        };
        out_dir.join(name)
    });

    Ok(Args {
        headless,
        frames: n_frames,
        width,
        height,
        preset,
        fibers,
        fiber_samples,
        particles,
        orbs,
        tube_radius,
        multiply,
        dirty_particles,
        dirty_fibers,
        glow,
        capture,
        out_dir,
        json,
        record,
        cinematic,
        scene,
        grid,
        cell_extent,
        orb_scale,
        ring_radius,
        ring_tube,
        dirty_rings,
        fluid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_windowed_4090() {
        let a = parse_from(Vec::<String>::new()).unwrap();
        assert!(!a.headless);
        assert_eq!(a.frames, 0);
        assert_eq!(a.preset, Preset::FourK90);
        assert_eq!(a.fibers, 4096);
        assert_eq!(a.particles, 262_144);
        assert!(a.dirty_particles && a.dirty_fibers && a.glow);
        assert_eq!(a.capture, Capture::None);
    }

    #[test]
    fn headless_fills_preset_frames_and_capture() {
        let a = parse_from(["--headless"]).unwrap();
        assert_eq!(a.frames, 600);
        assert_eq!(a.capture, Capture::FirstLast);
        assert_eq!(a.width, 1920);
        assert_eq!(a.height, 1080);
    }

    #[test]
    fn overrides_beat_preset() {
        let a = parse_from([
            "--preset",
            "smoke",
            "--fibers",
            "10",
            "--particles",
            "32",
            "--no-glow",
            "--clean",
            "--dirty-particles",
        ])
        .unwrap();
        assert_eq!(a.fibers, 10);
        assert_eq!(a.particles, 32);
        assert_eq!(a.orbs, 64);
        assert!(!a.glow);
        assert!(a.dirty_particles);
        assert!(!a.dirty_fibers);
    }

    #[test]
    fn unknown_flag_is_error() {
        assert_eq!(parse_from(["--nope"]).unwrap_err(), 2);
    }

    #[test]
    fn particle_cap_rejects() {
        assert_eq!(parse_from(["--particles", "9000000"]).unwrap_err(), 2);
    }

    #[test]
    fn record_forces_headless_all_frames() {
        let a = parse_from(["--record", "out.mp4"]).unwrap();
        assert!(a.headless);
        assert!(a.cinematic);
        assert_eq!(a.capture, Capture::All);
        assert_eq!(a.frames, 600);
        assert_eq!(a.record.as_deref(), Some(std::path::Path::new("out.mp4")));
    }

    #[test]
    fn default_scene_is_hopf() {
        let a = parse_from(Vec::<String>::new()).unwrap();
        assert_eq!(a.scene, Scene::Hopf);
        assert_eq!(a.fibers, 4096);
    }

    #[test]
    fn gradient_4090_is_32_grid() {
        let a = parse_from(["--headless", "--scene", "gradient", "--preset", "4090"]).unwrap();
        assert_eq!(a.scene, Scene::Gradient);
        assert_eq!(a.grid, 32);
        assert_eq!(a.orbs, 1024);
        assert_eq!(a.fibers, 1024);
        assert_eq!(a.fiber_samples, 64);
        assert_eq!(a.particles, 0);
        assert!(a.dirty_rings);
        assert!(!a.dirty_particles);
        assert!(a.glow);
        assert!((a.ring_tube - 0.004).abs() < 1e-6);
    }

    #[test]
    fn fluid_forces_particle_bed() {
        let a = parse_from(["--scene", "gradient", "--preset", "4090", "--fluid"]).unwrap();
        assert!(a.fluid);
        assert!(a.dirty_particles && a.dirty_rings);
        assert_eq!(a.particles, 256 * 256);
        assert_eq!(a.orbs, 1024);
    }

    #[test]
    fn public_demo_flags_are_65k_ocean() {
        let a = parse_from([
            "--scene",
            "gradient",
            "--preset",
            "4090",
            "--grid",
            "64",
            "--fluid",
            "--dirty-rings",
            "--dirty-particles",
        ])
        .unwrap();
        assert!(!a.headless);
        assert_eq!(a.frames, 0);
        assert_eq!(a.scene, Scene::Gradient);
        assert_eq!(a.grid, 64);
        assert_eq!(a.orbs, 4096);
        assert_eq!(a.fibers, 4096);
        assert_eq!(a.particles, 65_536);
        assert!(a.fluid && a.dirty_particles && a.dirty_rings);
        assert_eq!(a.capture, Capture::None);
    }

    #[test]
    fn ngsm_alias_and_grid_override() {
        let a = parse_from(["--scene", "ngsm", "--preset", "smoke", "--grid", "10"]).unwrap();
        assert_eq!(a.scene, Scene::Gradient);
        assert_eq!(a.grid, 10);
        assert_eq!(a.orbs, 100);
        assert!(!a.dirty_rings);
        assert_eq!(a.fiber_samples, 48);
    }
}
