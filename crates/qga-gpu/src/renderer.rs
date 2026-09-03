//! Pipelines, upload ring, and the frame. Software fact.

use crate::camera::Camera;
use crate::context::GpuContext;
use crate::mesh::{sphere_faces, Mesh};
use crate::profile::HardwareProfile;
use crate::types::{
    FaceVert, FiberMeta, FrameUniforms, GpuFiber, GpuFiberPoint, GpuHub, GpuOrbInstance,
    GpuParticle, HudVert, LineStyle, LineVert,
};
use anyhow::Result;
use glam::Vec3;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub struct VisualState {
    pub glow: f32,
    pub pulse: f32,
    pub flux_speed: f32,
    pub tube_radius: f32,
    pub paused: bool,
    pub show_rings: bool,
    pub aperture: f32,
    pub height_scale: f32,
    pub zener: f32,
}

impl Default for VisualState {
    fn default() -> Self {
        Self {
            glow: 1.15,
            pulse: 0.55,
            flux_speed: 1.0,
            tube_radius: HardwareProfile::THIS_BOX.tube_radius,
            paused: false,
            show_rings: true,
            aperture: 1.0,
            height_scale: 1.0,
            zener: 2.4,
        }
    }
}

pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UploadStats {
    pub write_buffer_calls: u64,
    pub ring_copies: u64,
    pub fiber_reallocs: u64,
    /// Particle staging/VB grew. Not mixed into `fiber_reallocs`.
    pub particle_grows: u64,
    /// `Queue::write_buffer` used for particles (zero slots ready).
    /// Counted, allowed. One in 300 headless-no-vsync is healthy on a 4090.
    pub particle_fallbacks: u64,
    /// Times static fiber GPU buffers were actually written.
    pub static_uploads: u64,
    pub static_skipped: u64,
    pub live_skipped: u64,
    pub particle_skipped: u64,
}

struct Pipelines {
    fiber: wgpu::RenderPipeline,
    particle: wgpu::RenderPipeline,
    hub: wgpu::RenderPipeline,
    face: wgpu::RenderPipeline,
    line: wgpu::RenderPipeline,
    #[cfg(not(feature = "glow"))]
    blit: wgpu::RenderPipeline,
    hud: wgpu::RenderPipeline,
    #[cfg(feature = "glow")]
    post: wgpu::RenderPipeline,
}

struct ColorTarget {
    view: wgpu::TextureView,
    tex: wgpu::Texture,
    blit_bg: wgpu::BindGroup,
    staging: Option<wgpu::Buffer>,
    size: (u32, u32),
    padded_bpr: u32,
}

struct FrameBind {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
}

struct GrowBuf {
    buf: wgpu::Buffer,
    cap: u64,
    count: u32,
}

struct FiberSlot {
    points: wgpu::Buffer,
    meta: wgpu::Buffer,
    bg: wgpu::BindGroup,
    cap_points: u32,
    n_points: u32,
    n_fibers: u32,
    hash: u64,
    radius: f32,
}

/// 3-slot HOST_VISIBLE staging → DEVICE_LOCAL VB.
/// A slot is mapped on the CPU or in a submitted copy, never both.
/// Do not persist-map the VERTEX buffer (no `MAPPABLE_PRIMARY_BUFFERS`).
struct ParticleRing {
    staging: [wgpu::Buffer; 3],
    ready: [Arc<AtomicBool>; 3],
    gpu: wgpu::Buffer,
    cap_bytes: u64,
    cursor: usize,
    /// Outstanding staging → GPU copies. Never dropped on fallback.
    pending: Vec<(usize, u64)>,
    n: u32,
}

pub struct Renderer {
    pipelines: Pipelines,
    frame: FrameBind,
    fiber_layout: wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,

    quad_vb: wgpu::Buffer,

    live: FiberSlot,
    static_fibers: FiberSlot,
    static_dirty: bool,

    hub: GrowBuf,
    face: GrowBuf,
    line: GrowBuf,
    hud: GrowBuf,
    orb: GrowBuf,
    n_orb_instances: u32,
    geo_face: GrowBuf,
    geo_inst: GrowBuf,
    geo_count: u32,
    geo_queue: Vec<GpuOrbInstance>,
    mesh_hash: u64,
    particle_hash: u64,

    particles: ParticleRing,

    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    color: Option<ColorTarget>,
    resolve: Option<ColorTarget>,

    stats: UploadStats,
}

fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    })
}

fn additive() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

const QUAD: [[f32; 2]; 6] = [
    [-1.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [-1.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
];

const FACE_ATTRS: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 12,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 16,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 32,
        shader_location: 3,
    },
];
const LINE_ATTRS: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 1,
    },
];
const ORB_INST_ATTRS: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 12,
        shader_location: 6,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 16,
        shader_location: 5,
    },
];
const QUAD_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
const HUB_INST_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![1 => Float32x3, 2 => Float32, 3 => Float32x3];
const PART_INST_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    1 => Float32x3,
    2 => Float32,
    3 => Float32x3,
    4 => Float32
];

