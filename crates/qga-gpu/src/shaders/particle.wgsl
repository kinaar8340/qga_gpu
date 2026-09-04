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
    @location(1) mass: f32,
    @location(2) speed: f32,
    @location(3) hue: f32,
};

@vertex
fn vs_main(
    @location(0) corner: vec2<f32>,
    @location(1) inst_pos: vec3<f32>,
    @location(2) inst_mass: f32,
    @location(3) inst_vel: vec3<f32>,
    @location(4) inst_hue: f32,
) -> VsOut {
    let cam_fwd = normalize(cross(frame.cam_up, frame.cam_right));
    let dist = max(length(inst_pos - frame.cam_pos), 1.0);
    let size = mix(0.018, 0.072, clamp(inst_mass * 0.50, 0.0, 1.0))
        * clamp(dist / 16.0, 0.70, 1.70);

    let speed = length(inst_vel);
    var along = frame.cam_right;
    var across = frame.cam_up;
    var stretch = 1.0;
    let v_plane = inst_vel - cam_fwd * dot(inst_vel, cam_fwd);
    if inst_hue < 0.0005 && length(v_plane) > 0.04 {
        along = normalize(v_plane);
        across = normalize(cross(along, cam_fwd));
        stretch = 1.0 + min(speed * 0.045, 2.4);
    }
    let world = inst_pos
        + along * corner.x * size * stretch
        + across * corner.y * size;

    var out: VsOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.uv = corner * 0.5 + vec2(0.5);
    out.mass = inst_mass;
    out.speed = speed;
    out.hue = inst_hue;
    return out;
}

/// Unit-s hue wheel. Software fact of this shader, not a QGA theorem.
fn hue_rgb(h: f32) -> vec3<f32> {
    let k = vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0);
    let p = abs(fract(vec3<f32>(h) + k) * 6.0 - 3.0);
    return clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = in.uv * 2.0 - 1.0;
    let d = length(c);
    if d > 1.0 {
        discard;
    }
    let core = 1.0 - smoothstep(0.0, 0.32, d);
    let halo = 1.0 - smoothstep(0.18, 1.0, d);
    var rgb: vec3<f32>;
    if in.hue > 0.0005 {
        rgb = hue_rgb(in.hue);
    } else {
        rgb = mix(
            vec3<f32>(0.62, 0.86, 1.0),
            vec3<f32>(1.0, 0.78, 0.38),
            clamp(in.speed * 0.07, 0.0, 1.0),
        );
    }
    let col = rgb * (0.55 + core * 0.70);
    let a = (core * 0.55 + halo * 0.18) * (0.75 + 0.25 * frame.glow);
    return vec4<f32>(col, a);
}
