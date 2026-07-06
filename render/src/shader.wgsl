struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    let x = f32(i32(in_vertex_index) - 1);
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1);

    var color = vec3<f32>(0.0);
    if (in_vertex_index == 0u) {
        color = vec3<f32>(1.0, 0.0, 0.0);
    } else if (in_vertex_index == 1u) {
        color = vec3<f32>(0.0, 1.0, 0.0);
    } else {
        color = vec3<f32>(0.0, 0.0, 1.0);
    }

    return VertexOutput(
        vec4<f32>(x, y, 0.0, 1.0),
        color
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
