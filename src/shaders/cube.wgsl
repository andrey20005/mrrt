// 1. Описываем Uniform-структуру в змеином шрифте
struct CubeUniform {
    camera_mat: mat3x3f,
    camera_pos: vec3f,
    time: f32, 
}
@binding(0) @group(0) var<uniform> uf: CubeUniform;

// 2. Входные данные от процессора в Вершинный шейдер
struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
}

// 3. Данные из Вершинного шейдера во Фрагментный
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) normal: vec3f,
}

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Вращаем вершину вокруг центра куба
    let rotated_position = uf.camera_mat * input.position;

    // Сдвигаем вершину относительно камеры
    let final_position = rotated_position - uf.camera_pos;

    // Считаем простую перспективу
    let perspective_factor = 2.0 / (final_position.z + 4.0);
    out.clip_position = vec4f(
        final_position.x * perspective_factor,
        final_position.y * perspective_factor,
        0.5,
        1.0
    );

    // Вращаем нормаль вместе с кубом
    out.normal = uf.camera_mat * input.normal;

    return out;
}

@fragment
fn fragment_main(in: VertexOutput) -> @location(0) vec4f {
    // Направление на солнце
    let sun_direction = normalize(vec3f(1.0, 1.0, 1.0));

    // Нормализуем входящую нормаль
    let normal = normalize(in.normal);

    // Считаем освещенность (скалярное произведение)
    let diffuse_intensity = max(dot(normal, sun_direction), 0.0);

    // Базовый цвет куба и фоновый свет
    let cube_color = vec3f(1.0, 0.6, 0.1);
    let ambient_light = vec3f(0.1, 0.1, 0.1);

    let final_color = cube_color * diffuse_intensity + ambient_light;

    return vec4f(final_color, 1.0);
}
