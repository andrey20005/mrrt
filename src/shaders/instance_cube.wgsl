struct CubeUniform {
    camera_mat: mat3x3f,
    camera_pos: vec3f,
    time: f32, 
}
@binding(0) @group(0) var<uniform> uf: CubeUniform;

// Данные инстанса (приходят на каждый треугольник целиком)
struct TriangleInput {
    @location(0) v1: vec3f,
    @location(1) v2: vec3f,
    @location(2) v3: vec3f,
    @location(3) normal: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) normal: vec3f,
}

@vertex
fn vertex_main(
    instance: TriangleInput,
    @builtin(vertex_index) vertex_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Выбираем нужную вершину в зависимости от индекса внутри треугольника (0, 1 или 2)
    var raw_position: vec3f;
    let local_idx = vertex_idx % 3u; // Гарантируем диапазон 0, 1, 2
    
    if local_idx == 0u {
        raw_position = instance.v1;
    } else if local_idx == 1u {
        raw_position = instance.v2;
    } else {
        raw_position = instance.v3;
    }

    // Вся остальная математика трансформации остается прежней!
    let rotated_position = uf.camera_mat * raw_position;
    let final_position = rotated_position - uf.camera_pos;

    let perspective_factor = 2.0 / (final_position.z + 4.0);
    out.clip_position = vec4f(
        final_position.x * perspective_factor,
        final_position.y * perspective_factor,
        0.5,
        1.0
    );

    out.normal = uf.camera_mat * instance.normal;
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
