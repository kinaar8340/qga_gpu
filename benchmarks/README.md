# qga-gpu benchmarks

Software fact of `crates/qga-gpu-bench`. Scripts and result logs live here.
The thing that hammers the 4090 is the workspace binary, not this folder.

Hopf-fiber generation is **Model** (`glam::Quat` orbits in `hopf.rs`).
`--scene gradient` is a **Model** lattice (instanced orbs + live ring
centerlines). `--scene hold` is a **Model** frozen lattice: static
topology once, uniforms every frame, live harmonics + motes on a 30-frame
pulse. Camera sits in the sheet. `--scene loom` is a **Model** inverse-Hopf
from a Cartesian chart (latitudes → nested tori). Renderer claims stay
**Software fact**. This does **not** prove inner_cone mosaic/hull or
qga-app cosmos.

Visual theme for `--scene gradient`: “gradient / structure” by
Toshiyuki Nagashima (@ngsm)
https://x.com/ngsm/status/2094596901345825098
Rainbow ocean / local ring tilts are a **Model** of Stokes-skyrmion
textures (Chen et al., Nanophotonics 2026, vectorial diffractive
metasurfaces) and the gauged Hopf-lattice analog (Kinder,
arXiv:2607.16520). Inspiration only. Not a port of p5.js, not a copy of
those simulations, not a qga-app OAM/reveal scene.

## Run

From the repo root. The public demo is `make demo` (65k ocean, until Esc).
This folder is the bench harness, not that path.

```bash
make demo-headless   # 30 frames, 65k ocean, asserts UploadStats
make bench-smoke
make bench
./benchmarks/run.sh
```

`run.sh` runs `smoke` then `4090 --no-capture` and writes
`benchmarks/results/<gitsha>-<preset>.json`.

`make bench-record` encodes the 4090 hopf scene to
`benchmarks/results/qga-gpu-bench-4090.mp4` (every-frame capture Wait;
not a ring proof). `make bench-gradient-record` is the fluid gradient
ocean (`--scene gradient --fluid --record PATH.mp4`).
`make bench-hold-record` is the held lattice, in-sheet, HUD off
(`--scene hold --grid 64 --record PATH.mp4`). Capture Wait; not a skip
proof — `make bench-hold` is the counter check. This 4090 (300 headless):
`static_uploads=1`, `live_fiber_writes=10`, `particle_skipped=291`,
`ring_copies=10`, `particle_fallbacks=0`.

```bash
make bench-gradient           # 32×32 lattice, dirty rings, 600 frames
make bench-gradient-windowed
make bench-hold               # two-clock skip, 300 headless, no capture
make bench-hold-record        # --grid 64 1440p → mp4 (capture Wait; not a skip proof)
cargo run -p qga-gpu-bench --release -- --headless --scene gradient --preset smoke
```

Presets: `smoke`, `ring-qga`, `4090`, `soak`. See the crate `--help`.

## Results

- Commit `schema.json` and `sample-4090.json`.
- PNG/BMP captures and large bins are gitignored.
- VRAM estimate in the JSON is from record sizes and targets, not HAL.
