use qga_gpu::{FiberMeta, FrameUniforms, GpuFiberPoint, GpuHub, GpuOrbInstance, GpuParticle};
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
    const ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    assert_eq!(ALIGN, 256);
    let pad = |w: u32| (w * 4).div_ceil(ALIGN) * ALIGN;
    assert_eq!(pad(1920), 7680);
    assert_eq!(pad(1280), 5120);
    assert_eq!(pad(800), 3328);
}
