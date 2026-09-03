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

Tubes are shader-extruded from 32-byte `GpuFiberPoint` records. Do not add
meshlets because of particle fallbacks.

## Mapping on a discrete 4090 (Software fact)

The particle VB is `VERTEX | COPY_DST` (DEVICE_LOCAL). The ring is three
128 KiB `MAP_WRITE | COPY_SRC` buffers (HOST_VISIBLE + HOST_COHERENT staging).
CPU writes the map forward-only, unmaps, GPU `copy_buffer_to_buffer` into VRAM.
A slot is either mapped on the CPU **or** in a submitted copy, never both.

wgpu will not give a persistent CPU pointer to that VERTEX buffer without
`MAPPABLE_PRIMARY_BUFFERS`. Do not put the 4k field in BAR/ReBAR to skip the
copy: vertex fetch from write-combine mapped VRAM is the wrong trade. Do not
emulate `vkMapMemory` and leave it mapped — wgpu forbids using a `MAP_WRITE`
buffer in a copy while it stays mapped. Compute-update of particles is a v0
non-goal.

`Queue::write_buffer` stays the fallback when zero slots are ready (native
wgpu often allocates a short-lived staging chunk per call). Measured on this
box, dirty 4k particles:

| Harness | `ring_copies` | `particle_fallbacks` |
|---------|---------------|----------------------|
| Headless 300, capture first+last | 300 | 1 |
| Windowed FIFO 300 | 301 | 0 |

Full capture `Wait` every frame idles the GPU before the next write → 0
fallbacks (hides pressure). First+last capture lets the CPU run ahead of
`map_async` → one frame with no ready slot. FIFO present gates the CPU →
reclaim always wins. **One fallback in 300 is healthy:** 3 in-flight is tight
under no-vsync, not a reason to add a 4th slot (only if windowed fallbacks
exceed ~1%). Do not `poll(Wait)` in the particle ring. Grow Waits, then
rebuilds all three slots; keep `particle_grows == 0` at 4k.

Acceptance: dirty writes land as
`ring_copies + particle_fallbacks >= frames`. Fallbacks are allowed and
counted. `static_uploads == 1`. `particle_skipped == 0` when dirty.
DMA into DEVICE_LOCAL; map only HOST_VISIBLE staging; never wait the
swapchain on capture.

`UploadStats.static_uploads` is also the static-topology counter: default
headless 8-frame run must print `static_uploads=1`.

### `map_async` cost (Software fact)

`map_async` is cheap to **call** and expensive to **wait for**. The callback
cannot fire until every submitted use of that buffer is done **and** something
polls (`queue.submit`, `device.poll`, `instance.poll_all`). Native wgpu is
callback-based; a `Future` wrapper only hides a `poll`. Headless `Poll` in a
tight loop historically did **not** retire maps; `Wait` did. Windowed present
+ `submit` usually drains callbacks.

```
unmap slot
encoder.copy  staging → VB
submit
map_async(Write, 0..cap)   // AtomicBool in cb
write_particles: poll(Poll); test ready[]
```

That Poll is optional bookkeeping. It does **not** guarantee the previous
frame’s map finished. Fallback = all three still waiting.

| Piece | Cost |
|-------|------|
| `map_async` call | CPU µs |
| Wait for last copy of that slot | **1–3 frames of GPU work** (the real latency) |
| `vkMapMemory` / invalidate at 128 KiB | small; scales with **mapped range** |
| memcpy 128 KiB | bandwidth; CPU `Vec` already exists |
| `unmap` | flush if non-coherent; NVIDIA staging usually coherent |
| `device.poll(Wait)` | **stalls the GPU timeline to idle** — capture only |
| Mapping 2 MiB / 128 MB “just in case” | fat-map tax (Chrome 4090: 1.3–2× vs used range) |

Native Vulkan fat-map tax is smaller than in-browser, but `slice(..)` still
marks the whole cap live. Payload-sized slots keep that honest.

The 1/300 first+last-capture fallback is **map-async latency**, not memcpy.
`poll(Wait)` after every `map_async` would zero fallbacks and destroy overlap
(serial-await). Hash skip still beats any map.

Poll policy:

- Frame loop: `submit` only. Windowed event loop already pumps wgpu.
- `write_particles`: `poll(Poll)` optional, for fresher `ready[]` same call.
- Capture read: `map_async(Read)` + `Wait` **only** on grabbed frames.
- Grow: `Wait` before destroying mapped slots.

