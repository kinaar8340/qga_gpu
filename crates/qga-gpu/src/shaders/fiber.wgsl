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

struct FiberMeta {
    n_points: u32,
    n_fibers: u32,
    radius: f32,
    _pad: u32,
};

struct FiberPoint {
    pos: vec3<f32>,
    along: f32,
    color: vec3<f32>,
    phase: f32,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> fiber_meta: FiberMeta;
@group(1) @binding(1) var<storage, read> fibers: array<FiberPoint>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) along: f32,
    @location(3) phase: f32,
    @location(4) view_dir: vec3<f32>,
    @location(5) around: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    let corner = vid % 6u;
    let seg = vid / 6u;
    let n = max(fiber_meta.n_points, 1u);
    let fi = seg / n;
    let i = seg % n;
    let a = fibers[fi * n + i];
    let b = fibers[fi * n + (i + 1u) % n];

    let tangent = b.pos - a.pos;
    var side = cross(tangent, frame.cam_up);
    if dot(side, side) < 1e-10 {
        side = cross(tangent, frame.cam_right);
    }
    let sl = length(side);
    if sl > 1e-8 {
        side = side / sl;
    } else {
        side = frame.cam_right;
    }
    let radius = select(frame.tube_radius, fiber_meta.radius, fiber_meta.radius > 1e-8);
    side = side * radius;

    var pos: vec3<f32>;
    var along: f32;
    var around: f32;
    switch corner {
        case 0u: { pos = a.pos - side; along = a.along; around = 0.0; }
        case 1u: { pos = a.pos + side; along = a.along; around = 1.0; }
        case 2u: { pos = b.pos + side; along = b.along; around = 1.0; }
        case 3u: { pos = a.pos - side; along = a.along; around = 0.0; }
        case 4u: { pos = b.pos + side; along = b.along; around = 1.0; }
        default: { pos = b.pos - side; along = b.along; around = 0.0; }
    }

    let color = select(b.color, a.color, corner < 2u || corner == 3u);
    let phase = select(b.phase, a.phase, corner < 2u || corner == 3u);

    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(pos, 1.0);
    out.uv = vec2<f32>(along, around);
    out.color = color;
    out.along = along;
    out.phase = phase;
    out.view_dir = frame.cam_pos - pos;
    out.around = around;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tau = 6.28318530718;
    let t = frame.time;
    let along = in.along;
    let around = in.around;

    var flux = sin(along * 18.0 + around * 2.5 + in.phase - t * 3.0) * 0.5 + 0.5;
    flux = pow(flux, 1.6);

    let helix = along * 9.0 * tau + around * tau * 1.2 + in.phase - t * 0.35;
    let nodes = pow(sin(helix) * 0.5 + 0.5, 3.2);
    let nodes2 = pow(sin(helix * 0.55 - t * 0.9) * 0.5 + 0.5, 4.0) * 0.55;

    let v = normalize(in.view_dir);
    let rim = pow(1.0 - abs(around - 0.5) * 1.8, 2.5);
    let fres = pow(1.0 - abs(v.y), 1.8);
    let breathe = sin(frame.time * 0.8 + in.phase * 0.3) * 0.5 + 0.5;

    var color = in.color * (0.50 + flux * 0.85 + (nodes + nodes2) * 0.55);
    color += in.color * max(rim, fres) * 0.40;
    color *= (0.88 + breathe * frame.pulse * 0.18) * (0.70 + 0.40 * frame.glow);

    let alpha = 0.82 + 0.14 * max(flux, nodes);
    return vec4<f32>(color, clamp(alpha, 0.0, 1.0));
}
