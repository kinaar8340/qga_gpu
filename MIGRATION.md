# Consumer wiring

This repo is the extracted renderer. Do **not** edit `qga_engine` or
`inner_cone` from this extract. Local `inner_cone` `03e1fb2` is the contract
if GitHub still shows `73de02d`.

## Status (Software fact)

| Consumer | Form | Features | Fiber conversion |
|----------|------|----------|------------------|
| `inner_cone` @ `03e1fb2` | `path = "../qga_gpu/crates/qga-gpu"` | `["capture"]` | `geometry::gpu_fiber` |
| `qga_engine` (`qga-app`) | git, **no `rev`** | `winit`, `headless`, `capture`, `glow` | `qga-app::convert` |

`qga-math` / `qga-sim` stay in the engine workspace. Do **not** enable
`qga-gpu/qga-math` on either consumer: the renderer must not take a default
math dep.

## inner_cone

Switched. Working dep while sibling checkouts drift:

```toml
qga-math = { path = "../qga_engine/crates/qga-math" }
qga-sim  = { path = "../qga_engine/crates/qga-sim" }
qga-gpu  = { path = "../qga_gpu/crates/qga-gpu", features = ["capture"] }
```

That is **not** `../qga_engine/crates/qga-gpu`. `capture` is the right local
set for `--export` / F12. `winit` is this crate’s default; `glow` is optional
bloom the demo can drive through `VisualState` either way.

If that repo later uses the git form, pin a sha and add `winit` / `headless`
/ `glow` only where that binary actually needs them:

```toml
qga-gpu = { git = "https://github.com/kinaar8340/qga_gpu", rev = "<sha>", features = ["winit", "headless", "capture", "glow"] }
```

`inner_cone` has no headless binary. `--export --frames N` is the windowed
stand-in and does not print or assert `UploadStats`. Until it does (print
from `--export`, or a thin `--headless --frames N` that exits non-zero on
the predicates below), this crate’s demo is the only proof the extracted
renderer is not retessellating the sculpture every frame:

```bash
make headless   # 8 offscreen frames; requires static_uploads == 1
make ring       # 300 dirty frames; ring_copies + particle_fallbacks >= frames
```

Fallbacks are allowed and counted. `particle_skipped == 0` when dirty.

## qga_engine

In-tree `crates/qga-gpu` is gone. Workspace dep:

```toml
# qga_engine/Cargo.toml workspace.dependencies
qga-gpu = { git = "https://github.com/kinaar8340/qga_gpu", features = ["winit", "headless", "capture", "glow"] }
```

**Risk:** no `rev`. `Cargo.lock` pins a sha until the next `cargo update -p
qga-gpu`. A push to this repo’s `main` can change record layout or feature
defaults while `inner_cone`’s path dep stays frozen to the sibling tree.
Prefer `rev = "<sha>"` before treating `qga-app` as a published consumer.

`qga-app --headless` does not print or assert `UploadStats`. Realm / cosmos /
oam / reveal **scenes stay in qga-app / qga-sim**, not in this crate.

Do not open a PR into `qga_engine` from this extract.

## Call-site notes (Software fact)

| Engine / inner_cone names | This crate |
|---------------------------|------------|
| `update_static_fibers` | `retain_static_fibers` (alias kept) |
| `update_solid_fibers` | `write_live_fibers` (alias kept) |
| `upload_gpu_hubs(&[(Vec3, f32, Vec3)])` | `upload_hubs(&[GpuHub])` (tuple alias kept) |
| `write_hud_verts` | `write_hud` (alias kept) |
| `write_particles(&[qga_sim::Particle])` | `write_particles(&[GpuParticle])` |
| `update_solid_fibers(&[qga_math::Fiber])` | `GpuFiber` or `--features qga-math` `From` |
| `render(..., SceneKind::Reveal, time, grab)` | `render(gpu, cam, vis, time, capture)` |

`inner_cone` `03e1fb2` already calls the new names. Static topology: tessellate
once, then `retain_meshes` / `retain_static_fibers`. Identical re-upload is a
no-op (`static_uploads` stays 1; `static_skipped` grows). Live harmonics go
through `write_live_fibers`. Particles through the staging ring.
