struct Frame {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    time: f32,
    cam_right: vec3<f32>,
    pulse: f32,
    cam_up: vec3<f32>,
    glow: f32,
    tube_radius: f32,
    aperture: f32,
    height_scale: f32,
    zener: f32,
};

@group(0) @binding(0) var<uniform> frame: Frame;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(4) inst_offset: vec3<f32>,
    @location(6) inst_scale: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(vin: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(vin.pos * vin.inst_scale + vin.inst_offset, 1.0);
    out.color = vin.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
