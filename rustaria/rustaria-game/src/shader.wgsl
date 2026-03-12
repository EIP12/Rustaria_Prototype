struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct LightUniform {
    sun_dir:   vec3<f32>,
    ambient:   f32,
    sun_color: vec3<f32>,
    _padding:  f32,
};
@group(0) @binding(1)
var<uniform> light: LightUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
    @location(2) normal:   vec3<f32>,
    @location(3) ao:       f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color:  vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) ao:     f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color  = in.color;
    out.normal = in.normal;
    out.ao     = in.ao;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ambient     = light.ambient;
    let diffuse     = max(dot(in.normal, light.sun_dir), 0.0);
    let light_total = ambient + diffuse * 0.8;
    let final_color = in.color * light_total * in.ao;
    return vec4<f32>(final_color, 1.0);
}

// Unlit fragment shader — used for debug overlays (chunk borders, etc.)
@fragment
fn fs_unlit(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}