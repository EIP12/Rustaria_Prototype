use wgpu::util::DeviceExt;

use crate::camera;

/// Calcule le LightUniform pour un instant du cycle jour/nuit.
/// `time` : 0.0 = aube, 0.25 = midi, 0.5 = crépuscule, 0.75 = minuit
pub fn day_night_light(time: f32) -> [f32; 8] {
    use std::f32::consts::TAU;

    let angle = time * TAU;
    let sun_x = angle.cos();
    let sun_y = angle.sin();

    let day     = sun_y.max(0.0).powf(0.4);
    let ambient = 0.03 + day * 0.15;
    let warmth  = (1.0 - (sun_y - 0.7).abs().min(1.0)).max(0.0);
    let d       = day.clamp(0.0, 1.0);

    [
        sun_x, sun_y, 0.3,          // sun_dir
        ambient,                     // ambient
        (0.6 + warmth * 0.4) * d,   // sun_color.r
        (0.6 + warmth * 0.1) * d,   // sun_color.g
        (0.5 - warmth * 0.3) * d,   // sun_color.b
        0.0,                         // _padding
    ]
}

// ─────────────────────────────────────────────
/// Couleur de fond cohérente avec le cycle jour/nuit.
pub fn sky_color(time: f32) -> wgpu::Color {
    use std::f32::consts::TAU;
    let angle  = time * TAU;
    let day    = angle.sin().max(0.0).powf(0.4) as f64;
    let s      = angle.sin() as f64;
    let warmth = ((1.0 - (s - 0.5).abs() * 2.0).max(0.0)).powf(2.0);

    wgpu::Color {
        r: 0.004 + day * (0.3 + warmth * 0.5),
        g: 0.016 + day * (0.55 - warmth * 0.1),
        b: 0.035 + day * (0.9 - warmth * 0.6),
        a: 1.0,
    }
}

// Layout du vertex buffer — stride = 40 bytes (10 x f32)
// Correspond à la struct Vertex de rustaria-core :
//   position : vec3  (offset  0)
//   color    : vec3  (offset 12)
//   normal   : vec3  (offset 24)
//   ao       : f32   (offset 36)
fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    const F: wgpu::BufferAddress = std::mem::size_of::<f32>() as wgpu::BufferAddress;
    wgpu::VertexBufferLayout {
        array_stride: F * 10,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { offset: 0,      shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: F *  3, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: F *  6, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: F *  9, shader_location: 3, format: wgpu::VertexFormat::Float32   },
        ],
    }
}

/// Regroupe tout ce que `create_pipeline` retourne.
pub struct PipelineBundle {
    pub fill_pipeline:         wgpu::RenderPipeline,
    pub wireframe_pipeline:    wgpu::RenderPipeline,
    pub chunk_border_pipeline: wgpu::RenderPipeline,
    pub camera_bind_group:     wgpu::BindGroup,
    pub camera_buffer:         wgpu::Buffer,
    pub light_buffer:          wgpu::Buffer,
}

/// Crée le pipeline de rendu, les buffers uniforms et le bind group.
pub fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> PipelineBundle {
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

    // ── Uniform buffer caméra (MVP fixe) — délégué à camera.rs ─────────
    let camera_buffer = camera::build_camera_buffer(device, width, height);

    let light_data: [f32; 8] = day_night_light(0.25); // valeurs initiales : midi
    let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Light Uniform Buffer"),
        contents: bytemuck::cast_slice(&light_data),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let uniform_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Camera+Light Bind Group Layout"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::VERTEX),
            uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
        ],
    });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Camera+Light Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: light_buffer.as_entire_binding() },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                // En wireframe on désactive le culling pour voir toutes les arêtes
                cull_mode: (polygon_mode == wgpu::PolygonMode::Fill).then_some(wgpu::Face::Back),
                polygon_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    };

    let fill_pipeline      = make_pipeline(wgpu::PolygonMode::Fill, "Fill Pipeline");
    let wireframe_pipeline = make_pipeline(wgpu::PolygonMode::Line, "Wireframe Pipeline");

    // Chunk border pipeline: LineList topology, unlit fragment shader, no depth write
    let chunk_border_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Chunk Border Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[vertex_buffer_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_unlit",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false, // borders don't occlude terrain
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    PipelineBundle { fill_pipeline, wireframe_pipeline, chunk_border_pipeline, camera_bind_group, camera_buffer, light_buffer }
}

/// Crée le depth buffer (Depth32Float), à recréer à chaque resize.
pub fn create_depth_texture_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
    .create_view(&wgpu::TextureViewDescriptor::default())
}
