//! Unused. Fiber conversion lives in the consumer, not this crate.
//! Left as a reference; not compiled (no `qga-math` feature).
//! Software fact: only GPU-visible fields (points, color) are copied.

use crate::types::GpuFiber;

impl From<&qga_math::Fiber> for GpuFiber {
    fn from(f: &qga_math::Fiber) -> Self {
        GpuFiber {
            points: f.points.clone(),
            color: f.color,
        }
    }
}

impl From<qga_math::Fiber> for GpuFiber {
    fn from(f: qga_math::Fiber) -> Self {
        GpuFiber {
            points: f.points,
            color: f.color,
        }
    }
}