Do not add async/await wrappers, extra poll threads, or
`on_submitted_work_done` per particle write. `particle_fallbacks` under dirty
windowed vs first+last headless **is** the map-async overhead meter. 0–1 per
300 frames at 128 KiB is the operating point. Do not map the VERTEX buffer.

## StagingBelt (Software fact — do not add for particles)

`wgpu::util::StagingBelt` is wgpu’s arena over the same MAP_WRITE + COPY_SRC
ring this crate built by hand. It wins when **one submit contains many small
copies**. It does not beat a dedicated 128 KiB particle slot.

Belt chunks: `active` (mapped, bump this frame) → `finish` unmaps → submit →
`recall` `map_async` → `free`. `Queue::write_buffer` still often allocates a
short-lived chunk **per call**. The belt amortizes map/unmap across N writes
on the **same** encoder. Chunk size must be larger than the biggest write and
about ¼–1× bytes per `finish()`. A write that will not fit allocates a
one-off chunk (`size.max(chunk_size)`).

| Upload | Size / cadence | Belt? |
|--------|----------------|-------|
| Particles (dirty) | one 128 KiB blit / frame | **No.** 3-slot ring is the belt with `chunk_size = payload`. |
| Frame uniforms | 256 B / frame | **No.** One `write_buffer` cheaper than finish/recall. |
| Static fibers / meshes | once (`static_uploads=1`) | **No.** |
| Live fibers | rare (hash skip) | Later, only if many short centerlines per tick. |
| Hubs / HUD / geo instances | packed `Vec` + one `write_grow` | **Yes, if** tens of copies per present. Not today. |

Dirty 4090 runs: `ring_copies≈frames`, `particle_fallbacks` 0–1,
`write_buffer≈1/frame` (uniforms). A particle belt would `map_async` the
**whole** chunk — the fat-map trap if `chunk_size` ≫ 128 KiB.

| | 3-slot particle ring | StagingBelt |
|--|----------------------|-------------|
| Allocation | fixed 3 × payload | bump inside `chunk_size`, else new buffer |
| Close | unmap one slot | unmap **every** active chunk, even 1% full |
| Reclaim | `map_async` that slot | `map_async(slice(..))` **whole chunk** |
| Fallback | `write_buffer` if 0 ready | create another chunk (silent growth) |
| Best write | one 128 KiB field | dozens of 16–256 B scraps |

“Fast” belt: one `finish()` per submit, 2–3 chunks in flight, non-blocking
`recall`, DEST DEVICE_LOCAL, **many** writes sharing one mapped chunk. One
belt write per submit is `write_buffer` with extra ceremony. `chunk_size` =
256 KiB “so everything fits” is the unused-range tax. Do not `submit([])`
every N belt writes unless streaming assets. Do not map, write 256 B,
`finish`, repeat 300 times.

If a belt is added: `finish` → `submit` → `recall` (or
`finish_and_recall_on_submit` then submit **that** encoder before the next
allocate). Count `belt_chunks_created`; it must plateau by frame ~3.

If inner_cone later storms HUD/hubs (`write_buffer_calls` tens per present):
add a **16–64 KiB** belt on that path only, dest still DEVICE_LOCAL
`VERTEX|COPY_DST`. Count `belt_writes` / `belt_chunks_created` separately from
`ring_copies` / `particle_fallbacks`. Do not enable
`MAPPABLE_PRIMARY_BUFFERS`. Do not `Wait` on `recall`. Cap ~3 chunks in
flight, same as 3 slots. One encoder per frame.

Belt `allocate`: (1) first **active** chunk with room after
`align_to(offset, max(user_align, MAP_ALIGNMENT))`; (2) drain `map_async`
into **free**; (3) first free chunk that fits; (4) else
`create_buffer(size.max(chunk_size), MAP_WRITE|COPY_SRC, mapped_at_creation)`.
`finish` unmaps every active chunk **even if 1% full**. `recall` remaps
**full range**. Offset resets only when a chunk returns to free.

| Symptom | Cause |
|---------|--------|
| Chunk count grows every frame | `recall` after a submit that never happens, or CPU ahead of `map_async` (same as `particle_fallbacks`) |
| Many chunks, barely used | `chunk_size` ≫ per-`finish` bytes, or one write > remaining space |
| One huge chunk per write | single write > `chunk_size` → one-off `size`, never reuses well |
| Map cost scales with cap not payload | `recall` maps the whole chunk |

