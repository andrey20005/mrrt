struct TimeUniform {
    value: f32,
};
@group(0) @binding(0) var<uniform> time: TimeUniform;
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>;

// Хэш-функция для генерации четкого псевдослучайного шума (как белый шум в телеке)
fn hash(p: vec2<f32>) -> f32 {
    let s = sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123;
    return fract(s);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let dimensions = textureDimensions(output_texture);
    
    if (global_id.x >= dimensions.x || global_id.y >= dimensions.y) {
        return;
    }

    // Центрируем координаты и приводим к диапазону от -1.0 до 1.0 (с учетом аспекта)
    let aspect = f32(dimensions.x) / f32(dimensions.y);
    let uv = (vec2<f32>(global_id.xy) / vec2<f32>(dimensions) - 0.5) * vec2<f32>(aspect, 1.0);

    // 1. Создаем тонкую детализированную сетку, которая при низком скейле превратится в кашу
    let grid_scale = 40.0;
    let grid = sin(uv.x * grid_scale + time.value) * sin(uv.y * grid_scale);
    let grid_lines = smoothstep(0.8, 0.9, grid);

    // 2. Добавляем тонкие концентрические круги (эффект муара на низком разрешении)
    let dist = length(uv);
    let rings = sin(dist * 80.0 - time.value * 4.0);
    let ring_lines = smoothstep(0.7, 0.9, rings) * 0.4;

    // 3. Добавляем резкое зерно (высокочастотный шум)
    let grain = hash(uv + time.value) * 0.15;

    // Смешиваем всё в психоделический узор
    var r = 0.5 + 0.5 * sin(dist * 10.0 - time.value) + grid_lines;
    var g = 0.2 + 0.5 * cos(time.value * 0.5) + ring_lines;
    var b = 0.5 + 0.5 * sin(uv.x * 5.0 + time.value) + grain;

    // Делаем легкую виньетку (затемнение по краям кадра)
    let vignette = smoothstep(1.2, 0.4, dist);
    let color = vec4<f32>(r, g, b, 1.0) * vignette;

    textureStore(output_texture, vec2<i32>(global_id.xy), color);
}
