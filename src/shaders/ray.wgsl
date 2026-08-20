struct Uniform {
    // секунд с начала работы программы 
    time: f32,
    pw: u32,
    ph: u32,
    // просто коэффициенты для вывода правильного uv
    aspect: vec2f,
    // вращение камеры 
    camera_mat: mat3x3f,
    // матрица для вращения камеры
    camera_pos: vec3f,
    // чем больше значение тем больше зум
    camera_zoom: f32,
    // размер пикселя относительно uv, считать как 2 / min(колич. пикс. по высоте, колич. пикс. по ширине)
    pixel_size: f32,
    // свет фона
    background_color: vec3f,
    polygons_count: u32,
    bounces: u32,
    samples: u32,
    graphics_mode: u32
}
@binding(0) @group(0) var<uniform> uf: Uniform;

struct Polygon {
    // переводит в пространство где координаты вершин (0 0 0) (1 0 0) (0 1 0)
    global_to_local: mat3x3f, 
    // направление не вожно
    normal: vec3f,
    // центр основной вершины
    origin: vec3f,
    // в зависимости от контекста
    color: vec3f,
    // от 0 до 1 это частично матовая поверхность 
    //   где 0 зеркало, 0.5 полу матовый материал, а 1 идеально матовый
    //   в этих случаях color это степень поглащения материала
    // 2 это материал без переотражений, свечение 
    //   в таком случае color это яркость свечения
    t: f32 
}
// просто список полигонов на сцене
@binding(1) @group(0) var<storage, read> polygons: array<Polygon>;

struct BvhNode {
    box_max: vec3f,
    // первый ребенок всегда находтся на +1
    // номер второго ребенка или номер начала полигонов
    sec_child_or_first_poly: i32,
    box_min: vec3f,
    // если отрицательное число то в коробке лежат две другие коробки, иначе полигоны в количестве 
    poly_count: i32 
}
@binding(2) @group(0) var<storage, read> bvh: array<BvhNode>;

struct VertexOutput {
    @builtin(position) position : vec4f,
    @location(0) uv : vec2f,
};

// захарткощенный шейдер для выведения фрагментного шейдера на весь экран 
@vertex
fn vertex_main(
    @builtin(vertex_index) vid : u32
) -> VertexOutput {
    var position = vec2f();
    if vid <= 2 {
        if vid == 0 { position = vec2f( 1,  1); }
        if vid == 1 { position = vec2f( 1, -1); }
        if vid == 2 { position = vec2f(-1,  1); }
    } else {
        if vid == 3 { position = vec2f( 1, -1); }
        if vid == 4 { position = vec2f(-1,  1); }
        if vid == 5 { position = vec2f(-1, -1); }
    }
    return VertexOutput(vec4f(position, 0., 1.), position * uf.aspect);
}

// случайность 
var<private> rng_state: u32;
fn hash_u32(seed: u32) -> u32 {
    let word = ((seed >> ((seed >> 28u) + 4u)) ^ seed) * 277803737u;
    return (word >> 22u) ^ word;
}
fn hash_f32(seed: u32) -> f32 { return f32(hash_u32(seed)) / 4294967295.0; }
fn new_seed(a: vec4<u32>) -> u32 {
    return hash_u32(hash_u32(hash_u32(hash_u32(a.x) + a.y) + a.z) + a.w);
}
fn new_seed_f32(a: vec4f) -> u32 {
    return new_seed(vec4<u32>(bitcast<u32>(a.x), bitcast<u32>(a.y), bitcast<u32>(a.z), bitcast<u32>(a.w)));
}
fn random_u32() -> u32 {
    let old_state = rng_state;
    rng_state = old_state * 747796405u + 2891336453u;
    return hash_u32(rng_state);
}
fn random_f32() -> f32 { return f32(random_u32()) / 4294967295.0; }
fn rand3d_cosine_hemisphere(n: vec3f) -> vec3f {
    let r_sq = random_f32(); 
    let r = sqrt(r_sq);
    let phi = random_f32() * 6.283185307;
    
    let local_dir = vec3f(r * cos(phi), r * sin(phi), sqrt(max(0.0, 1.0 - r_sq)));
    
    // ИСПРАВЛЕНИЕ: Безопасный выбор вспомогательного вектора.
    // Если нормаль почти совпадает с осью X, берем ось Y. Иначе всегда берем ось X.
    // Это на 100% защищает от NaN на полах vec3f(0, 1, 0) и стенах.
    let up = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), abs(n.x) > 0.9);
    
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    
    return tangent * local_dir.x + bitangent * local_dir.y + n * local_dir.z;
}


// --- ЛОГИКА РЕЙТРЕЙСИНГА ---

