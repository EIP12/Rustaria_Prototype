use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

// ─────────────────────────────────────────────
// Uniform caméra : la matrice view_proj envoyée au vertex shader
// Caméra FIXE pour l'alpha : calculée une seule fois, jamais mise à jour
// ─────────────────────────────────────────────
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    fn new(width: u32, height: u32) -> Self {
        let aspect = width as f32 / height as f32;

        // View : caméra placée en (3, 2, 3) regardant vers (0, 0.5, 0)
        // Légèrement en hauteur pour voir les 3 faces du cube
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(3.0, 2.0, 3.0),  // position caméra
            glam::Vec3::new(0.0, 0.5, 0.0),  // cible
            glam::Vec3::Y,                    // vecteur "haut"
        );

        // Projection perspective, FOV 70°
        let proj = glam::Mat4::perspective_rh(
            f32::to_radians(70.0), // fov vertical
            aspect,
            0.1,   // near plane
            100.0, // far plane
        );

        Self {
            view_proj: (proj * view).to_cols_array_2d(),
        }
    }
}

// ─────────────────────────────────────────────
// Layout du vertex buffer
// Doit correspondre EXACTEMENT à la struct Vertex de rustaria-core :
//   position : [f32; 3]  → location 0
//   color    : [f32; 3]  → location 1
// ─────────────────────────────────────────────
fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress, // 24 bytes
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            // @location(0) position
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            // @location(1) color
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress, // 12 bytes
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
        ],
    }
}

// ─────────────────────────────────────────────
// create_pipeline : pipeline de rendu + bind group caméra
// Retourne les deux car main.rs a besoin des deux séparément
// ─────────────────────────────────────────────
pub fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::RenderPipeline, wgpu::BindGroup) {
    // ── Shader WGSL ──────────────────────────────────────────────────────
    // learn-wgpu tuto 3 : les deux entry points doivent avoir des noms différents
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

    // ── Uniform buffer caméra (MVP fixe) ────────────────────────────────
    let camera_uniform = CameraUniform::new(width, height);
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[camera_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // ── Bind group layout : décrit ce que le shader attend en @group(0) ─
    let camera_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX, // seulement le vertex shader en a besoin
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

    // ── Bind group : lie le buffer concret au layout ─────────────────────
    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Camera Bind Group"),
        layout: &camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    // ── Pipeline layout ──────────────────────────────────────────────────
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&camera_bind_group_layout],
        push_constant_ranges: &[],
    });

    // ── RenderPipeline : assemble tout ──────────────────────────────────
    // learn-wgpu tuto 3 : chaque champ est expliqué
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&pipeline_layout),

        // Vertex shader
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[vertex_buffer_layout()], // layout de notre struct Vertex
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },

        // Fragment shader
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),

        // Triangles, face avant = sens anti-horaire (CCW), back-face culling activé
        // learn-wgpu tuto 3 : FrontFace::Ccw correspond à nos indices mesh.rs
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },

        // Depth buffer : INDISPENSABLE pour le rendu 3D
        // Sans ça les faces se dessinent dans le mauvais ordre (z-fighting)
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),

        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },

        multiview: None,
        cache: None,
    });

    (render_pipeline, camera_bind_group)
}

// ─────────────────────────────────────────────
// create_depth_texture_view
// Crée le depth buffer (texture séparée Depth32Float)
// Doit être recréé à chaque resize car il doit avoir la même taille que la surface
// ─────────────────────────────────────────────
pub fn create_depth_texture_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
}
