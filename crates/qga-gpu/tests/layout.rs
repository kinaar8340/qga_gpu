use qga_gpu::{
    capture_staging_bytes, copy_bytes_per_row_bgra, particle_ring_need_bytes, FaceVert, FiberMeta,
    FrameUniforms, GpuFiberPoint, GpuHub, GpuOrbInstance, GpuParticle, HudVert, LineVert,
};
use std::mem::size_of;

#[test]
fn frame_uniforms_are_256_bytes() {
    assert_eq!(size_of::<FrameUniforms>(), 256);
}

#[test]
fn particle_fiber_hub_are_32_bytes() {
    assert_eq!(size_of::<GpuParticle>(), 32);
    assert_eq!(size_of::<GpuFiberPoint>(), 32);
    assert_eq!(size_of::<GpuHub>(), 32);
}

#[test]
fn fiber_meta_is_16_bytes() {
    assert_eq!(size_of::<FiberMeta>(), 16);
}

#[test]
fn orb_instance_is_32_bytes() {
    assert_eq!(size_of::<GpuOrbInstance>(), 32);
}

#[test]
fn wgpu_alignment_constants() {
    assert_eq!(wgpu::COPY_BUFFER_ALIGNMENT, 4);
    assert_eq!(wgpu::MAP_ALIGNMENT, 8);
    assert_eq!(wgpu::VERTEX_STRIDE_ALIGNMENT, 4);
    assert_eq!(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT, 256);
    assert_eq!(wgpu::QUERY_SIZE, 8);
    assert_eq!(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 256);
}

#[test]
fn records_meet_map_and_copy_alignment() {
    for n in [
        size_of::<FiberMeta>(),
        size_of::<GpuParticle>(),
        size_of::<GpuFiberPoint>(),
        size_of::<GpuHub>(),
        size_of::<GpuOrbInstance>(),
        size_of::<FrameUniforms>(),
        size_of::<HudVert>(),
        size_of::<FaceVert>(),
        size_of::<LineVert>(),
    ] {
        assert_eq!(n as u64 % wgpu::COPY_BUFFER_ALIGNMENT, 0);
        assert_eq!(n as u64 % wgpu::MAP_ALIGNMENT, 0);
        assert_eq!(n as u64 % wgpu::VERTEX_STRIDE_ALIGNMENT, 0);
    }
}

#[test]
fn hud_face_line_sizes() {
    // 24 B HUD, not 12 B. n×12 would copy (%4) but map 0..12 is illegal.
    assert_eq!(size_of::<HudVert>(), 24);
    assert_eq!(size_of::<FaceVert>(), 48);
    assert_eq!(size_of::<LineVert>(), 32);
    assert_eq!(12u64 % wgpu::COPY_BUFFER_ALIGNMENT, 0);
    assert_ne!(12u64 % wgpu::MAP_ALIGNMENT, 0);
}

#[test]
fn mapped_then_copied_spans_must_be_8() {
    let map = wgpu::MAP_ALIGNMENT;
    let copy = wgpu::COPY_BUFFER_ALIGNMENT;
    for legal in [8u64, 16, 32, 40, 256, 128 * 1024] {
        assert_eq!(legal % map, 0);
        assert_eq!(legal % copy, 0);
    }
    for illegal_map in [4u64, 12] {
        assert_eq!(illegal_map % copy, 0);
        assert_ne!(illegal_map % map, 0);
    }
}

#[test]
fn capture_row_pitch_is_256() {
    assert_eq!(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 256);
    assert_eq!(copy_bytes_per_row_bgra(1920), 7680);
    assert_eq!(copy_bytes_per_row_bgra(1280), 5120);
    assert_eq!(copy_bytes_per_row_bgra(800), 3328);
    assert_ne!(800 * 4, copy_bytes_per_row_bgra(800));
    assert_eq!(1920 * 4, copy_bytes_per_row_bgra(1920));
    assert_eq!(1280 * 4, copy_bytes_per_row_bgra(1280));
    // 3200 is dense 800-wide BGRA — illegal as encoder bytes_per_row.
    assert_ne!(3200 % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
    assert_eq!(3328 % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
}

#[test]
fn capture_staging_is_padded_rows_and_map_aligned() {
    let (w, h) = (800u32, 1080u32);
    let bytes = capture_staging_bytes(w, h);
    assert_eq!(bytes, 3328 * 1080);
    assert_ne!(bytes, u64::from(w) * 4 * u64::from(h));
    assert_eq!(bytes % wgpu::MAP_ALIGNMENT, 0);
    assert_eq!(capture_staging_bytes(1920, 1080), 1920 * 4 * 1080);
    // Origin 0 is required for our BGRA8 full-frame copy; offset 4 would be
    // legal for RGBA8 block size 1 but Metal wants 16 on some copies — stay 0.
}

#[test]
fn particle_partial_map_need_stays_aligned() {
    // Map 0..need, not 0..cap. need = n * 32 must be % 8 even when n is odd.
    assert_eq!(particle_ring_need_bytes(0), 0);
    for n in [1usize, 2, 3, 7, 4095, 4096] {
        let need = particle_ring_need_bytes(n);
        assert_eq!(need % wgpu::COPY_BUFFER_ALIGNMENT, 0, "n={n}");
        assert_eq!(need % wgpu::MAP_ALIGNMENT, 0, "n={n}");
        assert_eq!(need, (n * size_of::<GpuParticle>()) as u64);
    }
}

#[test]
fn particle_slot_grow_stays_map_aligned() {
    let mut cap = 4096 * size_of::<GpuParticle>() as u64;
    assert_eq!(cap % wgpu::MAP_ALIGNMENT, 0);
    cap *= 2;
    assert_eq!(cap % wgpu::MAP_ALIGNMENT, 0);
    cap *= 2;
    assert_eq!(cap % wgpu::MAP_ALIGNMENT, 0);
}
