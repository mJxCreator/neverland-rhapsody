struct SceneUniforms {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: SceneUniforms;

@group(0) @binding(1)
var atlas_texture: texture_2d<f32>;

@group(0) @binding(2)
var atlas_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 图集提供方块基色，顶点色只保留朝向明暗与稳定变化。
    let sampled = textureSample(atlas_texture, atlas_sampler, input.uv);
    // 交叉植被使用带透明边缘的纹理，低 Alpha 像素直接丢弃。
    if (sampled.a < 0.5) {
        discard;
    }
    return vec4<f32>(sampled.rgb * input.color, 1.0);
}
