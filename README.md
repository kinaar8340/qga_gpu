# qga-gpu

wgpu/Vulkan renderer. **This crate owns the frame.** Geometry meaning lives in
[`qga`](https://github.com/kinaar8340/qga) / `qga-math`.

Renderer claims are **Software fact**.

Workspace:

```
crates/qga-gpu         library (WGSL in crates/qga-gpu/src/shaders/)
crates/qga-gpu-demo    1 sphere + 2 cones + separator torus + 4k particles
```

Extracted from `qga_engine/crates/qga-gpu` and the inner_cone upload path.
`inner_cone` and `qga_engine` still path-depend on the engine copy. They will
switch later; see [MIGRATION.md](MIGRATION.md). This extract does **not** open
a PR into those repos.

## Hardware target (this machine)

| | |
|---|---|
| GPU | NVIDIA GeForce RTX 4090, 24 GiB, driver 580, Vulkan 1.4 |
| CPU | AMD Ryzen 9 3900X, 12 cores / 24 threads |
| OS | Ubuntu, GNOME, **Wayland** |

Vulkan through `wgpu` only. No OpenGL. CUDA is out of scope for v0.

## Build

```bash
make check          # cargo check --workspace && cargo test -p qga-gpu
make demo           # windowed
make headless       # 8 offscreen frames; requires static_uploads == 1
make ring           # 300 dirty frames, headless (in-flight map_async)
make ring-windowed  # 300 dirty frames, windowed FIFO/mailbox, then exit
```

```bash
cargo check --workspace
cargo test -p qga-gpu
cargo run -p qga-gpu-demo --release
cargo run -p qga-gpu-demo --release -- --headless --frames 8
cargo run -p qga-gpu-demo --release -- --dirty-particles
cargo run -p qga-gpu-demo --release -- --dirty-particles --frames 300
```

Demo window: LMB orbit, wheel zoom, `C` cinematic, `G` glow, Space pause, Esc quit.
`--dirty-particles` nudges the 4k field every frame. `--frames N` exits after N presents (windowed or headless).

The default demo has **no path to `qga_engine`**. Optional `--features qga-math`
on `qga-gpu` adds `From<&qga_math::Fiber>` and needs a local engine checkout.

## How consumers will depend on it later

| Consumer | Later dep | Not in this extract |
|----------|-----------|---------------------|
| `inner_cone` | `qga-gpu = { path = "../qga_gpu/crates/qga-gpu" }` | do not edit inner_cone here |
| `qga_engine` | git or path dep; drop in-tree `crates/qga-gpu` | do not PR into qga_engine here |

## Dirty-flag / instance / persistent-particle policy

Software fact. This is the inner_cone upload contract, implemented in the
renderer so callers do not rebuild GPU topology every frame.

**Dirty flag (static topology).** `retain_meshes` / `retain_static_fibers`
hash mesh kind + lod + tube radius + centerline bytes. Identical re-upload is
a no-op. `UploadStats.static_uploads` counts real GPU writes; the headless
demo requires `static_uploads == 1` across 8 frames. `static_skipped` counts
the hits. `mark_static_dirty` forces the next retain. Live params
(`aperture`, `height_scale`, `zener`, `time`) are **frame uniforms** — they do
not retessellate cones/spheres/tori.

**Instance.** Repeated geodesic orbs use `draw_geodesic_orb(transform, color, lod)`
against a unit sphere retained at `Renderer::new`. One mesh, N instances
(`GpuOrbInstance`, 32 bytes: offset, scale, color, lod).

**Persistent particles.** `write_particles` keeps a GPU vertex buffer and a
3-slot `MAP_WRITE` staging ring (128 KiB slots, grow ×2). Hash skip
(`particle_skipped`) is the cheapest path. When bytes change: pick **any**
ready slot, memcpy, queue a `copy_buffer_to_buffer` on the next `render`.
`Queue::write_buffer` only if **zero** slots are ready (`particle_fallbacks`);
outstanding `pending` copies are never dropped. `particle_grows` is counted
apart from `fiber_reallocs`. Reclaim maps the slot cap (tight pow2, not a fat
arena). Profile `ring_copies` vs `particle_fallbacks` on this 4090 before
meshlets. Dirty-every-frame smoke:

```bash
make ring            # 300 frames headless; capture first+last only
make ring-windowed   # 300 presents; mailbox/FIFO in-flight
cargo run -p qga-gpu-demo --release -- --headless --frames 300 --dirty-particles
cargo run -p qga-gpu-demo --release -- --dirty-particles --frames 300
```

Acceptance (dirty): `ring_copies + particle_fallbacks >= frames`,
`particle_skipped == 0`, `static_uploads == 1`. Fallbacks are **allowed
and counted**. This 4090: headless 300 first+last-capture → 1 fallback
(CPU ahead of `map_async`); windowed FIFO 300 → 0 fallbacks (present
paces reclaim). That is DMA into DEVICE_LOCAL, not a broken ring. Do not
add a 4th slot unless windowed fallbacks exceed ~1%. Do not persist-map the particle VB through wgpu. Do not
`poll(Wait)` on write maps (`map_async` latency is 1–3 GPU frames;
Wait serializes them). Do not put particles on `StagingBelt`.
A 16 KiB belt is only for a future HUD/hub storm of tens of small
copies per present.
Frame loop is one encoder + one `submit` (pending-writes then the pass).
Empty `submit([])` is for asset load, not the present loop.

**Live vs static fibers.** Static separator / torus: `retain_static_fibers`.
Live harmonics: `write_live_fibers` (same hash+radius no-op). Tubes are
centerlines + shader extrusion, not CPU ribbons.

## Public surface (v0)

```rust
Renderer::new
Renderer::retain_static_fibers
Renderer::write_live_fibers      // no-op if topology hash + tube_radius unchanged
Renderer::retain_meshes          // sphere/cone/torus tessellated once
Renderer::draw_geodesic_orb(transform, color, lod)
Renderer::upload_hubs
Renderer::write_particles        // staging ring; skip if bytes unchanged
Renderer::write_hud
Renderer::render(gpu, cam, vis, t, capture)
```

## Features (`qga-gpu`)

| Feature | Default | What it does |
|---------|---------|----------------|
| `winit` | yes | `GpuContext::init_windowed` + pollster |
| `headless` | no | pollster; `init_headless` |
| `capture` | no | BGRA readback |
| `glow` | no | 9-tap bloom + Reinhard |
| `qga-math` | no | `From` impls for `qga_math::Fiber` |

## Records

Software fact:

- Frame uniforms are **256 bytes**.
- Particle, fiber-point, hub, and orb-instance records are **32 bytes**.
- Buffer copies/maps: 4 B / 8 B. Texture `bytes_per_row`: **256 B** (capture).
  That pitch is WebGPU-on-Vulkan, not the 4090’s optimal tiling.
- wgpu validation is an enum tree (`BufferAccessError`, `TransferError`), not
  `VkResult`. `layout.rs` keeps illegal numbers off the upload path.
  `particle_fallbacks` is not a validation error.
- Tubes are centerlines + shader extrusion, not CPU ribbons.

## What this crate is not

- Realm terrain, cosmos n-body, OAM PDE, or reveal Lorenz scenes.
- A default `qga-math` / `qga-sim` dependency.
- A theorem about the Z-map or 350/π.

See [DESIGN.md](DESIGN.md) and [MIGRATION.md](MIGRATION.md).
