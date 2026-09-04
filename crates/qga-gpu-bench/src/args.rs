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
    /// Static lattice holds; uniforms breathe; live + motes pulse every 30.
    Hold,
    /// Shared Hopf frame + stacked phase-color braid (photonic fabric loom).
    Loom,
}

impl Scene {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hopf => "hopf",
            Self::Gradient => "gradient",
            Self::Hold => "hold",
            Self::Loom => "loom",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "hopf" | "sculpture" => Some(Self::Hopf),
            "gradient" | "ngsm" => Some(Self::Gradient),
            "hold" => Some(Self::Hold),
            "loom" | "braid" | "fabric" => Some(Self::Loom),
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
    /// Loom base-space flux. Elliptic = nested tori. Hyperbolic = gated braid.
    pub flux: Flux,
    /// Loom phase coupling in [0, 1]. High locks ψ, low lets the weave wander.
    pub lambda: f32,
    /// Loom mosaic tiles on a side (1 = one chart). `--mosaic 2` / `2x2`.
    pub mosaic: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flux {
    Elliptic,
    Hyperbolic,
}

impl Flux {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Elliptic => "elliptic",
            Self::Hyperbolic => "hyperbolic",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "elliptic" | "closed" => Some(Self::Elliptic),
            "hyperbolic" | "river" | "gated" => Some(Self::Hyperbolic),
            _ => None,
        }
    }
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
  --scene hold is a Model of a frozen lattice: static topology once,
  uniforms every frame, live harmonics + motes on a 30-frame pulse.
  --scene loom is a Model of inverse Hopf from a Cartesian Γ-chart:
  N×N warp/weft (static), cells near three S² latitudes (live tubes),
  particle fill. Alias braid|fabric. Not a fabricated silica loom.

  Public demo (make demo): --scene gradient --preset 4090 --grid 64 --fluid
  until Esc. 4096 speakers + 65536 motes. Prints UploadStats on exit.
  Two-clock skip proof (make bench-hold): --scene hold --preset 4090
  --frames 300 --headless --no-capture.
  Hold encode (make bench-hold-record): --grid 64 1440p, in-sheet, no HUD
  (capture Wait; not a skip proof).
  Loom smoke (make bench-loom-smoke): --scene loom --preset smoke.

Mode
  --scene hopf|sculpture|gradient|ngsm|hold|loom|braid|fabric   default hopf (CLI; make demo is gradient)
  --headless                 init_headless (frames default from preset)
  --frames N                 exit after N presents (0 = unlimited windowed)
  --width N  --height N      swapchain / offscreen size
  --out DIR                  JSON + optional BMP (default benchmarks/results)

Scene scale (preset first, then overrides)
  --preset smoke|ring-qga|4090|soak   default 4090
  --fibers N                 live centerlines (cap 16384)  [hopf; hold 8–16; loom unused]
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

Loom sculpt (Model; not inner_cone mosaic / hull)
  --flux elliptic|hyperbolic  nested tori (default) or gated river-braid
  --lambda F                  phase lock 0…1 (default 0.15)
  --mosaic 1|2|2x2            independent chart tiles (default 1)

Dirty / present
  --dirty-particles          advance mote phase every frame (no hash skip)
  --dirty-fibers             rotate Hopf generator / loom chart phase each frame
  --clean                    still lattice (clears preset dirty flags)
  --glow / --no-glow         VisualState.glow on / off
  --capture-first-last       capture frames 0 and N-1 only
  --no-capture               no readback
  --record PATH.mp4          headless, every frame → ffmpeg (not a ring proof).
                             Loom orbits (nested tori); hopf records crane.
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

Hold (grid from preset; live tubes 12; 16k motes; dirty flags off)
  4090      grid=32   1920x1080  glow, pulse every 30 frames

