// 模型着色器：实例世界变换 + 基础颜色纹理 + 简单方向光。
// 骨骼蒙皮在 CPU 端完成，这里接收已变形的局部空间顶点。

struct Scene {
    view_proj: mat4x4<f32>,
};

struct ModelParams {
    model: mat4x4<f32>,
    // x = Alpha Mask 阈值，其余分量保留。
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene: Scene;
@group(1) @binding(0) var<uniform> model: ModelParams;
@group(1) @binding(1) var model_texture: texture_2d<f32>;
@group(1) @binding(2) var model_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world = model.model * vec4<f32>(input.position, 1.0);
    output.clip_position = scene.view_proj * world;
    output.uv = input.uv;
    output.color = input.color;
    output.normal = normalize((model.model * vec4<f32>(input.normal, 0.0)).xyz);
    return output;
}

fn shade(input: VertexOutput, texel: vec4<f32>) -> vec4<f32> {
    let color = input.color * texel;
    let light_dir = normalize(vec3<f32>(0.35, 0.9, 0.25));
    let lambert = 0.55 + 0.45 * max(dot(normalize(input.normal), light_dir), 0.0);
    return vec4<f32>(color.rgb * lambert, color.a);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return shade(input, textureSample(model_texture, model_sampler, input.uv));
}

@fragment
fn fs_mask(input: VertexOutput) -> @location(0) vec4<f32> {
    let shaded = shade(input, textureSample(model_texture, model_sampler, input.uv));
    if (shaded.a < model.params.x) {
        discard;
    }
    return shaded;
}
