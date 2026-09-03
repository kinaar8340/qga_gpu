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
