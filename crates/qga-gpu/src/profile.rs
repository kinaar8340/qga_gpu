//! Hardware profiles. Lab (4090) is one target, not the only box.

#[derive(Clone, Copy, Debug)]
pub struct HardwareProfile {
    pub name: &'static str,
    pub cpu_threads: u32,
    pub vram_mib: u32,
    pub default_fibers: u32,
    pub default_points: u32,
    pub particle_cap: u32,
    pub tube_radius: f32,
}

impl HardwareProfile {
    /// Lab target: Ryzen 9 3900X (24 threads) + RTX 4090 24 GiB.
    /// Not the laptop demo and not CI.
    pub const LAB_4090: Self = Self {
        name: "lab: RTX 4090 + Ryzen 9 3900X",
        cpu_threads: 24,
        vram_mib: 24564,
        default_fibers: 256,
        default_points: 192,
        particle_cap: 65_536,
        tube_radius: 0.042,
    };
}
