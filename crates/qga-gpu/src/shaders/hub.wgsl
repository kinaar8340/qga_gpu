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

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) corner: vec2<f32>,
    @location(1) inst_pos: vec3<f32>,
    @location(2) inst_radius: f32,
    @location(3) inst_color: vec3<f32>,
) -> VsOut {
    let r = inst_radius * (1.0 + 0.08 * sin(frame.time * 1.4 + inst_pos.x));
    let world = inst_pos
        + frame.cam_right * corner.x * r
        + frame.cam_up * corner.y * r;
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.uv = corner;
    out.color = inst_color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    if d > 1.0 {
        discard;
    }
    let core = 1.0 - smoothstep(0.0, 0.4, d);
    let ring = exp(-pow((d - 0.72) * 8.0, 2.0));
    let col = in.color * (0.55 + core * 1.4) * frame.glow + vec3<f32>(1.0) * ring * 0.85;
    let a = core * 0.95 + ring * 0.65;
    return vec4<f32>(col, a);
}