impl Renderer {
    pub fn new(gpu: &GpuContext) -> Result<Self> {
        let device = &gpu.device;
        let surface_format = gpu
            .config
            .as_ref()
            .map(|c| c.format)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        let fiber_sm = shader(device, "fiber", include_str!("shaders/fiber.wgsl"));
        let particle_sm = shader(device, "particle", include_str!("shaders/particle.wgsl"));
        let hub_sm = shader(device, "hub", include_str!("shaders/hub.wgsl"));
        let face_sm = shader(device, "face", include_str!("shaders/face.wgsl"));
        let line_sm = shader(device, "line", include_str!("shaders/line.wgsl"));
        #[cfg(not(feature = "glow"))]
        let blit_sm = shader(device, "blit", include_str!("shaders/blit.wgsl"));
        let hud_sm = shader(device, "hud", include_str!("shaders/hud.wgsl"));

        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let fiber_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fiber-layout"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX),
                storage_entry(1, wgpu::ShaderStages::VERTEX, true),
            ],
        });

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-ub"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame-bg"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

        let fiber = fiber_pipeline(
            device,
            &fiber_sm,
            &frame_layout,
            &fiber_layout,
            surface_format,
        );
        let hub = color_pipeline(
            device,
            "hub",
            &hub_sm,
            &[&frame_layout],
            surface_format,
            true,
            true,
            &[
                wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &QUAD_ATTRS,
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuHub>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &HUB_INST_ATTRS,
                },
            ],
            "vs_main",
            "fs_main",
        );
        let particle = color_pipeline(
            device,
            "particle",
            &particle_sm,
            &[&frame_layout],
            surface_format,
            true,
            true,
            &[
                wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &QUAD_ATTRS,
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuParticle>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &PART_INST_ATTRS,
                },
            ],
            "vs_main",
            "fs_main",
        );
        let orb_inst_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuOrbInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ORB_INST_ATTRS,
        };
        let face = alpha_pipeline(
            device,
            "face",
            &face_sm,
            &frame_layout,
            surface_format,
            wgpu::PrimitiveTopology::TriangleList,
            &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FaceVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &FACE_ATTRS,
                },
                orb_inst_layout.clone(),
            ],
            false,
            0,
        );
        let line = alpha_pipeline(
            device,
            "line",
            &line_sm,
            &frame_layout,
            surface_format,
            wgpu::PrimitiveTopology::LineList,
            &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &LINE_ATTRS,
                },
                orb_inst_layout,
            ],
            false,
            -1,
        );

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit-samp"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        #[cfg(not(feature = "glow"))]
        let blit = color_pipeline(
            device,
            "blit",
            &blit_sm,
            &[&blit_layout],
            surface_format,
            false,
            false,
            &[],
            "vs_main",
            "fs_main",
        );

        #[cfg(feature = "glow")]
        let post = {
            let post_sm = shader(device, "post", include_str!("shaders/post.wgsl"));
            color_pipeline(
                device,
                "post",
                &post_sm,
                &[&frame_layout, &blit_layout],
                surface_format,
                false,
                false,
                &[],
                "vs_main",
                "fs_main",
            )
        };

        let hud_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud-pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let hud_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let hud = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud"),
            layout: Some(&hud_pl),
            vertex: wgpu::VertexState {
                module: &hud_sm,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<HudVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &hud_attrs,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &hud_sm,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let dummy_fiber = [GpuFiberPoint {
            pos: [0.0; 3],
            along: 0.0,
            color: [0.0; 3],
            phase: 0.0,
        }];
        let dummy_hub = [GpuHub {
            pos: [0.0; 3],
            radius: 0.2,
            color: [1.0, 0.8, 0.3],
            pad: 0.0,
        }];
        let dummy_face = [FaceVert {
            pos: [0.0; 3],
            alpha: 0.0,
            color: [0.0; 3],
            pad: 0.0,
            nrm: [0.0, 1.0, 0.0],
            pad2: 0.0,
        }];
        let dummy_line = [LineVert {
            pos: [0.0; 3],
            pad: 0.0,
            color: [0.0; 4],
        }; 2];
        let dummy_hud = [HudVert::new([0.0, 0.0], [0.0; 4]); 6];
        let dummy_orb = [GpuOrbInstance::identity()];

        let live = make_fiber_slot(device, &fiber_layout, &dummy_fiber, "live");
        let static_fibers = make_fiber_slot(device, &fiber_layout, &dummy_fiber, "static");

        // 4k × 32 B = 128 KiB. Grow ×2 on cap miss. Do not map a fat arena.
        let particles = ParticleRing::new(device, ParticleRing::MIN_CAP);

        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad"),
            contents: bytemuck::cast_slice(&QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Ok(Self {
            pipelines: Pipelines {
                fiber,
                particle,
                hub,
                face,
                line,
                #[cfg(not(feature = "glow"))]
                blit,
                hud,
                #[cfg(feature = "glow")]
                post,
            },
            frame: FrameBind {
                buffer: frame_buffer,
                group: frame_group,
            },
            fiber_layout,
            surface_format,
            quad_vb,
            live,
            static_fibers,
            static_dirty: true,
            hub: grow_init(
                device,
                "hub-vb",
                bytemuck::cast_slice(&dummy_hub),
                vertex_dst(),
            ),
            face: grow_init(
                device,
                "face-vb",
                bytemuck::cast_slice(&dummy_face),
                vertex_dst(),
            ),
            line: grow_init(
                device,
                "line-vb",
                bytemuck::cast_slice(&dummy_line),
                vertex_dst(),
            ),
            hud: grow_init(
                device,
                "hud-vb",
                bytemuck::cast_slice(&dummy_hud),
                vertex_dst(),
            ),
            orb: grow_init(
                device,
                "orb-vb",
                bytemuck::cast_slice(&dummy_orb),
                vertex_dst(),
            ),
            n_orb_instances: 1,
            geo_face: {
                let faces = sphere_faces(1.0, glam::Vec3::ONE, 10, 14);
                let mut g = grow_init(device, "geo-vb", bytemuck::cast_slice(&faces), vertex_dst());
                g.count = faces.len() as u32;
                g
            },
            geo_inst: grow_init(
                device,
                "geo-inst",
                bytemuck::cast_slice(&dummy_orb),
                vertex_dst(),
            ),
            geo_count: 0,
            geo_queue: Vec::new(),
            mesh_hash: 0,
            particle_hash: 0,
            particles,
            blit_layout,
            blit_sampler,
            color: None,
            resolve: None,
            stats: UploadStats::default(),
        })
    }

    pub fn upload_stats(&self) -> UploadStats {
        self.stats
    }

    pub fn mark_static_dirty(&mut self) {
        self.static_dirty = true;
    }

    pub fn retain_static_fibers(
        &mut self,
        gpu: &GpuContext,
        fibers: &[GpuFiber],
        tube: f32,
    ) -> Result<()> {
        let packed = pack_fibers(fibers);
        let hash = fnv1a64(bytemuck::cast_slice(&packed.points)) ^ (tube.to_bits() as u64);
        if !self.static_dirty
            && hash == self.static_fibers.hash
            && packed.n_fibers == self.static_fibers.n_fibers
            && packed.n_points == self.static_fibers.n_points
        {
            self.stats.static_skipped += 1;
            return Ok(());
        }
        self.write_fiber_slot(gpu, true, packed, tube, hash);
        self.static_dirty = false;
        Ok(())
    }

    pub fn write_live_fibers(
        &mut self,
        gpu: &GpuContext,
        fibers: &[GpuFiber],
        tube: f32,
    ) -> Result<()> {
        let packed = pack_fibers(fibers);
        let hash = fnv1a64(bytemuck::cast_slice(&packed.points)) ^ (tube.to_bits() as u64);
        if hash == self.live.hash
            && packed.n_fibers == self.live.n_fibers
            && tube == self.live.radius
        {
            self.stats.live_skipped += 1;
            return Ok(());
        }
        self.write_fiber_slot(gpu, false, packed, tube, hash);
        Ok(())
    }

    pub fn update_static_fibers(&mut self, gpu: &GpuContext, fibers: &[GpuFiber], radius: f32) {
        let _ = self.retain_static_fibers(gpu, fibers, radius);
    }

    pub fn update_solid_fibers(&mut self, gpu: &GpuContext, fibers: &[GpuFiber], radius: f32) {
        let _ = self.write_live_fibers(gpu, fibers, radius);
    }

    /// Tessellate sphere/cone/torus once. No-op when the mesh set + lod is unchanged.
    pub fn retain_meshes(&mut self, gpu: &GpuContext, meshes: &[Mesh], lod: u32) -> Result<()> {
        let mut h = fnv1a64(&lod.to_le_bytes());
        for m in meshes {
            h ^= fnv1a64(bytemuck::bytes_of(&m.color.to_array()));
            h ^= match m.kind {
                crate::mesh::MeshKind::Sphere { radius } => radius.to_bits() as u64,
                crate::mesh::MeshKind::Cone { radius, height } => {
                    (radius.to_bits() as u64) ^ ((height.to_bits() as u64) << 1)
                }
                crate::mesh::MeshKind::Torus { major, minor } => {
                    (major.to_bits() as u64) ^ ((minor.to_bits() as u64) << 1)
                }
            };
            h ^= m.rot_x.to_bits() as u64;
            h ^= (m.rot_z.to_bits() as u64).rotate_left(17);
        }
        if h == self.mesh_hash && !self.static_dirty {
            self.stats.static_skipped += 1;
            return Ok(());
        }
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        let mut fibers = Vec::new();
        for m in meshes {
            let t = m.tessellate(lod);
            faces.extend(t.faces);
            edges.extend(t.edges);
            fibers.extend(t.fibers);
        }
        self.update_faces(gpu, &faces);
        self.update_line_segments(gpu, &edges, LineStyle::black_hairline());
        if !fibers.is_empty() {
            self.retain_static_fibers(gpu, &fibers, 0.03)?;
        }
        self.mesh_hash = h;
        Ok(())
    }

    pub fn draw_geodesic_orb(&mut self, transform: glam::Mat4, color: glam::Vec3, lod: u32) {
        self.geo_queue
            .push(GpuOrbInstance::from_transform(transform, color, lod));
    }

    fn write_fiber_slot(
        &mut self,
        gpu: &GpuContext,
        is_static: bool,
        packed: PackedFibers,
        radius: f32,
        hash: u64,
    ) {
        let slot = if is_static {
            &mut self.static_fibers
        } else {
            &mut self.live
        };
        if packed.n_fibers == 0 {
            slot.n_fibers = 0;
            slot.n_points = 0;
            slot.hash = hash;
            slot.radius = radius;
            return;
        }
        let need = packed.points.len() as u32;
        if slot.cap_points < need {
            let mut cap = slot.cap_points.max(16).max(need);
            while cap < need {
                cap = cap.saturating_mul(2);
            }
            slot.points = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(if is_static {
                    "static-fiber-points"
                } else {
                    "live-fiber-points"
                }),
                size: cap as u64 * 32,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            slot.cap_points = cap;
            slot.bg = bind_fiber(&gpu.device, &self.fiber_layout, &slot.meta, &slot.points);
            self.stats.fiber_reallocs += 1;
        }
        gpu.queue
            .write_buffer(&slot.points, 0, bytemuck::cast_slice(&packed.points));
        self.stats.write_buffer_calls += 1;
        let meta = FiberMeta {
            n_points: packed.n_points,
            n_fibers: packed.n_fibers,
            radius,
            _pad: 0,
        };
        gpu.queue
            .write_buffer(&slot.meta, 0, bytemuck::bytes_of(&meta));
        self.stats.write_buffer_calls += 1;
        slot.n_points = packed.n_points;
        slot.n_fibers = packed.n_fibers;
        slot.hash = hash;
        slot.radius = radius;
        if is_static {
            self.stats.static_uploads += 1;
        }
    }

    pub fn update_faces(&mut self, gpu: &GpuContext, faces: &[FaceVert]) {
        write_grow(
            gpu,
            &mut self.face,
            bytemuck::cast_slice(faces),
            "face-vb",
            &mut self.stats,
        );
        self.face.count = faces.len() as u32;
    }

    pub fn update_line_segments(
        &mut self,
        gpu: &GpuContext,
        edges: &[[Vec3; 2]],
        style: LineStyle,
    ) {
        if edges.is_empty() {
            self.line.count = 0;
            return;
        }
        let col = [style.color.x, style.color.y, style.color.z, style.opacity];
        let verts: Vec<LineVert> = edges
            .iter()
            .flat_map(|[a, b]| {
                [
                    LineVert {
                        pos: (*a).into(),
                        pad: 0.0,
                        color: col,
                    },
                    LineVert {
                        pos: (*b).into(),
                        pad: 0.0,
                        color: col,
                    },
                ]
            })
            .collect();
        write_grow(
            gpu,
            &mut self.line,
            bytemuck::cast_slice(&verts),
            "line-vb",
            &mut self.stats,
        );
        self.line.count = verts.len() as u32;
        let _ = style.width;
        let _ = style.depth_bias;
    }

    pub fn update_orb_instances(&mut self, gpu: &GpuContext, instances: &[GpuOrbInstance]) {
        if instances.is_empty() {
            let id = GpuOrbInstance::identity();
            write_grow(
                gpu,
                &mut self.orb,
                bytemuck::bytes_of(&id),
                "orb-vb",
                &mut self.stats,
            );
            self.n_orb_instances = 1;
            return;
        }
        write_grow(
            gpu,
            &mut self.orb,
            bytemuck::cast_slice(instances),
            "orb-vb",
            &mut self.stats,
        );
        self.n_orb_instances = instances.len() as u32;
    }

    pub fn upload_hubs(&mut self, gpu: &GpuContext, hubs: &[GpuHub]) -> Result<()> {
        if hubs.is_empty() {
            self.hub.count = 0;
            return Ok(());
        }
        write_grow(
            gpu,
            &mut self.hub,
            bytemuck::cast_slice(hubs),
            "hub-vb",
            &mut self.stats,
        );
        self.hub.count = hubs.len() as u32;
        Ok(())
    }

    pub fn upload_gpu_hubs(&mut self, gpu: &GpuContext, hubs: &[(Vec3, f32, Vec3)]) {
        let data: Vec<GpuHub> = hubs
            .iter()
            .map(|(pos, r, col)| GpuHub::new(*pos, *r, *col))
            .collect();
        let _ = self.upload_hubs(gpu, &data);
    }

    pub fn write_hud(&mut self, gpu: &GpuContext, verts: &[HudVert]) -> Result<()> {
        if verts.is_empty() {
            self.hud.count = 0;
            return Ok(());
        }
        write_grow(
            gpu,
            &mut self.hud,
            bytemuck::cast_slice(verts),
            "hud-vb",
            &mut self.stats,
        );
        self.hud.count = verts.len() as u32;
        Ok(())
    }

    pub fn write_hud_verts(&mut self, gpu: &GpuContext, verts: &[HudVert]) {
        let _ = self.write_hud(gpu, verts);
    }

    pub fn write_particles(&mut self, gpu: &GpuContext, particles: &[GpuParticle]) -> Result<()> {
        if particles.is_empty() {
            self.particles.n = 0;
            self.particle_hash = 0;
            // Do not map 0..0 — wgpu size 0 is not a valid map/copy range.
            return Ok(());
        }
        let bytes = bytemuck::cast_slice(particles);
        debug_assert_eq!(
            bytes.len() as u64 % wgpu::MAP_ALIGNMENT,
            0,
            "particle map 0..need must be MAP_ALIGNMENT"
        );
        let hash = fnv1a64(bytes);
        if hash == self.particle_hash && particles.len() as u32 == self.particles.n {
            self.stats.particle_skipped += 1;
            return Ok(());
        }
        self.particle_hash = hash;
        let need = bytes.len() as u64;
        if need > self.particles.cap_bytes {
            self.particles.grow(&gpu.device, need);
            self.stats.particle_grows += 1;
        }
        // Callbacks run on submit/poll. Poll does not Wait; fallback is backpressure.
        gpu.device.poll(wgpu::Maintain::Poll);
        if let Some(i) = self.particles.pick_ready() {
            {
                let mut view = self.particles.staging[i]
                    .slice(0..need)
                    .get_mapped_range_mut();
                view.copy_from_slice(bytes);
            }
            self.particles.staging[i].unmap();
            self.particles.ready[i].store(false, Ordering::SeqCst);
            self.particles.pending.push((i, need));
            self.particles.cursor = i + 1;
        } else {
            gpu.queue.write_buffer(&self.particles.gpu, 0, bytes);
            self.stats.write_buffer_calls += 1;
            self.stats.particle_fallbacks += 1;
        }
        self.particles.n = particles.len() as u32;
        Ok(())
    }

    fn ensure_color_target(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        let ok = self
            .color
            .as_ref()
            .map(|c| c.size == (width, height))
            .unwrap_or(false);
        if !ok {
            self.color = Some(make_color_target(
                gpu,
                &self.blit_layout,
                &self.blit_sampler,
                self.surface_format,
                width,
                height,
                "scene-color",
                cfg!(feature = "capture"),
            ));
        }
        let need_resolve = cfg!(feature = "glow") || gpu.surface.is_none();
        if need_resolve {
            let ok = self
                .resolve
                .as_ref()
                .map(|c| c.size == (width, height))
                .unwrap_or(false);
            if !ok {
                self.resolve = Some(make_color_target(
                    gpu,
                    &self.blit_layout,
                    &self.blit_sampler,
                    self.surface_format,
                    width,
                    height,
                    "resolve-color",
                    cfg!(feature = "capture"),
                ));
            }
        }
    }

    pub fn render(
        &mut self,
        gpu: &mut GpuContext,
        camera: &Camera,
        vis: &VisualState,
        time: f32,
        grab: bool,
    ) -> Result<Option<CapturedFrame>> {
        let (width, height) = match gpu.config.as_ref() {
            Some(c) => (c.width, c.height),
            None => return Ok(None),
        };
        self.ensure_color_target(gpu, width, height);
        let Some(depth) = gpu.depth.as_ref() else {
            return Ok(None);
        };

        if !self.geo_queue.is_empty() {
            let inst = std::mem::take(&mut self.geo_queue);
            write_grow(
                gpu,
                &mut self.geo_inst,
                bytemuck::cast_slice(&inst),
                "geo-inst",
                &mut self.stats,
            );
            self.geo_count = inst.len() as u32;
        }

        let uniforms = FrameUniforms::new(
            camera.view(),
            camera.proj(),
            camera.eye(),
            camera.right(),
            camera.up(),
            time * vis.flux_speed,
            vis.pulse,
            vis.glow,
            vis.tube_radius,
            vis.aperture,
            vis.height_scale,
            vis.zener,
        );
        // Pending-writes stream. Flushed at the start of submit, before this encoder.
        gpu.queue
            .write_buffer(&self.frame.buffer, 0, bytemuck::bytes_of(&uniforms));
        self.stats.write_buffer_calls += 1;

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        let remap_slots: Vec<usize> = self
            .particles
            .pending
            .drain(..)
            .map(|(slot, bytes)| {
                encoder.copy_buffer_to_buffer(
                    &self.particles.staging[slot],
                    0,
                    &self.particles.gpu,
                    0,
                    bytes,
                );
                self.stats.ring_copies += 1;
                slot
            })
            .collect();

        {
            let color_view = &self.color.as_ref().unwrap().view;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            if self.face.count > 0 {
                pass.set_bind_group(0, &self.frame.group, &[]);
                pass.set_pipeline(&self.pipelines.face);
                pass.set_vertex_buffer(0, self.face.buf.slice(..));
                pass.set_vertex_buffer(1, self.orb.buf.slice(..));
                pass.draw(0..self.face.count, 0..self.n_orb_instances.max(1));
            }
            if self.geo_count > 0 && self.geo_face.count > 0 {
                pass.set_bind_group(0, &self.frame.group, &[]);
                pass.set_pipeline(&self.pipelines.face);
                pass.set_vertex_buffer(0, self.geo_face.buf.slice(..));
                pass.set_vertex_buffer(1, self.geo_inst.buf.slice(..));
                pass.draw(0..self.geo_face.count, 0..self.geo_count);
            }
            if self.line.count > 0 {
                pass.set_bind_group(0, &self.frame.group, &[]);
                pass.set_pipeline(&self.pipelines.line);
                pass.set_vertex_buffer(0, self.line.buf.slice(..));
                pass.set_vertex_buffer(1, self.orb.buf.slice(..));
                pass.draw(0..self.line.count, 0..self.n_orb_instances.max(1));
            }
            if self.static_fibers.n_fibers > 0 {
                draw_fibers(
                    &mut pass,
                    &self.frame.group,
                    &self.pipelines.fiber,
                    &self.static_fibers,
                );
            }
            if vis.show_rings && self.live.n_fibers > 0 {
                draw_fibers(
                    &mut pass,
                    &self.frame.group,
                    &self.pipelines.fiber,
                    &self.live,
                );
            }
            if self.hub.count > 0 {
                pass.set_bind_group(0, &self.frame.group, &[]);
                pass.set_pipeline(&self.pipelines.hub);
                pass.set_vertex_buffer(0, self.quad_vb.slice(..));
                pass.set_vertex_buffer(1, self.hub.buf.slice(..));
                pass.draw(0..6, 0..self.hub.count);
            }
            if self.particles.n > 0 {
                pass.set_bind_group(0, &self.frame.group, &[]);
                pass.set_pipeline(&self.pipelines.particle);
                pass.set_vertex_buffer(0, self.quad_vb.slice(..));
                pass.set_vertex_buffer(1, self.particles.gpu.slice(..));
                pass.draw(0..6, 0..self.particles.n);
            }
        }

        if self.hud.count > 0 {
            let color_view = &self.color.as_ref().unwrap().view;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipelines.hud);
            pass.set_vertex_buffer(0, self.hud.buf.slice(..));
            pass.draw(0..self.hud.count, 0..1);
        }

        let swap = if let Some(surface) = gpu.surface.as_ref() {
            match surface.get_current_texture() {
                Ok(f) => Some(f),
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    log::warn!("swapchain lost/outdated — reconfiguring");
                    gpu.reconfigure();
                    return Ok(None);
                }
                Err(wgpu::SurfaceError::Timeout) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        } else {
            None
        };

        let do_capture = grab && cfg!(feature = "capture");
        let headless = swap.is_none();
        let composite_offscreen = headless || do_capture && cfg!(feature = "glow");

        if let Some(frame) = swap.as_ref() {
            let swap_view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.composite(&mut encoder, Some(&swap_view), vis);
        } else if composite_offscreen || headless {
            self.composite(&mut encoder, None, vis);
        }

        if do_capture {
            let src = if cfg!(feature = "glow") {
                self.resolve.as_ref().or(self.color.as_ref())
            } else {
                self.color.as_ref()
            };
            if let Some(color) = src {
                if let Some(staging) = color.staging.as_ref() {
                    encoder.copy_texture_to_buffer(
                        wgpu::TexelCopyTextureInfo {
                            texture: &color.tex,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyBufferInfo {
                            buffer: staging,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(color.padded_bpr),
                                rows_per_image: Some(height),
                            },
                        },
                        wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }

        gpu.queue.submit(Some(encoder.finish()));
        // Reclaim after submit. Callback waits on GPU use of this slot (1–3 frames),
        // not on this call. Do not poll(Wait) here — that is the capture anti-pattern.
        // Cap is payload-sized; mapping 0..cap at 128 KiB is not a fat arena.
        let cap = self.particles.cap_bytes;
        for slot in remap_slots {
            let ready = self.particles.ready[slot].clone();
            self.particles.staging[slot].slice(0..cap).map_async(
                wgpu::MapMode::Write,
                move |res| {
                    ready.store(res.is_ok(), Ordering::SeqCst);
                },
            );
        }
        if let Some(frame) = swap {
            frame.present();
        }

        if !do_capture {
            return Ok(None);
        }

        let color = if cfg!(feature = "glow") {
            self.resolve.as_ref().or(self.color.as_ref())
        } else {
            self.color.as_ref()
        }
        .expect("color target");
        let Some(staging) = color.staging.as_ref() else {
            return Ok(None);
        };
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        gpu.device.poll(wgpu::Maintain::Wait);
        let mapped = slice.get_mapped_range();
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        let src_bpr = color.padded_bpr as usize;
        let dst_bpr = (width * 4) as usize;
        for y in 0..height as usize {
            let s = y * src_bpr;
            let d = y * dst_bpr;
            bgra[d..d + dst_bpr].copy_from_slice(&mapped[s..s + dst_bpr]);
        }
        drop(mapped);
        staging.unmap();
        Ok(Some(CapturedFrame {
            width,
            height,
            bgra,
        }))
    }

    fn composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        swap_view: Option<&wgpu::TextureView>,
        vis: &VisualState,
    ) {
        let scene_bg = &self.color.as_ref().unwrap().blit_bg;
        let dest = swap_view.or_else(|| self.resolve.as_ref().map(|r| &r.view));
        let Some(dest) = dest else {
            return;
        };
        let _ = vis;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dest,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        #[cfg(feature = "glow")]
        {
            pass.set_pipeline(&self.pipelines.post);
            pass.set_bind_group(0, &self.frame.group, &[]);
            pass.set_bind_group(1, scene_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        #[cfg(not(feature = "glow"))]
        {
            pass.set_pipeline(&self.pipelines.blit);
            pass.set_bind_group(0, scene_bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    pub fn particle_count(&self) -> u32 {
        self.particles.n
    }
    pub fn fiber_count(&self) -> u32 {
        self.live.n_fibers
    }
}

fn draw_fibers<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    frame_group: &'a wgpu::BindGroup,
    pipeline: &'a wgpu::RenderPipeline,
    slot: &'a FiberSlot,
) {
    let verts = slot
        .n_fibers
        .saturating_mul(slot.n_points)
        .saturating_mul(6);
    if verts == 0 {
        return;
    }
    pass.set_bind_group(0, frame_group, &[]);
    pass.set_bind_group(1, &slot.bg, &[]);
    pass.set_pipeline(pipeline);
    pass.draw(0..verts, 0..1);
}

struct PackedFibers {
    points: Vec<GpuFiberPoint>,
    n_fibers: u32,
    n_points: u32,
}

fn pack_fibers(fibers: &[GpuFiber]) -> PackedFibers {
    let nonempty: Vec<&GpuFiber> = fibers.iter().filter(|f| f.points.len() >= 2).collect();
    if nonempty.is_empty() {
        return PackedFibers {
            points: Vec::new(),
            n_fibers: 0,
            n_points: 0,
        };
    }
    let n_points = nonempty.iter().map(|f| f.points.len()).max().unwrap_or(0) as u32;
    let n_fibers = nonempty.len() as u32;
    let mut points = Vec::with_capacity((n_fibers * n_points) as usize);
    for (fi, f) in nonempty.iter().enumerate() {
        let n = f.points.len();
        for i in 0..n_points as usize {
            let src = i.min(n - 1);
            let along = src as f32 / n.max(1) as f32;
            points.push(GpuFiberPoint {
                pos: f.points[src].into(),
                along,
                color: f.color.into(),
                phase: along * std::f32::consts::TAU + fi as f32 * 0.37,
            });
        }
    }
    PackedFibers {
        points,
        n_fibers,
        n_points,
    }
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn vertex_dst() -> wgpu::BufferUsages {
    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
}

fn grow_init(
    device: &wgpu::Device,
    label: &str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
) -> GrowBuf {
    let cap = bytes.len() as u64;
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytes,
        usage,
    });
    GrowBuf { buf, cap, count: 0 }
}

/// Packed `Vec` → one `write_buffer`. A StagingBelt is only worth it if this
/// is called tens of times per present (HUD/hub storm), not for particles.
fn write_grow(
    gpu: &GpuContext,
    slot: &mut GrowBuf,
    bytes: &[u8],
    label: &str,
    stats: &mut UploadStats,
) {
    if bytes.is_empty() {
        slot.count = 0;
        return;
    }
    let need = bytes.len() as u64;
    if slot.cap < need {
        let mut cap = slot.cap.max(64).max(need);
        while cap < need {
            cap *= 2;
        }
        slot.buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap,
            usage: vertex_dst(),
            mapped_at_creation: false,
        });
        slot.cap = cap;
        stats.fiber_reallocs += 1;
    }
    gpu.queue.write_buffer(&slot.buf, 0, bytes);
    stats.write_buffer_calls += 1;
}

fn make_fiber_slot(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    dummy: &[GpuFiberPoint],
    label: &str,
) -> FiberSlot {
    let points = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(dummy),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let meta = FiberMeta {
        n_points: 1,
        n_fibers: 0,
        radius: 0.0,
        _pad: 0,
    };
    let meta_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fiber-meta"),
        contents: bytemuck::bytes_of(&meta),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bg = bind_fiber(device, layout, &meta_buf, &points);
    FiberSlot {
        points,
        meta: meta_buf,
        bg,
        cap_points: dummy.len() as u32,
        n_points: 0,
        n_fibers: 0,
        hash: 0,
        radius: 0.0,
    }
}

fn bind_fiber(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    meta: &wgpu::Buffer,
    points: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fiber-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: meta.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: points.as_entire_binding(),
            },
        ],
    })
}

