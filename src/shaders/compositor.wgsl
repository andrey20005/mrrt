struct Uniform {
    time: f32,
    aspect: vec2f,
    camera_mat: mat3x3f,
    camera_pos: vec3f,
    camera_zoom: f32,
    pixel_size: f32,
    background_color: vec3f,
    polygons_count: u32,
    bounces: u32,
    samples: u32,
}
@binding(0) @group(0) var<uniform> uf: Uniform;

struct PathData {
    color: vec3f,
    has_noisy: f32,
    reflected_light: vec3f,
    hit_mat_type: f32,
    hit_pos: vec3f,
    _pad0: f32,
    hit_normal: vec3f,
    _pad1: f32,
    hit_dir: vec3f,
    _pad2: f32,
}
@binding(4) @group(0) var<storage, read> path_data_buffer: array<PathData>;

@binding(3) @group(0) var output_texture: texture_storage_2d<rgba8unorm, write>;

fn ton_mapping(x: vec3f) -> vec3f {
    return x * 1.2 / (x + vec3f(1.0));
}

// --- ШЕЙДЕР 2: КОМПОЗИТИНГ (БУДУЩИЙ ДЕНОЙЗЕР) ---
@compute @workgroup_size(16, 16)
fn main_pass2(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dimensions = textureDimensions(output_texture);
    if (global_id.x >= dimensions.x || global_id.y >= dimensions.y) { return; }

    let idx = global_id.y * dimensions.x + global_id.x;
    let data = path_data_buffer[idx];

    // Базовая сборка цвета (в будущем здесь будет логика денойзера)
    var final_color = data.color * data.reflected_light;

    // Применяем tonemapping
    final_color = ton_mapping(final_color);

    // Ограничиваем диапазон и записываем в текстуру
    final_color = max(vec3f(0), min(vec3f(1), final_color));
    textureStore(output_texture, vec2<i32>(global_id.xy), vec4f(final_color, 1.));
}

// --- ШЕЙДЕР 2: КОМПОЗИТИНГ С ПРОСТЫМ ДЕНОЙЗЕРОМ (5x5) ---
@compute @workgroup_size(16, 16)
fn main_denoise(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dimensions = textureDimensions(output_texture);
    if (global_id.x >= dimensions.x || global_id.y >= dimensions.y) { return; }

    let idx = global_id.y * dimensions.x + global_id.x;
    let center_data = path_data_buffer[idx];

    // Если шума на этом пути не было (например, чистое зеркало или прямой свет), 
    // денойзинг только размоет идеальную картинку. Отдаем результат как есть.
    if (center_data.has_noisy < 0.0) {
        let final_color = ton_mapping(center_data.color * center_data.reflected_light);
        let clamped_color = max(vec3f(0.0), min(vec3f(1.0), final_color));
        textureStore(output_texture, vec2<i32>(global_id.xy), vec4f(clamped_color, 1.0));
        return;
    }

    var accumulated_color = vec3f(0.0);
    var total_weight = 0.0;

    let center_normal = normalize(center_data.hit_normal);
    let center_dir = normalize(center_data.hit_dir); // Направление отскока в центре

    let radius = 8; // Ядро 5x5: от -2 до +2

    for (var dy = -radius; dy <= radius; dy = dy + 1) {
        for (var dx = -radius; dx <= radius; dx = dx + 1) {
            let nx = i32(global_id.x) + dx;
            let ny = i32(global_id.y) + dy;

            // Проверка границ экрана
            if (nx < 0 || ny < 0 || nx >= i32(dimensions.x) || ny >= i32(dimensions.y)) {
                continue;
            }

            let n_idx = u32(ny) * dimensions.x + u32(nx);
            let neighbor_data = path_data_buffer[n_idx];

            // 1. Пространственный вес (Гауссиан). Центр важнее краев.
            let dist_sq = f32(dx * dx + dy * dy);
            let w_spatial = exp(-dist_sq * 0.); // 2.5 - параметр "ширины" размытия

            // 2. Вес по нормалям. Предотвращает растекание цвета через границы объектов.
            let neighbor_normal = normalize(neighbor_data.hit_normal);
            let normal_dot = max(0.0, dot(center_normal, neighbor_normal));
            // Возводим в степень, чтобы резко обрывать вес на границах (16 или 32 дают хороший результат)
            let w_normal = pow(normal_dot, 16.0);

            // 3. Вес по направлению отскока (как ты и просил).
            // Если направление луча у соседа сильно отличается от центрального, снижаем вес.
            let neighbor_dir = normalize(neighbor_data.hit_dir);
            let dir_dot = max(0.0, dot(center_dir, neighbor_dir));
            let w_dir = pow(dir_dot, 8.0); // Степень чуть ниже, чтобы не было слишком жестких артефактов

            // Итоговый вес пикселя
            let weight = w_spatial * w_normal * w_dir;

            // Накапливаем взвешенный цвет (произведение цвета и отраженного света)
            let neighbor_radiance = neighbor_data.color * neighbor_data.reflected_light;
            accumulated_color += neighbor_radiance * weight;
            total_weight += weight;
        }
    }

    // Избегаем деления на ноль, если все веса оказались нулевыми
    let final_weight = max(total_weight, 0.001);
    var final_color = accumulated_color / final_weight;

    // Применяем tonemapping и ограничиваем диапазон [0, 1]
    final_color = ton_mapping(final_color);
    final_color = max(vec3f(0.0), min(vec3f(1.0), final_color));

    textureStore(output_texture, vec2<i32>(global_id.xy), vec4f(final_color, 1.0));
}