// обработка цвета
fn ton_mapping(x: vec3f) -> vec3f {
    return x * 1.2 / (x + vec3f(1.0));
}

var<private> poly_intersect_count: i32 = 0;
fn poly_intersect(ro: vec3f, rd: vec3f, poly: Polygon) -> f32 {
    poly_intersect_count++;
    // if (dot(rd, poly.normal) >= 0.0) { return -1.0; }

    let local_ro = poly.global_to_local * (ro - poly.origin);
    let local_rd = poly.global_to_local * rd;

    if (abs(local_rd.z) < 0.00001) { return -1.0; }
    
    let t = -local_ro.z / local_rd.z;
    if (t < 0.0) { return -1.0; }

    let p = local_ro + local_rd * t;

    if (p.x >= 0.0 && p.y >= 0.0 && (p.x + p.y) <= 1.0) {
        return t;
    }
    return -1.0;
}
var<private> box_intersect_count: i32 = 0;
fn box_intersect(
    box_min: vec3<f32>, box_max: vec3<f32>, 
    ro: vec3<f32>, inv_dir: vec3<f32>
) -> f32 {
    box_intersect_count++;

    let t0 = (box_min - ro) * inv_dir;
    let t1 = (box_max - ro) * inv_dir;
    
    let tmin_v = min(t0, t1);
    let tmax_v = max(t0, t1);
    
    let t_near = max(max(tmin_v.x, tmin_v.y), tmin_v.z);
    let t_far  = min(min(tmax_v.x, tmax_v.y), tmax_v.z);
    
    let hit = t_near <= t_far && t_far >= 0.0;
    
    return select(-1.0, max(0.0, t_near), hit);
}


fn get_inv_dir(rd: vec3<f32>) -> vec3<f32> {
    let eps = 1e-6;
    let sx = select(rd.x, sign(rd.x) * eps, rd.x == 0.0);
    let sy = select(rd.y, sign(rd.y) * eps, rd.y == 0.0);
    let sz = select(rd.z, sign(rd.z) * eps, rd.z == 0.0);
    return vec3<f32>(1.0 / sx, 1.0 / sy, 1.0 / sz);
}

struct RayHit {
    hit_poly_idx: i32,
    dist: f32,
}

