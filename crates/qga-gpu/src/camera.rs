use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Orbit,
    Fly,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub mode: CameraMode,
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub eye: Vec3,
    pub fovy: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub fly_speed: f32,
    /// Slow yaw crane. User look/zoom turns it off.
    pub cinematic: bool,
}

impl Camera {
    pub fn orbit(target: Vec3, distance: f32) -> Self {
        Self {
            mode: CameraMode::Orbit,
            target,
            distance,
            yaw: 0.55,
            pitch: 0.42,
            eye: target + Vec3::new(0.0, distance * 0.4, distance),
            fovy: 55.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
            near: 0.05,
            far: 800.0,
            fly_speed: 8.0,
            cinematic: false,
        }
    }

    /// Generic orbit crane. Not a scene tour.
    pub fn tick_cinematic(&mut self, dt: f32) {
        if !self.cinematic || self.mode != CameraMode::Orbit {
            return;
        }
        self.yaw += 0.14 * dt;
    }

    pub fn eye(&self) -> Vec3 {
        match self.mode {
            CameraMode::Orbit => {
                let cp = self.pitch.cos();
                self.target
                    + Vec3::new(
                        self.distance * self.yaw.cos() * cp,
                        self.distance * self.pitch.sin(),
                        self.distance * self.yaw.sin() * cp,
                    )
            }
            CameraMode::Fly => self.eye,
        }
    }

    pub fn forward(&self) -> Vec3 {
        match self.mode {
            CameraMode::Orbit => (self.target - self.eye()).normalize_or_zero(),
            CameraMode::Fly => {
                let cp = self.pitch.cos();
                Vec3::new(self.yaw.cos() * cp, self.pitch.sin(), self.yaw.sin() * cp)
            }
        }
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize_or_zero()
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize_or_zero()
    }

    pub fn view(&self) -> Mat4 {
        let eye = self.eye();
        let target = match self.mode {
            CameraMode::Orbit => self.target,
            CameraMode::Fly => eye + self.forward(),
        };
        Mat4::look_at_rh(eye, target, Vec3::Y)
    }

    pub fn proj(&self) -> Mat4 {
        Mat4::perspective_rh(self.fovy, self.aspect.max(0.05), self.near, self.far)
    }

    pub fn orbit_delta(&mut self, dx: f32, dy: f32) {
        self.cinematic = false;
        self.yaw += dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.45, 1.45);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.cinematic = false;
        match self.mode {
            CameraMode::Orbit => {
                self.distance = (self.distance * (1.0 - delta * 0.08)).clamp(0.4, 160.0);
            }
            CameraMode::Fly => {
                self.eye += self.forward() * delta * 0.6;
            }
        }
    }

    pub fn fly_move(&mut self, wish: Vec3, dt: f32) {
        if self.mode != CameraMode::Fly {
            return;
        }
        let f = self.forward();
        let r = self.right();
        let dir = (r * wish.x + Vec3::Y * wish.y + f * wish.z).normalize_or_zero();
        self.eye += dir * self.fly_speed * dt;
    }
}
