# Later refactor: point consumers at this crate

This repo is the extracted renderer. `qga_engine` and `inner_cone` still carry
their own copies / path deps. Do **not** edit those trees as part of this
extract. The switchover is a later session.

## inner_cone

Today:

```toml
qga-gpu = { path = "../qga_engine/crates/qga-gpu" }
```

After this crate is on `main`:

```toml
qga-gpu = { path = "../qga_gpu/crates/qga-gpu" }
```

Call-site notes (Software fact):

| Engine / inner_cone today | This crate |
|---------------------------|------------|
| `update_static_fibers` | `retain_static_fibers` (alias kept) |
| `update_solid_fibers` | `write_live_fibers` (alias kept) |
| `upload_gpu_hubs(&[(Vec3, f32, Vec3)])` | `upload_hubs(&[GpuHub])` (tuple alias kept) |
| `write_hud_verts` | `write_hud` (alias kept) |
| `write_particles(&[qga_sim::Particle])` | `write_particles(&[GpuParticle])` |
| `update_solid_fibers(&[qga_math::Fiber])` | `GpuFiber` or `--features qga-math` `From` |
| `render(..., SceneKind::Reveal, time, grab)` | `render(gpu, cam, vis, time, capture)` |

Static topology: tessellate once, then `retain_meshes` / `retain_static_fibers`.
Identical re-upload is a no-op (`static_uploads` stays 1; `static_skipped` grows).
Live harmonics go through `write_live_fibers`. Particles through the staging ring.

## qga_engine

Replace the in-tree `crates/qga-gpu` member with a git or path dep on this repo.
Leave `qga-math` / `qga-sim` / `qga-app` in the engine workspace.

Path (local):

```toml
# qga_engine/Cargo.toml workspace.dependencies
qga-gpu = { path = "../qga_gpu/crates/qga-gpu" }
```

Git (after this repo is pushed):

```toml
qga-gpu = { git = "https://github.com/kinaar8340/qga_gpu" }
```

Then delete `qga_engine/crates/qga-gpu` and drop it from `[workspace].members`.
`qga-app` keeps calling the public surface above. Realm / cosmos / oam / reveal
**scenes stay in qga-app / qga-sim**, not in this crate.

Do not open a PR into `qga_engine` from this extract.