impl ParticleRing {
    /// 4096 particles × 32 B. Next-pow2 grow; cap stays MAP_ALIGNMENT (8).
    const MIN_CAP: u64 = 4096 * 32;

    fn new(device: &wgpu::Device, cap_bytes: u64) -> Self {
        let cap_bytes = cap_bytes.max(Self::MIN_CAP);
        let mk = |i: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("part-stage-{i}")),
                size: cap_bytes,
                usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: true,
            })
        };
        let gpu = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part-gpu"),
            size: cap_bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            staging: [mk(0), mk(1), mk(2)],
            ready: [
                Arc::new(AtomicBool::new(true)),
                Arc::new(AtomicBool::new(true)),
                Arc::new(AtomicBool::new(true)),
            ],
            gpu,
            cap_bytes,
            cursor: 0,
            pending: Vec::new(),
            n: 0,
        }
    }

    fn pick_ready(&self) -> Option<usize> {
        for k in 0..3 {
            let i = (self.cursor + k) % 3;
            if self.ready[i].load(Ordering::SeqCst) {
                return Some(i);
            }
        }
        None
    }

    fn grow(&mut self, device: &wgpu::Device, need: u64) {
        let mut cap = self.cap_bytes.max(need);
        while cap < need {
            cap *= 2;
        }
        // Dest VB is replaced; in-flight copies to the old VB cannot land.
        device.poll(wgpu::Maintain::Wait);
        for i in 0..3 {
            if self.ready[i].load(Ordering::SeqCst) {
                self.staging[i].unmap();
                self.ready[i].store(false, Ordering::SeqCst);
            }
        }
        let mk = |i: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("part-stage-{i}")),
                size: cap,
                usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: true,
            })
        };
        self.staging = [mk(0), mk(1), mk(2)];
        self.ready = [
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(true)),
        ];
        self.gpu = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part-gpu"),
            size: cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.cap_bytes = cap;
        self.pending.clear();
        self.cursor = 0;
    }
}

