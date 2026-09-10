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
    @location(7) inst_lod: f32,
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
    var p = vin.pos;
    p.y *= frame.height_scale;
    let world = p * vin.inst_scale + vin.inst_offset;
    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.color = vin.color * vin.inst_color;
    let inst_a = select(1.0, vin.inst_lod, vin.inst_lod > 1e-4);
    out.alpha = vin.alpha * inst_a;
    out.nrm = vin.nrm;
    out.world = world;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.nrm);
    let light = normalize(vec3<f32>(0.28, 0.82, 0.48));
    let fill = normalize(vec3<f32>(-0.52, 0.18, -0.42));
    let v = normalize(frame.cam_pos - in.world);
    let ndl = max(dot(n, light), 0.0);
    let wrap = ndl * 0.42 + 0.38 + max(dot(n, fill), 0.0) * 0.22;
    let h = normalize(light + v);
    let spec = pow(max(dot(n, h), 0.0), 52.0) * (0.18 + 0.16 * frame.glow);
    let rim = pow(clamp(1.0 - max(dot(n, v), 0.0), 0.0, 1.0), 2.6) * 0.24;
    // zener/2.4 == 1 at VisualState default: identity mix.
    let z = frame.zener / 2.4;
    let glass = mix(in.color, vec3<f32>(0.70, 0.86, 1.0), clamp((z - 1.0) * 0.25, 0.0, 0.35));
    let col = glass * wrap + vec3<f32>(spec) + glass * rim;
    let a = in.alpha * clamp(frame.aperture, 0.25, 1.6);
    return vec4<f32>(col, a);
}
