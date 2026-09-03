# qga_gpu

Modular wgpu/Vulkan renderer extracted from ~/Projects/qga_engine/crates/qga-gpu
and the inner_cone upload path. Geometry meaning lives in qga / qga-math.
This crate owns the frame.

## Hard rules
- Do not edit ~/Projects/qga_engine or ~/Projects/inner_cone in this repo session.
- Do not sys.path or subprocess Python at runtime.
- Do not copy realm, cosmos n-body, oam PDE, or reveal Lorenz scenes.
- Claim labels: Theorem / Model / Software fact / Open. Renderer claims are Software fact.
- Frame uniforms 256-byte aligned. Particle and fiber records 32 bytes.
- Buffer copy/map: 4 / 8. Texture copy rows: 256. Capture uses padded_bpr.
- wgpu textures are optimal-tiled; padded_bpr is WebGPU-on-Vulkan tax, not vkGetImageSubresourceLayout.
- wgpu has no VkResult table. Validation panics. particle_fallbacks is a ready-slot miss, not Unaligned*.
- No per-frame error scopes. Drop without pop discards. Do not scope write_particles.
- Do not depend on wgpu-core to downcast TransferError. Grep description. No VkResult under Validation.
- Debug validation by the Caused by tree + fn_ident, not a numeric code. Cluster: map 8, copy 4, pitch 256, submit-while-mapped. Default panic; do not recover.
- wgpu trace is a WebGPU API log; RenderDoc is a Vulkan frame capture. Opposite sides of wgpu-hal. wgpu 24 cannot record (gfx-rs/wgpu#5974). Counters are neither tool. RenderDoc on windowed 4090.
- Vulkan via wgpu only. No OpenGL. CUDA is out of scope for v0.

## Target machine
RTX 4090 24 GiB, driver 580, Vulkan 1.4, Wayland + winit, Ryzen 9 3900X.

## Public surface (v0)
GpuContext, Renderer, Camera, VisualState,
retain_static_fibers / write_live_fibers (hash + tube_radius no-op),
retain_meshes (sphere/cone/torus tessellated once),
draw_geodesic_orb(transform, color, lod),
write_particles (3-slot ring; any ready slot; skip if unchanged; never drop pending),
upload_hubs, write_hud,
render(gpu, cam, vis, time, capture).
Live uniforms: aperture, height_scale, zener, time.
UploadStats.static_uploads counts real static fiber GPU writes (headless: == 1).
Dirty particles: ring_copies + particle_fallbacks >= frames; fallbacks allowed.

## Optimization contract (from inner_cone)
- Tessellate static topology once. Cones/spheres/tori are parametric.
- Rebuild only aperture/height/zener/camera/particles/live harmonics.
- Instance repeated geodesic orbs.
- Tubes from centerline + shader extrusion, not CPU ribbon rebuild.
- Profile Queue::write_buffer before adding meshlets.
- StagingBelt only if tens of small copies per present; never on the 128 KiB particle blit.
- One encoder, one submit per present. Do not empty-submit or split upload/draw submits at this size.
- map_async is cheap to call; Wait is GPU-timeline. particle_fallbacks is the meter. Never Wait on the ring.

## Features
default = ["winit"]
winit, headless, capture, glow, qga-math (optional From; not default)
