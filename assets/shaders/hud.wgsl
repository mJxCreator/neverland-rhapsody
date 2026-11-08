// 屏幕空间 HUD：顶点位置直接使用裁剪空间坐标，颜色携带透明度。
// UV 为负值时表示纯色顶点；否则由 atlas 选择物品图集或 UI 图集采样。

struct SceneUniforms {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: SceneUniforms;

@group(0) @binding(1)
var item_atlas: texture_2d<f32>;

@group(0) @binding(2)
var item_sampler: sampler;

@group(0) @binding(3)
var ui_atlas: texture_2d<f32>;

@group(0) @binding(4)
var ui_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) atlas: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) atlas: f32,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    output.atlas = input.atlas;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 两种图集都无条件采样，避免在非一致控制流中调用带导数的 textureSample。
    let item_sample = textureSample(item_atlas, item_sampler, input.uv);
    let ui_sample = textureSample(ui_atlas, ui_sampler, input.uv);
    let sampled = select(item_sample, ui_sample, input.atlas > 0.5);
    let textured = sampled * input.color;
    // UV 为负值是纯色顶点，直接使用顶点颜色。
    return select(textured, input.color, input.uv.x < 0.0);
}
