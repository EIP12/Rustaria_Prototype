// shader.wgsl
// learn-wgpu tuto 3 : les entry points vs_main et fs_main doivent avoir des noms différents

// Uniform caméra : matrice view_proj envoyée depuis pipeline.rs
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Entrée du vertex shader — correspond au layout de Vertex dans mesh.rs
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

// Sortie du vertex shader / entrée du fragment shader
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

// Vertex shader : applique la matrice MVP, passe la couleur
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

// Fragment shader : retourne la couleur telle quelle (déjà modulée dans mesh.rs)
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
