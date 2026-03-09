use crate::camera;

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
// PipelineBundle : regroupe tout ce que create_pipeline retourne
// Évite un tuple de 4 éléments fragile — ajouter un champ ne casse aucun appelant
// ─────────────────────────────────────────────
pub struct PipelineBundle {
    pub fill_pipeline: wgpu::RenderPipeline,
    pub wireframe_pipeline: wgpu::RenderPipeline,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_buffer: wgpu::Buffer,
}

// ─────────────────────────────────────────────
// create_pipeline : pipeline de rendu + bind group caméra + camera buffer
// Retourne un PipelineBundle — main.rs déstructure ce qu'il lui faut
// ─────────────────────────────────────────────
pub fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> PipelineBundle {
    // ── Shader WGSL ──────────────────────────────────────────────────────
    // learn-wgpu tuto 3 : les deux entry points doivent avoir des noms différents
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

    // ── Uniform buffer caméra (MVP fixe) — délégué à camera.rs ─────────
    let camera_buffer = camera::build_camera_buffer(device, width, height);

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

    // ── Helper closure pour créer un pipeline avec un polygon_mode donné ──
    let make_pipeline = |polygon_mode: wgpu::PolygonMode, label: &'static str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
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
            // Wireframe = PolygonMode::Line, rendu normal = Fill
            // Le back-face culling est désactivé en wireframe pour voir toutes les arêtes
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: if polygon_mode == wgpu::PolygonMode::Fill {
                    Some(wgpu::Face::Back)
                } else {
                    None // En wireframe on veut voir toutes les arêtes
                },
                polygon_mode,
                unclipped_depth: false,
                conservative: false,
            },
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
        })
    };

    // ── Pipeline normal (rendu rempli) ───────────────────────────────────
    let fill_pipeline = make_pipeline(wgpu::PolygonMode::Fill, "Fill Pipeline");

    // ── Pipeline wireframe (grille debug, touche G) ──────────────────────
    // Requiert wgpu::Features::POLYGON_MODE_LINE activé dans renderer.rs
    let wireframe_pipeline = make_pipeline(wgpu::PolygonMode::Line, "Wireframe Pipeline");

    PipelineBundle {
        fill_pipeline,
        wireframe_pipeline,
        camera_bind_group,
        camera_buffer,
    }
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
