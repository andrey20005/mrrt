enable f16;

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

// РОВНО 16 байт
struct NeighborDataCompact {
    light_and_noisy: vec4<f16>, // x: reflected_light.r, y: reflected_light.g, z: reflected_light.b, w: has_noisy
    dir_and_length: vec4<f16>,  // x: hit_dir.x, y: hit_dir.y, z: hit_dir.z, w: path_length
}
@binding(4) @group(0) var<storage, read> compact_buffer: array<NeighborDataCompact>;

// РОВНО 16 байт
struct CenterData {
    normal_and_mat: vec4<f16>,  // x: hit_normal.x, y: hit_normal.y, z: hit_normal.z, w: hit_mat_type
    color_and_pad: vec4<f16>,   // x: color.r, y: color.g, z: color.b, w: 0.0
}
@binding(5) @group(0) var<storage, read> center_buffer: array<CenterData>;

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
    let center_compact = compact_buffer[idx];
    let center_data = center_buffer[idx];

    let center_color = vec3f(
        f32(center_data.color_and_pad.x),
        f32(center_data.color_and_pad.y),
        f32(center_data.color_and_pad.z)
    );
    let center_reflected = vec3f(
        f32(center_compact.light_and_noisy.x),
        f32(center_compact.light_and_noisy.y),
        f32(center_compact.light_and_noisy.z)
    );

    // Базовая сборка цвета (в будущем здесь будет логика денойзера)
    var final_color = center_color * center_reflected;

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
    let center_compact = compact_buffer[idx];
    let center_data = center_buffer[idx];

    let center_color = vec3f(
        f32(center_data.color_and_pad.x),
        f32(center_data.color_and_pad.y),
        f32(center_data.color_and_pad.z)
    );

    // Если шума на этом пути не было (например, чистое зеркало или прямой свет), 
    // денойзинг только размоет идеальную картинку. Отдаем результат как есть.
    if (center_compact.light_and_noisy.w < 0.0) {
        let center_reflected = vec3f(
            f32(center_compact.light_and_noisy.x),
            f32(center_compact.light_and_noisy.y),
            f32(center_compact.light_and_noisy.z)
        );
        let final_color = ton_mapping(center_color * center_reflected);
        let clamped_color = max(vec3f(0.0), min(vec3f(1.0), final_color));
        textureStore(output_texture, vec2<i32>(global_id.xy), vec4f(clamped_color, 1.0));
        return;
    }

    let center_normal = normalize(vec3f(
        f32(center_data.normal_and_mat.x),
        f32(center_data.normal_and_mat.y),
        f32(center_data.normal_and_mat.z)
    ));
    let center_path_length = f32(center_compact.dir_and_length.w);

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

    const radius = 5;
    const radius_div = 1 / f32(radius * 4);
    for (var dy = -radius; dy <= radius; dy = dy + 1) {
        for (var dx = -radius; dx <= radius; dx = dx + 1) {
            let nx = i32(global_id.x) + dx;
            let ny = i32(global_id.y) + dy;

            // Проверка границ экрана
            if (nx < 0 || ny < 0 || nx >= i32(dimensions.x) || ny >= i32(dimensions.y)) {
                continue;
            }

            let n_idx = u32(ny) * dimensions.x + u32(nx);
            let neighbor_compact = compact_buffer[n_idx];
            let neighbor_path_length = f32(neighbor_compact.dir_and_length.w);

            let neighbor_dir = normalize(vec3f(
                f32(neighbor_compact.dir_and_length.x),
                f32(neighbor_compact.dir_and_length.y),
                f32(neighbor_compact.dir_and_length.z)
            ));
            let neighbor_reflected = vec3f(
                f32(neighbor_compact.light_and_noisy.x),
                f32(neighbor_compact.light_and_noisy.y),
                f32(neighbor_compact.light_and_noisy.z)
            );

            var max_dot = dot(neighbor_dir, bins_dir[0]);
            if (max_dot < 0.) { continue; }
            var bin_idx = 0;
            for (var i = 1; i < DIR_COUNT; i++) {
                let d = dot(neighbor_dir, bins_dir[i]);
                if (d > max_dot) {
                    max_dot = d;
                    bin_idx = i;
                }
            }

            let diff = neighbor_path_length / center_path_length - 1;

            let dist_sq = f32(dx * dx + dy * dy);
            // let w_spatial = 1.;
            // let w_spatial = exp(-dist_sq * radius_div);
            // let w_spatial = max(0, 1 - diff * diff * 10000);
            let w_spatial = exp(-dist_sq * radius_div) * max(0, 1 - diff * diff * 100);

            bins[bin_idx] += vec4f(neighbor_reflected, 1.0) * w_spatial;
        }
    }

    // Избегаем деления на ноль, если все веса оказались нулевыми
    // let final_weight = max(total_weight, 0.001);
    // var final_color = accumulated_color / final_weight;
    var final_color = vec3f(0);
    for (var i = 0; i < DIR_COUNT; i++) {
        if (bins[i].w <= 0.001) { continue; }
        final_color += bins[i].rgb / bins[i].w * dot(center_normal, bins_dir[i]); 
    }

    // Применяем tonemapping и ограничиваем диапазон [0, 1]
    final_color = ton_mapping(final_color * (1.0 / f32(DIR_COUNT)) * center_color);
    // final_color = ton_mapping(final_color * (1 / f32(DIR_COUNT)));
    final_color = max(vec3f(0.0), min(vec3f(1.0), final_color));

    textureStore(output_texture, vec2<i32>(global_id.xy), vec4f(final_color, 1.0));
}
