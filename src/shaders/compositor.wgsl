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

// КОМПОЗИТИНГ 
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

// КОМПОЗИТИНГ С  ДЕНОЙЗЕРОМ
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


    let center_normal = normalize(center_data.hit_normal);

    let up = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), abs(center_normal.x) > 0.9);
    let tangent = normalize(cross(up, center_normal));
    let bitangent = cross(center_normal, tangent);

    // const DIR_COUNT = 5;
    // var bins_dir = array<vec3f, DIR_COUNT>(
    //     center_normal,
    //     normalize(center_normal + tangent * 0.7),
    //     normalize(center_normal - tangent * 0.7),
    //     normalize(center_normal + bitangent * 0.7),
    //     normalize(center_normal - bitangent * 0.7)
    // );
    // // x,y,z = накопленный цвет * вес, w = сумма весов
    // var bins = array<vec4f, DIR_COUNT>(vec4f(0), vec4f(0), vec4f(0), vec4f(0), vec4f(0));

    const DIR_COUNT: i32 = 9;
    // Математически точные коэффициенты для равного телесного угла
    const k_inner: f32 = 0.515388; const k_outer: f32 = 1.425219;
    var bins_dir = array<vec3f, DIR_COUNT>(
        center_normal,                               
        normalize(center_normal + tangent * k_inner), 
        normalize(center_normal - tangent * k_inner), 
        normalize(center_normal + bitangent * k_inner), 
        normalize(center_normal - bitangent * k_inner), 
        normalize(center_normal + (tangent + bitangent) * k_outer), 
        normalize(center_normal + (-tangent + bitangent) * k_outer), 
        normalize(center_normal + (tangent - bitangent) * k_outer), 
        normalize(center_normal + (-tangent - bitangent) * k_outer) 
    );
    // x,y,z = накопленный цвет * вес, w = сумма весов
    var bins = array<vec4f, DIR_COUNT>(vec4f(0.0), vec4f(0.0), vec4f(0.0), vec4f(0.0), vec4f(0.0), vec4f(0.0), vec4f(0.0), vec4f(0.0), vec4f(0.0));


    let radius = 5;
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

            var max_dot = dot(neighbor_data.hit_dir, bins_dir[0]);
            if (max_dot < 0.) { continue; }
            var bin_idx = 0;
            for (var i = 1; i < DIR_COUNT; i++) {
                let d = dot(neighbor_data.hit_dir, bins_dir[i]);
                if (d > max_dot) {
                    max_dot = d;
                    bin_idx = i;
                }
            }

            // Добавляем учет расстояния (Гауссово ядро)
            let dist_sq = f32(dx * dx + dy * dy);
            let w_spatial = exp(-dist_sq / 20.0);

            bins[bin_idx] += vec4f(neighbor_data.reflected_light, 1.) * w_spatial;
        }
    }

    // Избегаем деления на ноль, если все веса оказались нулевыми
    // let final_weight = max(total_weight, 0.001);
    // var final_color = accumulated_color / final_weight;
    var final_color = vec3f(0);
    for (var i = 0; i < 5; i++) {
        if (bins[i].w <= 0.001) { continue; }
        final_color += bins[i].rgb / bins[i].w * dot(center_normal, bins_dir[i]); 
    }

    // Применяем tonemapping и ограничиваем диапазон [0, 1]
    final_color = ton_mapping(final_color * (1 / f32(DIR_COUNT)) * center_data.color);
    // final_color = ton_mapping(final_color * (1 / f32(DIR_COUNT)));
    final_color = max(vec3f(0.0), min(vec3f(1.0), final_color));

    textureStore(output_texture, vec2<i32>(global_id.xy), vec4f(final_color, 1.0));
}