Steady state after warmup: **created ≈ 2–3**, `free+active+closed` bounded,
`bytes_written / capacity` not ≪ 1. On `make ring-windowed`, if
`belt_chunks_created` (when a belt exists) climbs past ~4, `recall` is late
or `chunk_size` is smaller than the largest write.

| Per-submit payload | `chunk_size` | After 300 frames |
|--------------------|--------------|------------------|
| 256 B uniforms only | 16 KiB | 2–3 resident, ~1.5% full — waste, still cheap |
| uniforms + HUD + hubs (~2–8 KiB) | 16 KiB | good |
| + one 128 KiB particle blit | ≥128 KiB | particles steal the belt; the ring is better |
| 128 KiB particles + 256 B | 256 KiB | fat map; worse than a tight slot |

## Queue submit (Software fact — pattern A)

Two streams meet at `submit`: wgpu **pending-writes** (`Queue::write_buffer`
memcpy into impl staging, GPU copy flushed **at the start of the next
submit**), then your `CommandBuffer`s. `submit([])` flushes pending-writes.
Staging is released after **that** submit retires. Many `write_buffer`s with
no submit = many live impl stagings (CubeCL flushes every 64). This crate
does ~1 `write_buffer`/frame plus rare mesh — no empty-submit in the present
loop.

```
write_particles            // map slot or fallback write_buffer (pending-writes)
write_buffer(uniforms)     // pending-writes
encoder:
  copy staging → particle VB
  render / blit / optional capture
submit([encoder])          // pending-writes first, then encoder
map_async particle slot
present
```

That is **one encoder, one submit**. Particle copies live on the encoder
(`ring_copies`). Uniforms live on pending-writes (`write_buffer_calls`). A
belt would move small copies onto the encoder and add `finish`/`recall`; it
would not change submit count.

Do **not**: upload-then-draw double `submit` (128 KiB + 256 B is not a fat
stream); `submit([])` mid-present; submit while a buffer is mapped (ring
unmaps before `pending`; belt `finish` unmaps first; capture maps **after**
submit, first+last only); a transfer queue (wgpu v0 is one graphics queue —
overlap is CPU record vs GPU previous frame).

Keep DEST DEVICE_LOCAL. One graphics submit per present until
`write_buffer_calls` per frame is tens, or you stream megabytes before the
pass.

## Alignment and texture staging (Software fact)

Portable wgpu table (WebGPU + D3D12), not a 4090 quirk. Copy/map and bind
are different problems. Texture row pitch is **not** a buffer-copy rule.
`layout.rs` pins the subset this crate uses plus the full constant values.

| Symbol | Value | Must be multiple of |
|--------|-------|---------------------|
| `COPY_BUFFER_ALIGNMENT` | **4** | `copy_buffer_to_buffer` / `clear_buffer` offset **and size**; `mapped_at_creation` buffer size |
| `MAP_ALIGNMENT` | **8** | `map_async` / `get_mapped_range` offset **and size** |
| `VERTEX_STRIDE_ALIGNMENT` | **4** | vertex offset and array stride |
| Immediate data (WebGPU) | **4** | `set_immediates` ranges (wgpu 24 has no named const) |
| Storage bind size | **4** | bound SSBO size (wgpu 24 has no named const) |
| Uniform bind offset | **256** | dynamic uniform offset; this crate’s UBO is **256 B** |
| `QUERY_RESOLVE_BUFFER_ALIGNMENT` | **256** | `resolve_query_set` dest offset |
| `QUERY_SIZE` | **8** | one query slot |
| `COPY_BYTES_PER_ROW_ALIGNMENT` | **256** | `copy_*_texture` **bytes_per_row only** — not `Queue::write_buffer` |

`Queue::write_buffer` dest offset + size need **copy 4**, not map 8, unless
you also map that dest. Belt: allocate size % 4; bump
`max(user, MAP_ALIGNMENT)`. `create_buffer_init` pads content length to ≥4
then copies the unpadded prefix.