#[allow(clippy::too_many_arguments)]
fn make_color_target(
    gpu: &GpuContext,
    blit_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    label: &str,
    with_staging: bool,
) -> ColorTarget {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let blit_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit-bg"),
        layout: blit_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    // Texture copies only: 256 B rows. Not COPY_BUFFER_ALIGNMENT / MAP_ALIGNMENT.
    let padded_bpr = crate::types::copy_bytes_per_row_bgra(width);
    let staging = if with_staging {
        Some(gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture-staging"),
            size: padded_bpr as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }))
    } else {
        None
    };
    ColorTarget {
        view,
        tex,
        blit_bg,
        staging,
        size: (width, height),
        padded_bpr,
    }
}

fn uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(
    binding: u32,
    vis: wgpu::ShaderStages,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn fiber_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    frame_layout: &wgpu::BindGroupLayout,
    fiber_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fiber"),
        bind_group_layouts: &[frame_layout, fiber_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fiber"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(additive()),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn alpha_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
    buffers: &[wgpu::VertexBufferLayout],
    depth_write: bool,
    depth_bias: i32,
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: depth_write,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: depth_bias,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn color_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    layouts: &[&wgpu::BindGroupLayout],
    format: wgpu::TextureFormat,
    depth: bool,
    additive_blend: bool,
    buffers: &[wgpu::VertexBufferLayout],
    vs: &'static str,
    fs: &'static str,
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: layouts,
        push_constant_ranges: &[],
    });
    let blend = if additive_blend {
        additive()
    } else {
        wgpu::BlendState::REPLACE
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: if depth {
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: !additive_blend,
                depth_compare: if additive_blend {
                    wgpu::CompareFunction::LessEqual
                } else {
                    wgpu::CompareFunction::Less
                },
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        } else {
            None
        },
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
