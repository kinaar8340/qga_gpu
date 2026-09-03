use anyhow::{Context, Result};
use wgpu::TextureView;

#[cfg(feature = "winit")]
use std::sync::Arc;
#[cfg(feature = "winit")]
use winit::window::Window;

pub struct DepthTarget {
    pub view: TextureView,
    pub tex: wgpu::Texture,
    pub size: (u32, u32),
}

pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
    pub surface: Option<wgpu::Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    pub depth: Option<DepthTarget>,
    pub present_mailbox: bool,
}

impl GpuContext {
    pub fn request_instance() -> wgpu::Instance {
        wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::empty(),
            ..Default::default()
        })
    }

    pub fn init_headless() -> Result<Self> {
        Self::init_headless_extent(1920, 1080)
    }

    pub fn init_headless_extent(width: u32, height: u32) -> Result<Self> {
        let instance = Self::request_instance();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .context("no Vulkan adapter (is the NVIDIA ICD visible?)")?;
        let mut ctx = Self::from_adapter(instance, adapter)?;
        ctx.configure_offscreen(width.max(1), height.max(1));
        Ok(ctx)
    }

    #[cfg(feature = "winit")]
    pub fn init_windowed(window: Arc<Window>) -> Result<Self> {
        let instance = Self::request_instance();
        let surface = instance
            .create_surface(window.clone())
            .context("create Wayland/Vulkan surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .context("no Vulkan adapter compatible with the window")?;
        let mut ctx = Self::from_adapter(instance, adapter)?;
        let size = window.inner_size();
        ctx.configure_surface(surface, size.width.max(1), size.height.max(1))?;
        Ok(ctx)
    }

    fn from_adapter(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Result<Self> {
        let info = adapter.get_info();
        log::info!(
            "GPU adapter: {} ({:?}, vendor {:#x} device {:#x})",
            info.name,
            info.backend,
            info.vendor,
            info.device
        );
        let mut limits = adapter.limits();
        limits.max_compute_workgroup_size_x = limits.max_compute_workgroup_size_x.max(256);
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("qga-gpu"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .context("request_device")?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            adapter_info: info,
            surface: None,
            config: None,
            depth: None,
            present_mailbox: false,
        })
    }

    /// Offscreen color size + depth. No swapchain. Software fact: headless still draws.
    pub fn configure_offscreen(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        self.rebuild_depth(width, height);
        self.config = Some(config);
        self.surface = None;
    }

    #[cfg(feature = "winit")]
    pub fn configure_surface(
        &mut self,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let caps = surface.get_capabilities(&self.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| *f == wgpu::TextureFormat::Bgra8UnormSrgb)
            .or_else(|| caps.formats.iter().copied().find(|f| f.is_srgb()))
            .unwrap_or(caps.formats[0]);
        // FIFO is the reliable path on NVIDIA + GNOME Wayland. Mailbox has been
        // presenting empty frames on driver 580 (and wgpu 24 warns on
        // FIFO_LATEST_READY_EXT = 1000361000). Software fact.
        let mailbox = caps.present_modes.contains(&wgpu::PresentMode::Mailbox);
        let present_mode = wgpu::PresentMode::Fifo;
        let alpha = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            caps.alpha_modes[0]
        };
        log::info!(
            "surface format={format:?} alpha={alpha:?} present={present_mode:?} modes={:?}",
            caps.present_modes
        );
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: alpha,
            view_formats: vec![],
        };
        surface.configure(&self.device, &config);
        self.present_mailbox = mailbox;
        self.rebuild_depth(config.width, config.height);
        self.config = Some(config);
        self.surface = Some(surface);
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let Some(config) = self.config.as_ref() {
            if config.width == width && config.height == height {
                return;
            }
        }
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
            self.rebuild_depth(width, height);
            return;
        }
        if self.surface.is_none() {
            if let Some(config) = self.config.as_mut() {
                config.width = width;
                config.height = height;
            }
            self.rebuild_depth(width, height);
        }
    }

    /// Re-apply the current surface config (Lost / Outdated).
    pub fn reconfigure(&mut self) {
        let (width, height) = match self.config.as_ref() {
            Some(c) => (c.width, c.height),
            None => return,
        };
        if let (Some(surface), Some(config)) = (&self.surface, &self.config) {
            surface.configure(&self.device, config);
        }
        self.rebuild_depth(width, height);
    }

    pub fn toggle_present_mode(&mut self) {
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let Some(config) = self.config.as_mut() else {
            return;
        };
        config.present_mode = match config.present_mode {
            wgpu::PresentMode::Fifo if self.present_mailbox => wgpu::PresentMode::Mailbox,
            _ => wgpu::PresentMode::Fifo,
        };
        surface.configure(&self.device, config);
        log::info!("present mode {:?}", config.present_mode);
    }

    fn rebuild_depth(&mut self, width: u32, height: u32) {
        let depth_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth = Some(DepthTarget {
            view: depth_tex.create_view(&wgpu::TextureViewDescriptor::default()),
            tex: depth_tex,
            size: (width, height),
        });
    }

    pub fn report(&self) -> String {
        format!(
            "{} | {:?} | Vulkan | {}x{} | mailbox={} | surface={}",
            self.adapter_info.name,
            self.adapter_info.device_type,
            self.config.as_ref().map(|c| c.width).unwrap_or(0),
            self.config.as_ref().map(|c| c.height).unwrap_or(0),
            self.present_mailbox,
            self.surface.is_some(),
        )
    }
}