A range that is **mapped and then copied** must be % 8 (implies % 4):
`0..8`, `0..32`, `8..40`, `n×32`. Illegal map: `0..4`, `0..12`. Illegal
copy: offset 2, size 2 or 6. `need = 0` is not a legal map — skip (empty
particles). Mappable staging **allocation** should be % 8 if you
`map_async(0..cap)`. Grow ×2 from 128 KiB stays legal.
`mapped_at_creation` alone only needs % 4.

`create_buffer_init` rounds content length up to a multiple of 4. StagingBelt
asserts copy size/offset % 4 == 0 and map align ≥ 8.

| Record | Size | Copy 4 | Map 8 | Bind |
|--------|------|--------|-------|------|
| `GpuParticle` / fiber / hub / orb | 32 | yes | yes | vertex/storage |
| `FiberMeta` | 16 | yes | yes | uniform |
| `FrameUniforms` | 256 | yes | yes | UBO 256 |
| `HudVert` | 24 | yes | yes | vertex |
| `FaceVert` | 48 | yes | yes | vertex |
| `LineVert` | 32 | yes | yes | vertex |
| `need = n×32` | | yes | yes | — |
| slot cap 128 KiB ×2 | | yes | yes | — |

This crate’s HUD vert is **24 B** (`[f32;2]` + `[f32;4]`), so `n×24` is
always % 8. A **12 B** HUD record would copy (`% 4`) but mapping `0..12` is
illegal; pad mapped/copy size to `align(n×12, 8)` or `write_buffer` without
mapping. Do not copy `particles.len()` without `× 32`. Dynamic uniform
offsets 256-aligned — one UBO at offset 0. Do not `resolve_query_set` into
the particle VB (dest offset % 256).

Uniform 256 B is a **bind** rule, not a copy rule: you can `write_buffer` 64 B
into a 256 B UBO; you cannot bind a 64 B std140 range on all backends.

There is no mapped texture. CPU → GPU color is either `Queue::write_texture`
(impl staging, pending-writes, **caller layout may be dense**; wgpu may pad
internally — D3D12 256, Metal 16) or a mapped buffer +
`copy_buffer_to_texture` where **you** set `bytes_per_row` to a multiple of
256. Dense 1920×4 = 7680 is 256-aligned; 1280×4 = 5120 is; 800×4 = 3200 pads
to **3328**. Last row may be dense; intervening rows include pad.

This crate’s capture is the *out* path: `copy_texture_to_buffer` +
`padded_bpr` + `map_async(Read)` + `Wait` on first/last only, then copy
row-by-row into packed BGRA. Do not tighten pitch below 256. Bloom/post
textures are GPU-only (`TEXTURE | COPY_SRC` for capture) — do not stage them
from CPU. Offscreen 1920×1080 BGRA: packed 8 294 400 B equals padded at that
width. Awkward widths are where padding shows.

`write_texture` for one blit/frame; belt + `copy_buffer_to_texture` only if
streaming many mips/layers (`chunk_size` ≥ padded image).
`create_texture_with_data` is `write_texture` per mip/layer — load-once.

Refuse: map range not multiple of 8; `copy_buffer_to_buffer` of 2 bytes;
`copy_texture_to_buffer` with `bytes_per_row = width * 4` when that is 3200.
The ring never hits those if caps stay `max(32, next_pow2(n * 32))`.

`tests/layout.rs` pins both worlds: `size_of` % 4 and % 8; partial map
`particle_ring_need_bytes(n) = n × 32` for odd `n`; empty `n = 0` is 0 and
must not be mapped; grow ×2 from 128 KiB stays % 8; capture pitch
`copy_bytes_per_row_bgra` (1920/1280 exact, 800 → 3328). That is the portable
wgpu contract, not a 4090 quirk. Do not merge HOST_VISIBLE buffer staging
(4/8) with the capture MAP_READ buffer (`padded_bpr × height`). Do not put
the particle field in a texture to dodge alignment — that swaps a solved
32 B copy for a 256 B pitch path.

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
   Not `StagingBelt`: one 128 KiB blit/frame is the wrong shape for a belt.
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
- Persistent `vkMapMemory` / BAR-mapped particle VB through wgpu.
- `StagingBelt` on the particle path.
- Claiming the Z-map or 350/π as theorems inside the renderer.
- Vendoring all of `qga-math`.

Later consumer switch: [MIGRATION.md](MIGRATION.md).

## Claim labels

Theorem / Model / Software fact / Open. Everything in this crate is
**Software fact** unless a comment says otherwise.
