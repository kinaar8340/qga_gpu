use qga_gpu::{
    copy_bytes_per_row_bgra, particle_ring_need_bytes, FiberMeta, FrameUniforms, GpuFiberPoint,
    GpuHub, GpuOrbInstance, GpuParticle,
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
fn records_meet_map_and_copy_alignment() {
    assert_eq!(wgpu::COPY_BUFFER_ALIGNMENT, 4);
    assert_eq!(wgpu::MAP_ALIGNMENT, 8);
    for n in [
        size_of::<FiberMeta>(),
        size_of::<GpuParticle>(),
        size_of::<GpuFiberPoint>(),
        size_of::<GpuHub>(),
        size_of::<GpuOrbInstance>(),
        size_of::<FrameUniforms>(),
    ] {
        assert_eq!(n as u64 % wgpu::COPY_BUFFER_ALIGNMENT, 0);
        assert_eq!(n as u64 % wgpu::MAP_ALIGNMENT, 0);
    }
}

#[test]
fn capture_row_pitch_is_256() {
    assert_eq!(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 256);
    assert_eq!(copy_bytes_per_row_bgra(1920), 7680);
    assert_eq!(copy_bytes_per_row_bgra(1280), 5120);
    assert_eq!(copy_bytes_per_row_bgra(800), 3328);
    // Packed vs padded: 800-wide BGRA is 3200 dense, 3328 in the staging buffer.
    assert_ne!(800 * 4, copy_bytes_per_row_bgra(800));
    assert_eq!(1920 * 4, copy_bytes_per_row_bgra(1920));
    assert_eq!(1280 * 4, copy_bytes_per_row_bgra(1280));
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
