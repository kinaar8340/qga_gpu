# qga-gpu — design

A standalone wgpu/Vulkan renderer. **This crate owns the frame.** Geometry
meaning lives in qga / qga-math. Renderer claims are **Software fact**.

This is not the QGA engine, not a fantasy realm, and not a solar-nebula sim.
Those scenes stay in `qga_engine` / `inner_cone`. This crate is the upload path
and the swapchain.

## Crate map

```
qga_gpu/
├── crates/qga-gpu/                 # library
│   ├── src/{context,camera,renderer,types,mesh,hud,profile}.rs
│   └── src/shaders/{fiber,particle,hub,face,line,hud,blit,post}.wgsl
└── crates/qga-gpu-demo/            # window + headless smoke
```

| Crate | Role |
|-------|------|
| `qga-gpu` | Vulkan device, pipelines, resident buffers, upload API |
| `qga-gpu-demo` | 1 sphere, 2 cones, separator torus, 4k particles |

WGSL lives in-tree under `crates/qga-gpu/src/shaders/`. No runtime Python.

## Sources of truth

| Concern | Upstream | Here |
|---------|----------|------|
| Quaternions, Hopf, Hurwitz, topographs | `qga` / `qga-math` | not imported (optional `qga-math` feature) |
| Tube look, bloom, void | `flux_hopf_explorer` via engine WGSL | `src/shaders` |
| Static lattice once, live harmonics per frame | `inner_cone` engine tick | `Renderer` upload API |
| Realm / cosmos / OAM / reveal | `qga-app` | **not copied** |

Python is an authoring language in the ecosystem. The frame loop is Rust +
Vulkan. Do not `sys.path` into sibling repos.

## Why Vulkan / wgpu

This box is Wayland + NVIDIA 580 + Vulkan 1.4 ICD. `wgpu` selects the Vulkan
backend, talks Wayland WSI through `winit`, and keeps raster on the 4090
queue. CUDA is the wrong home for the swapchain.

FIFO present by default. Mailbox is detected (`present_mailbox`) but not used:
driver 580 has presented empty frames with mailbox. Software fact.

## Runtime

```
qga-gpu-demo  |  inner_cone / qga-app (later)
    │
    └── qga-gpu
            ├── GpuContext  Vulkan device, optional surface, depth
            ├── Camera      orbit / fly, Y-up look_at_rh
            └── Renderer
                    ├── retain_meshes: sphere / cone / torus once
                    ├── static fiber pass vs live harmonic pass
                    ├── geodesic orb instances
                    └── post: blit, or glow (threshold + 9-tap + Reinhard)
```

Draw what was uploaded. Live params (aperture, height_scale, zener, time) are
frame uniforms.

Frame uniforms are 256-byte aligned. Particle, fiber, hub, and orb-instance
records are 32 bytes.

## Buffer policy

| Buffer | Lifetime | CPU write |
|--------|----------|-----------|
| Static fibers (centerline storage) | Resident. Hash(kind, points, tube_radius). | `retain_static_fibers` / torus in `retain_meshes`. No-op if hash matches. Counted in `static_uploads`. |
| Live fibers | Resident, grow ×2. | `write_live_fibers` only if hash or radius changed. |
| Faces / lines / hubs / HUD | Resident, grow ×2. | On retain / explicit upload. |
| Geodesic orb mesh | Tessellated once in `Renderer::new`. | Instances via `draw_geodesic_orb`. |
| Particles | Persistent GPU VB + 3×128 KiB `MAP_WRITE` ring (grow ×2). | Skip if bytes unchanged (`particle_skipped`). Any ready slot; `write_buffer` only if none ready (`particle_fallbacks`). Never drop `pending`. Grows counted in `particle_grows`, not `fiber_reallocs`. |

Do not add meshlets until `write_buffer_calls` vs `ring_copies` is profiled on
this 4090. Tubes are shader-extruded from 32-byte `GpuFiberPoint` records.

`UploadStats.static_uploads` is the acceptance counter: headless 8-frame run
must print `static_uploads=1`.

## Upload contract (from inner_cone)

1. Tessellate static topology once (parametric sphere / cone / torus).
2. `retain_static_fibers` / `retain_meshes` / `upload_hubs` when the lattice
   key changes. Identical re-upload is skipped (`static_skipped`).
3. Each frame: `write_live_fibers` (live harmonics), `write_particles`,
   `write_hud`, `render`.
4. Tubes: `GpuFiberPoint` centerlines, camera-facing extrusion in `fiber.wgsl`.
   No CPU ribbon rebuild on camera/time.
5. Particles: 3-slot `MAP_WRITE` ring. Prefer any ready slot. Fallback
   `write_buffer` only if zero slots ready. Do not drop `pending`.
6. Rebuild only aperture/height/zener-driven topology (caller sets
   `mark_static_dirty`). Camera and particles do not realloc fibers.

## Key decisions

1. **No default `qga-math` / `qga-sim` dependency.** GPU records only. Callers
   convert. Optional `--features qga-math` for `From<&qga_math::Fiber>`.
2. **No scene-kind draw gate.** `render` draws whatever was uploaded.
   inner_cone today passes `Reveal` so hubs/HUD draw; that leak is not reproduced.
3. **Frame uniforms are 256 bytes**, not the engine's 272 (cosmos `palette` pad).
4. **Shader tube extrusion** instead of CPU `RibbonVert` (48 B).
5. **Particle staging ring** instead of realloc + `write_buffer` on every path.
6. **Headless actually renders** to an offscreen color target. The engine
   returned `Ok(None)` with no surface.
7. **Parametric mesh helpers** are tessellation (Software fact), not observer
   / Zener / flux meaning.
8. **HUD primitives only.** Scene overlays stay in the caller.

## Features

`default = ["winit"]`. Optional: `headless`, `capture`, `glow`, `qga-math`.

## Non-goals (v0)

- Editing or opening a PR into `qga_engine` or `inner_cone` in this extract.
- Porting `qga-app` scenes (lab / realm / cosmos / oam / reveal).
- Porting `qga-sim` worldgen, n-body ICs, or OAM twist PDE.
- CUDA, OpenGL, WebGPU-in-browser.
- N-body compute, meshlets, DLSS/FSR.
- Claiming the Z-map or 350/π as theorems inside the renderer.
- Vendoring all of `qga-math`.

Later consumer switch: [MIGRATION.md](MIGRATION.md).

## Claim labels

Theorem / Model / Software fact / Open. Everything in this crate is
**Software fact** unless a comment says otherwise.
