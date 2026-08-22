struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vertex_main(
    @builtin(vertex_index) vid : u32
) -> VertexOutput {
    return VertexOutput(
        array<vec4f, 6>(
            vec4f( 1.0,  1.0, 0, 1), 
            vec4f( 1.0, -1.0, 0, 1), 
            vec4f(-1.0,  1.0, 0, 1), 
            vec4f( 1.0, -1.0, 0, 1), 
            vec4f(-1.0,  1.0, 0, 1), 
            vec4f(-1.0, -1.0, 0, 1), 
        )[vid],
        array<vec2f, 6>(
            vec2f(1.0, 0.0), 
            vec2f(1.0, 1.0), 
            vec2f(0.0, 0.0), 
            vec2f(1.0, 1.0), 
            vec2f(0.0, 0.0), 
            vec2f(0.0, 1.0), 
        )[vid]
    );
}

@group(0) @binding(0) var my_texture: texture_2d<f32>;
@group(0) @binding(1) var my_sampler: sampler;

@fragment
fn fragment_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(my_texture, my_sampler, in.uv);
}
