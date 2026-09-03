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
    @location(1) alpha: f32,
    @location(2) color: vec3<f32>,
    @location(3) nrm: vec3<f32>,
    @location(4) inst_offset: vec3<f32>,
    @location(5) inst_color: vec3<f32>,
    @location(6) inst_scale: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) alpha: f32,
    @location(2) nrm: vec3<f32>,
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_main(vin: VsIn) -> VsOut {
    let world = vin.pos * vin.inst_scale + vin.inst_offset;
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.color = vin.color * vin.inst_color;
    out.alpha = vin.alpha;
    out.nrm = vin.nrm;
    out.world = world;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.nrm);
    let light = normalize(vec3<f32>(0.28, 0.82, 0.48));
    let wrap = abs(dot(n, light)) * 0.35 + 0.65;
    let col = in.color * wrap;
    return vec4<f32>(col, in.alpha);
}