fn cast_ray(ro: vec3f, rd: vec3f) -> RayHit {
    var min_dist = 1000000.0;

    var hit_poly_idx: i32 = -1; 

    // --- НАЧАЛО ОБХОДА BVH ---
    let inv_dir = get_inv_dir(rd);
    var stack: array<i32, 32>;
    var stack_ptr: i32 = 0;
    var currentNodeIdx: i32 = 0;
    var keep_running = true;
    var node: BvhNode;
    while (keep_running) {
        currentNodeIdx = stack[stack_ptr];
        node = bvh[currentNodeIdx];
        stack_ptr = stack_ptr - 1;

        if (node.poly_count > 0) {
            // ЛИСТ
            let box_dist = box_intersect(node.box_min, node.box_max, ro, inv_dir);
            if (box_dist >= 0. && box_dist < min_dist) {
                let startPoly = node.sec_child_or_first_poly;
                let endPoly = startPoly + node.poly_count;

                for (var i: i32 = startPoly; i < endPoly; i = i + 1) {
                    let polygon = polygons[i];
                    if (polygon.t >= 0. || dot(rd, polygon.normal) < 0.) {
                        let dist = poly_intersect(ro, rd, polygons[i]);
                        if (dist > 0.0 && dist < min_dist) {
                            min_dist = dist;
                            hit_poly_idx = i;
                        }
                    }
                }
            }
        } else {
            let box_dist = box_intersect(node.box_min, node.box_max, ro, inv_dir);
            if (box_dist >= 0. && box_dist < min_dist) {
                let child1_idx  = currentNodeIdx + 1;
                let child2_idx = node.sec_child_or_first_poly;

                let child1 = bvh[child1_idx];
                let child2 = bvh[child2_idx];

                let ch1_dist = box_intersect(child1.box_min, child1.box_max, ro, inv_dir);
                let ch2_dist = box_intersect(child2.box_min, child2.box_max, ro, inv_dir);

                if (ch1_dist >= 0. && ch1_dist < min_dist) {
                    if (ch2_dist >= 0 && ch2_dist < min_dist) {
                        stack_ptr += 2;
                        if (ch1_dist < ch2_dist) {
                            stack[stack_ptr-1] = child2_idx;
                            stack[stack_ptr] = child1_idx;
                        } else {
                            stack[stack_ptr-1] = child1_idx;
                            stack[stack_ptr] = child2_idx;
                        }
                    } else {
                        stack_ptr++;
                        stack[stack_ptr] = child1_idx;
                    }
                } else if (ch2_dist >= 0 && ch2_dist < min_dist) {
                    stack_ptr++;
                    stack[stack_ptr] = child2_idx;
                }
            }
        }

        keep_running = stack_ptr >= 0;
    }

    // let inv_dir = get_inv_dir(rd); // Считаем один раз для всех коробок
    // var stack: array<i32, 32>;
    // var stack_ptr: i32 = 0;
    // var currentNodeIdx: i32 = 0; // Начинаем с корня (всегда индекс 0)
    // var keep_running = true;
    // while (keep_running) {
    //     let node = bvh[currentNodeIdx];

    //     if (node.poly_count > 0) {
    //         // ЛИСТ
    //         let box_dist = box_intersect(node.box_min, node.box_max, ro, inv_dir);
    //         if (box_dist >= 0. && box_dist < min_dist) {
    //             let startPoly = node.sec_child_or_first_poly;
    //             let endPoly = startPoly + node.poly_count;

    //             for (var i: i32 = startPoly; i < endPoly; i = i + 1) {
    //                 let dist = poly_intersect(ro, rd, polygons[i]);
    //                 if (dist > 0.0 && dist < min_dist) {
    //                     min_dist = dist;
    //                     hit_poly_idx = i;
    //                 }
    //             }
    //         }

    //         // Закончили с листом, достаем следующую ноду из стека
    //         if (stack_ptr == 0) {
    //             keep_running = false; // Стек пуст, обход завершен
    //         } else {
    //             stack_ptr = stack_ptr - 1;
    //             currentNodeIdx = stack[stack_ptr];
    //         }
    //     } else {
    //         // ВНУТРЕННИЙ УЗЕЛ
    //         let firstChildIdx  = currentNodeIdx + 1;
    //         let secondChildIdx = node.sec_child_or_first_poly;

    //         let child1 = bvh[firstChildIdx];
    //         let child2 = bvh[secondChildIdx];

    //         let t1 = box_intersect(child1.box_min, child1.box_max, ro, inv_dir);
    //         let t2 = box_intersect(child2.box_min, child2.box_max, ro, inv_dir);

    //         // Проверяем, пересекаются ли дочерние коробки, 
    //         // и лежат ли они ближе, чем наше ТЕКУЩЕЕ ближайшее попадание (min_dist)
    //         let hit1 = t1 >= 0.0 && t1 < min_dist;
    //         let hit2 = t2 >= 0.0 && t2 < min_dist;

    //         if (hit1 && hit2) {
    //             // Магия оптимизации по расстоянию: 
    //             // Сначала идем в ту коробку, которая ближе, а дальнюю кладем в стек.
    //             if (t1 < t2) {
    //                 stack[stack_ptr] = secondChildIdx; // Дальняя — в стек
    //                 stack_ptr = stack_ptr + 1;
    //                 currentNodeIdx = firstChildIdx;  // В ближнюю идем сейчас
    //             } else {
    //                 stack[stack_ptr] = firstChildIdx;
    //                 stack_ptr = stack_ptr + 1;
    //                 currentNodeIdx = secondChildIdx;
    //             }
    //         } else if (hit1) {
    //             currentNodeIdx = firstChildIdx;
    //         } else if (hit2) {
    //             currentNodeIdx = secondChildIdx;
    //         } else {
    //             // Промазали мимо обеих коробок — достаем ноду из стека
    //             if (stack_ptr == 0) {
    //                 keep_running = false;
    //             } else {
    //                 stack_ptr = stack_ptr - 1;
    //                 currentNodeIdx = stack[stack_ptr];
    //             }
    //         }
    //     }
    // }

    // --- КОНЕЦ ОБХОДА BVH ---

    return RayHit(hit_poly_idx, min_dist);
}

struct RayReflection {
    color: vec3f,
    is_terminal: bool,
    newRo: vec3f,
    newRd: vec3f,
};

