use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

// ─────────────────────────────────────────────
// CameraUniform : la matrice view_proj envoyée au vertex shader (Option B)
// Mise à jour chaque frame via Camera::upload()
// ─────────────────────────────────────────────
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

// ─────────────────────────────────────────────
// Camera : position + orientation libre (Option B)
//
// Système de coordonnées :
//   yaw   = rotation autour de Y (gauche/droite), en radians
//   pitch = rotation autour de X (haut/bas), en radians, clampé ±89°
//
// Contrôles :
//   ZQSD / WASD  → déplacement dans le plan horizontal
//   Espace       → montée
//   Shift        → descente
//   Souris       → orientation (yaw/pitch)
// ─────────────────────────────────────────────
pub struct Camera {
    pub position: glam::Vec3,
    pub yaw: f32,   // radians
    pub pitch: f32, // radians

    // Projection (inchangée sauf au resize)
    aspect: f32,
    fov_y: f32,
    near: f32,
    far: f32,
}

impl Camera {
    /// Crée la caméra avec une position et une orientation initiales
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            // Position initiale : centre du monde (16*16/2=128), au-dessus du terrain
            position: glam::Vec3::new(128.0, 60.0, 128.0),
            yaw: f32::to_radians(180.0),
            pitch: f32::to_radians(-20.0),

            aspect: width as f32 / height as f32,
            fov_y: f32::to_radians(70.0),
            near: 0.1,
            far: 512.0,
        }
    }

    /// Met à jour le ratio d'aspect lors d'un resize
    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
    }

    /// Calcule le vecteur "avant" (direction de regard) à partir de yaw/pitch
    pub fn forward(&self) -> glam::Vec3 {
        glam::Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Vecteur "droite" (perpendiculaire au forward dans le plan horizontal)
    pub fn right(&self) -> glam::Vec3 {
        self.forward().cross(glam::Vec3::Y).normalize()
    }

    /// Recalcule la matrice view_proj et renvoie un CameraUniform prêt à uploader
    pub fn build_uniform(&self) -> CameraUniform {
        let view = glam::Mat4::look_at_rh(
            self.position,
            self.position + self.forward(),
            glam::Vec3::Y,
        );
        let proj =
            glam::Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);

        CameraUniform {
            view_proj: (proj * view).to_cols_array_2d(),
        }
    }

    /// Met à jour position + orientation depuis l'InputState du frame courant
    /// À appeler une fois par frame dans render(), avant upload()
    pub fn update(&mut self, input: &crate::input::InputState) {
        const MOVE_SPEED: f32 = 0.6;
        const MOUSE_SENSITIVITY: f32 = 0.002;

        let forward = self.forward();
        let right = self.right();

        if input.forward  { self.position += forward * MOVE_SPEED; }
        if input.backward { self.position -= forward * MOVE_SPEED; }
        if input.right    { self.position += right   * MOVE_SPEED; }
        if input.left     { self.position -= right   * MOVE_SPEED; }
        if input.up       { self.position.y += MOVE_SPEED; }
        if input.down     { self.position.y -= MOVE_SPEED; }

        if input.mouse_captured {
            self.yaw   += input.mouse_dx * MOUSE_SENSITIVITY;
            self.pitch -= input.mouse_dy * MOUSE_SENSITIVITY; // dy inversé : souris haut = regard haut
            self.pitch  = self.pitch.clamp(
                f32::to_radians(-89.0),
                f32::to_radians(89.0),
            );
        }
    }

    /// Écrit la view_proj courante dans le GPU uniform buffer
    pub fn upload(&self, queue: &wgpu::Queue, buffer: &wgpu::Buffer) {
        let uniform = self.build_uniform();
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&[uniform]));
    }
}

// ─────────────────────────────────────────────
// build_camera_buffer : crée le GPU uniform buffer (COPY_DST pour les updates)
// Retourne le buffer prêt à être lié dans un bind group
// ─────────────────────────────────────────────
pub fn build_camera_buffer(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
    let camera = Camera::new(width, height);
    let uniform = camera.build_uniform();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}
