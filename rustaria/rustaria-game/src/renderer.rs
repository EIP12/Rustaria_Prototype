use std::sync::Arc;
use winit::window::Window;

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub is_surface_configured: bool, // pattern learn-wgpu tuto 2 : false au départ
    pub window: Arc<Window>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        // ── 1. Instance : point d'entrée, choisit le backend (Vulkan/Metal/DX12)
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // ── 2. Surface : zone de rendu attachée à la fenêtre winit
        let surface = instance
            .create_surface(window.clone())
            .expect("Impossible to create surface");

        // ── 3. Adapter : handle du GPU physique
        // On prend le premier compatible avec notre surface
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No compatible GPU found — please check your drivers");

        // ── 4. Device + Queue : connexion logique au GPU
        //   Device  = crée les ressources (buffers, textures, pipelines)
        //   Queue   = envoie les commandes au GPU
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Rustaria Device"),
                    // POLYGON_MODE_LINE requis pour le wireframe debug (touche G)
                    required_features: wgpu::Features::POLYGON_MODE_LINE,
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                },
                None,
            )
            .await
            .expect("Impossible to create device");

        // ── 5. Configuration de la surface
        // On cherche un format sRGB (rendu des couleurs correct), sinon le premier dispo
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, // VSync : garanti sur toutes les plateformes
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // On NE configure PAS la surface ici.
        // Elle sera configurée dans resize() au premier WindowEvent::Resized.
        // C'est le pattern exact du tuto learn-wgpu tuto 2 :
        // is_surface_configured reste false jusqu'au premier resize.

        Self {
            surface,
            device,
            queue,
            config,
            is_surface_configured: false,
            window,
        }
    }

    // Appelé à chaque WindowEvent::Resized
    // OBLIGATOIRE même si on ne redimensionne pas :
    // c'est ici que la surface est vraiment configurée pour la première fois
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true; // ← débloque le rendu dans main.rs
        }
    }
}