fn reflect_ray(ro: vec3f, rd: vec3f, rayHit: RayHit) -> RayReflection {
    var hit_color = uf.background_color;
    var terminal = true;
    let hit_poly_idx = rayHit.hit_poly_idx;
    let dist = rayHit.dist;
    var newRo = ro;
    var newRd = rd;

    // Если луч задел какой-то полигон, обрабатываем его материал
    if (hit_poly_idx != -1) {
        let poly = polygons[hit_poly_idx];

        var t = poly.t;
        if (t < 0.) { // если t отрицательное то полигон виден только с одной стороны
            t = -t;
            // if (dot(poly.normal, ro) <= 0.) {
            //     hit_color = vec3f(1.);
            //     newRo = ro + rd * dist + poly.normal * 0.0001;
            //     newRd = rd;
            //     return RayReflection(hit_color, terminal, newRo, newRd);
            // }
        }
        
        var normal = poly.normal;
        if (dot(rd, poly.normal) > 0.0) { normal = -poly.normal; }

        newRo = ro + rd * dist + normal * 0.0001;
        
        if (t == 2.0) {
            // Режим 2: Свечение
            hit_color = poly.color;
            terminal = true;
        } 
        else if (t == 0.0) {
            // Режим 0: Идеальное зеркало
            hit_color = poly.color;
            newRd = reflect(rd, normal);
            terminal = false;
        } 
        else if (t == 1.0) {
            // Режим 1: Идеально матовая поверхность
            hit_color = poly.color;
            newRd = rand3d_cosine_hemisphere(normal);
            terminal = false;
        } 
        else if (t > 0.0 && t < 1.0) {
            // Режим от 0 до 1: Частично матовый материал
            hit_color = poly.color;
            // hit_color = poly.color * 0.5 + normal;
            let diffuse = rand3d_cosine_hemisphere(normal);
            let specular = reflect(rd, normal);
            
            newRd = normalize(mix(specular, diffuse, poly.t));
            terminal = false;
        } 
        else {
            // Некорректное значение t — возвращаем отладочный ярко-розовый цвет
            hit_color = vec3f(10.0, 0.0, 10.0);
            terminal = true;
        }
    }

    return RayReflection(hit_color, terminal, newRo, newRd);
}

fn trace_ray(ro_in: vec3f, rd_in: vec3f) -> vec3f {
    var col = vec3f(1.0);
    var ro = ro_in;
    var rd = rd_in;
    
    for (var i = uf.bounces; i > 0; i--) {
        let hit = cast_ray(ro, rd);
        let refl = reflect_ray(ro, rd, hit);
        col *= refl.color;
        if (refl.is_terminal) { break; }
        else if (i == 1) { col = vec3f(0); break; }
        ro = refl.newRo;
        rd = refl.newRd;
    }
    return col;
}

// Функция плавного шага для изоляции цветовых диапазонов
fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Перевод f32 [0.0 - 1.0] в классический спектр теплокарты (Синий -> Зеленый -> Желтый -> Красный)
fn thermal_palette(t: f32) -> vec3f {
    let x = clamp(t, 0.0, 1.0);
    
    // Рассчитываем интенсивность для каждого канала
    let r = smooth_step(0.4, 0.7, x);
    let g = smooth_step(0.1, 0.4, x) - smooth_step(0.7, 0.9, x);
    let b = smooth_step(0.0, 0.2, x) - smooth_step(0.4, 0.6, x) + smooth_step(0.9, 1.0, x) * 0.5;

    return vec3f(r, g, b);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    // сид для рандома
    rng_state = new_seed_f32(vec4f(uf.time, input.uv.x, input.uv.y, 0));
    
    let uv = input.uv;
    
    var color = vec3f(0);
    if uf.graphics_mode == 1 {
        for(var i: u32 = 0; i < uf.samples; i++) {
            color += trace_ray(
                uf.camera_pos, 
                normalize(uf.camera_mat * vec3f(uv + uf.pixel_size * vec2f(random_f32(), random_f32()), uf.camera_zoom))
            );
        }
        color = max(vec3f(0), min(vec3f(1), ton_mapping(color / f32(uf.samples))));
    } else if uf.graphics_mode == 2 {
        let ro = uf.camera_pos;
        let rd = normalize(uf.camera_mat * vec3f(uv + uf.pixel_size * vec2f(random_f32(), random_f32()), uf.camera_zoom));
        let hit = cast_ray(ro, rd); 
        let refl = reflect_ray(ro, rd, hit);
        // color = vec3f(1.) * (1. - min(1.0, max(0.0, hit.dist * (1. / 2.2) - 0.3)));
        color = refl.color * (1. - min(1.0, max(0.0, hit.dist * (1. / 4.))));
        // color = vec3f(1.) * min(1., pow(0.1 / (1 + hit.dist - 0.), 0.7));
        // color = refl.color * min(1., pow(0.1 / (1 + hit.dist - 0.5), 0.7));
    } else if uf.graphics_mode == 3 {
        let ro = uf.camera_pos;
        let rd = normalize(uf.camera_mat * vec3f(uv + uf.pixel_size * vec2f(random_f32(), random_f32()), uf.camera_zoom));
        cast_ray(ro, rd);

        const mm = 400.0;
        // let a = min(1.0, f32(poly_intersect_count) / mm + f32(box_intersect_count) / mm);
        // let a = min(1.0, f32(poly_intersect_count) / mm);
        let a = min(1.0, f32(box_intersect_count) / mm);
        // color = vec3f(a);
        color = thermal_palette(a);
    }

    return vec4f(color, 1.0);
}