Loom (--grid = N×N Cartesian cells; live = latitude stitches, not N²)
  smoke     grid=16  elliptic  λ=0.15  mosaic=1   4096 motes   1280x720
  ring-qga  grid=16  elliptic  λ=0.15  mosaic=1  16384 motes   1280x720
  4090      grid=16  elliptic  λ=0.15  mosaic=1  32768 motes   1920x1080, glow
  soak      grid=32  elliptic  λ=0.15  mosaic=1  65536 motes   2560x1440, glow
  Enable cells near θ=π/4, π/2, 3π/4; inverse-Hopf each to a tube.
  Cartesian warp/weft is the faint static frame. --dirty-fibers grows
  outer needles first, then the trunk, and rotates φ on the chart.

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
    let mut flux = Flux::Elliptic;
    let mut lambda: Option<f32> = None;
    let mut mosaic: Option<u32> = None;

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
                    eprintln!("unknown scene {v} (hopf|sculpture|gradient|ngsm|hold|loom|braid|fabric)");
                    2
                })?;
            }
            "--grid" => grid = Some(take_u32("--grid", &mut it)?),
            "--cell-extent" => cell_extent = Some(take_f32("--cell-extent", &mut it)?),
            "--orb-scale" => orb_scale = Some(take_f32("--orb-scale", &mut it)?),
            "--ring-radius" => ring_radius = Some(take_f32("--ring-radius", &mut it)?),
            "--ring-tube" => ring_tube = Some(take_f32("--ring-tube", &mut it)?),
            "--lambda" => lambda = Some(take_f32("--lambda", &mut it)?),
            "--mosaic" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --mosaic");
                    return Err(2);
                };
                mosaic = Some(parse_mosaic(&v)?);
            }
            "--flux" => {
                let Some(v) = it.next() else {
                    eprintln!("missing value for --flux");
                    return Err(2);
                };
                flux = Flux::parse(&v).ok_or_else(|| {
                    eprintln!("unknown --flux {v} (elliptic|hyperbolic)");
                    2
                })?;
            }
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
    let grid_opt = grid;
    let mut grid = grid.unwrap_or(g_grid);
    let cell_extent = cell_extent.unwrap_or(0.22);
    let orb_scale = orb_scale.unwrap_or(0.055);
    let ring_radius = ring_radius.unwrap_or(0.09);
    let ring_tube = ring_tube.unwrap_or(0.004);

    let fibers_opt = fibers;
    let mut fibers = fibers.unwrap_or(seed.fibers);
    let mut fiber_samples = fiber_samples.unwrap_or(seed.samples);
    let particles_opt = particles;
    let mut particles = particles.unwrap_or(seed.particles);
    let orbs_opt = orbs;
    let mut orbs = orbs.unwrap_or(seed.orbs);
    let width = width.unwrap_or(seed.width).max(1);
    let height = height.unwrap_or(seed.height).max(1);
    let tube_radius_opt = tube_radius;
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

    if scene == Scene::Hold {
        fiber_samples = if fiber_samples == seed.samples {
            g_samples
        } else {
            fiber_samples
        };
        fibers = fibers_opt.unwrap_or(12).clamp(8, 16);
        orbs = 0;
        dirty_particles = false;
        dirty_fibers = false;
        dirty_rings = false;
        glow = glow_flag.unwrap_or(g_glow);
        tube_radius = tube_radius_opt.unwrap_or(0.012);
        particles = particles_opt.unwrap_or(16_384);
    }

    if scene == Scene::Loom {
        let (n_grid, n_motes, tube) = match preset {
            Preset::Smoke => (16, 4_096, 0.016),
            Preset::RingQga => (16, 16_384, 0.014),
            Preset::FourK90 => (16, 32_768, 0.014),
            Preset::Soak => (32, 65_536, 0.012),
        };
        fiber_samples = if fiber_samples == seed.samples {
            g_samples
        } else {
            fiber_samples
        };
        grid = grid_opt.unwrap_or(n_grid);
        fibers = fibers_opt.unwrap_or(grid);
        orbs = orbs_opt.unwrap_or(grid.saturating_mul(grid).min(ORB_CAP));
        dirty_rings = false;
        glow = glow_flag.unwrap_or(g_glow);
        tube_radius = tube_radius_opt.unwrap_or(tube);
        particles = particles_opt.unwrap_or(n_motes);
        dirty_particles = seed.dirty_particles;
        dirty_fibers = seed.dirty_fibers;
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
        if scene != Scene::Loom {
            dirty_rings = true;
            let side = (grid.saturating_mul(8)).clamp(32, 256);
            particles = particles_opt.unwrap_or(side.saturating_mul(side));
        }
    } else if scene == Scene::Gradient && dirty_particles && particles == 0 {
        particles = 4 * grid.saturating_mul(grid);
    }

    // Loom orbits so the nested tori read as a 3D object. Hopf records crane.
    // Gradient stays locked (sheet silhouette).
    let cinematic = scene == Scene::Loom || (record.is_some() && scene == Scene::Hopf);
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

    if matches!(scene, Scene::Gradient | Scene::Hold) && grid > GRID_CAP {
        eprintln!("grid {grid} exceeds cap {GRID_CAP}");
        return Err(2);
    }
    if scene == Scene::Loom && (grid > GRID_CAP || grid < 2) {
        eprintln!("loom grid {grid} out of range 2…{GRID_CAP}");
        return Err(2);
    }
    let mosaic = mosaic.unwrap_or(1).clamp(1, 4);
    let lambda = lambda.unwrap_or(0.15).clamp(0.0, 1.0);
    let live_fibers = if scene == Scene::Loom {
        grid.saturating_mul(grid).saturating_mul(mosaic.saturating_mul(mosaic))
    } else {
        fibers
    };
    if live_fibers > FIBER_CAP {
        eprintln!("live fibers {live_fibers} exceeds cap {FIBER_CAP} (warp×layers for loom)");
        return Err(2);
    }
    if particles > PARTICLE_CAP {
        eprintln!(
            "particles {particles} exceeds cap {PARTICLE_CAP} (~256 MiB VB at 32 B/record); refusing before wgpu OOM"
        );
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
        * (u64::from(particles) + u64::from(live_fibers) * u64::from(fiber_samples) + u64::from(orbs));
    if staging > STAGING_BUDGET {
        eprintln!(
            "CPU staging budget {staging} B exceeds 1 GiB (32 × (particles + fibers × samples + orbs)); refusing before wgpu OOM"
        );
        return Err(2);
    }

    let json = json.unwrap_or_else(|| {
        let name = if matches!(scene, Scene::Gradient | Scene::Hold | Scene::Loom) {
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
        flux,
        lambda,
        mosaic,
    })
}

fn parse_mosaic(s: &str) -> Result<u32, i32> {
    match s {
        "1" | "1x1" => Ok(1),
        "2" | "2x2" => Ok(2),
        "3" | "3x3" => Ok(3),
        "4" | "4x4" => Ok(4),
        _ => {
            eprintln!("--mosaic wants 1|2|2x2|3|4, got {s}");
            Err(2)
        }
    }
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

    #[test]
    fn hold_4090_is_two_clock() {
        let a = parse_from([
            "--headless",
            "--scene",
            "hold",
            "--preset",
            "4090",
            "--frames",
            "300",
            "--no-capture",
        ])
        .unwrap();
        assert_eq!(a.scene, Scene::Hold);
        assert_eq!(a.grid, 32);
        assert_eq!(a.fibers, 12);
        assert_eq!(a.fiber_samples, 64);
        assert_eq!(a.particles, 16_384);
        assert_eq!(a.orbs, 0);
        assert_eq!(a.frames, 300);
        assert!(!a.dirty_particles && !a.dirty_fibers && !a.dirty_rings);
        assert!(a.glow);
        assert_eq!(a.capture, Capture::None);
        assert!((a.tube_radius - 0.012).abs() < 1e-6);
    }

    #[test]
    fn hold_fiber_override_stays_in_live_slot() {
        let a = parse_from(["--scene", "hold", "--fibers", "8", "--particles", "256"]).unwrap();
        assert_eq!(a.fibers, 8);
        assert_eq!(a.particles, 256);
        assert_eq!(a.orbs, 0);
    }

    #[test]
    fn hold_record_matches_makefile_scheme() {
        let a = parse_from([
            "--scene",
            "hold",
            "--preset",
            "4090",
            "--grid",
            "64",
            "--width",
            "2560",
            "--height",
            "1440",
            "--frames",
            "600",
            "--record",
            "benchmarks/results/qga-gpu-bench-hold-64.mp4",
        ])
        .unwrap();
        assert_eq!(a.scene, Scene::Hold);
        assert!(a.headless);
        assert!(!a.cinematic);
        assert_eq!(a.capture, Capture::All);
        assert_eq!(a.grid, 64);
        assert_eq!(a.fibers, 12);
        assert_eq!(a.particles, 16_384);
        assert_eq!(a.frames, 600);
        assert_eq!(a.width, 2560);
        assert_eq!(a.height, 1440);
        assert_eq!(
            a.record.as_deref(),
            Some(std::path::Path::new(
                "benchmarks/results/qga-gpu-bench-hold-64.mp4"
            ))
        );
    }

    #[test]
    fn loom_4090_is_elliptic_16() {
        let a = parse_from(["--headless", "--scene", "loom", "--preset", "4090"]).unwrap();
        assert_eq!(a.scene, Scene::Loom);
        assert_eq!(a.grid, 16);
        assert_eq!(a.fibers, 16);
        assert_eq!(a.orbs, 256);
        assert_eq!(a.fiber_samples, 64);
        assert_eq!(a.particles, 32_768);
        assert_eq!(a.flux, Flux::Elliptic);
        assert!((a.lambda - 0.15).abs() < 1e-6);
        assert_eq!(a.mosaic, 1);
        assert!(a.dirty_particles && a.dirty_fibers);
        assert!(a.cinematic);
        assert!(a.glow);
        assert_eq!(a.frames, 600);
        assert_eq!(a.width, 1920);
        assert_eq!(a.height, 1080);
    }

    #[test]
    fn loom_aliases_and_overrides() {
        let a = parse_from([
            "--scene",
            "braid",
            "--preset",
            "smoke",
            "--fibers",
            "8",
            "--grid",
            "3",
            "--particles",
            "64",
        ])
        .unwrap();
        assert_eq!(a.scene, Scene::Loom);
        assert_eq!(a.fibers, 8);
        assert_eq!(a.grid, 3);
        assert_eq!(a.particles, 64);
        assert_eq!(a.orbs, 9);
        assert!(!a.dirty_fibers);
        assert!(a.dirty_particles);
        assert_eq!(a.flux, Flux::Elliptic);
    }

    #[test]
    fn fabric_alias_is_loom() {
        let a = parse_from(["--scene", "fabric", "--preset", "smoke"]).unwrap();
        assert_eq!(a.scene, Scene::Loom);
        assert_eq!(a.fibers, 16);
        assert_eq!(a.grid, 16);
        assert_eq!(a.particles, 4_096);
    }

    #[test]
    fn loom_rejects_too_many_live_tubes() {
        assert_eq!(
            parse_from(["--scene", "loom", "--grid", "96", "--mosaic", "2"]).unwrap_err(),
            2
        );
    }

    #[test]
    fn loom_record_orbits() {
        let a = parse_from([
            "--scene",
            "loom",
            "--preset",
            "smoke",
            "--record",
            "out.mp4",
        ])
        .unwrap();
        assert!(a.headless);
        assert!(a.cinematic);
        assert_eq!(a.capture, Capture::All);
    }

    #[test]
    fn loom_flux_lambda_mosaic() {
        let a = parse_from([
            "--scene",
            "loom",
            "--flux",
            "hyperbolic",
            "--lambda",
            "0.8",
            "--mosaic",
            "2x2",
        ])
        .unwrap();
        assert_eq!(a.flux, Flux::Hyperbolic);
        assert!((a.lambda - 0.8).abs() < 1e-6);
        assert_eq!(a.mosaic, 2);
    }

    #[test]
    fn loom_windowed_record_flags() {
        let a = parse_from([
            "--scene",
            "loom",
            "--preset",
            "4090",
            "--dirty-particles",
            "--dirty-fibers",
            "--frames",
            "900",
            "--record",
            "benchmarks/results/qga-gpu-bench-loom-4090.mp4",
        ])
        .unwrap();
        assert_eq!(a.scene, Scene::Loom);
        assert!(a.headless);
        assert_eq!(a.frames, 900);
        assert_eq!(a.width, 1920);
        assert_eq!(a.height, 1080);
        assert_eq!(a.fibers, 16);
        assert_eq!(a.grid, 16);
        assert!(a.cinematic);
        assert!(a.dirty_particles && a.dirty_fibers);
        assert_eq!(
            a.record.as_deref(),
            Some(std::path::Path::new(
                "benchmarks/results/qga-gpu-bench-loom-4090.mp4"
            ))
        );
    }
}
