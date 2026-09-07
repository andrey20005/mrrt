use glam::Vec3;
use crate::polygon::Polygon;
use super::Splitter;

const BIN_COUNT: usize = 64;

pub struct BinnedSahSplit;

impl Splitter for BinnedSahSplit {
    fn new(_polygons: &mut [Polygon], _bounds_min: Vec3, _bounds_max: Vec3) -> Self {
        Self
    }

    fn split(&mut self, polygons: &mut [Polygon], bounds_min: Vec3, bounds_max: Vec3) -> (usize, Self, Self) {
        let count = polygons.len();
        if count < 2 {
            return (0, Self, Self);
        }

        // 1. Выбор самой длинной оси
        let size = bounds_max - bounds_min;
        let mut axis = 0;
        if size.y > size.x { axis = 1; }
        if size.z > size[axis] { axis = 2; }

        let axis_min = bounds_min[axis];
        let axis_max = bounds_max[axis];
        let axis_extent = axis_max - axis_min;

        // 2. Инициализация бинов на стеке (нулевые аллокации в куче)
        let mut bin_counts = [0u32; BIN_COUNT];
        let mut bin_mins = [Vec3::splat(f32::INFINITY); BIN_COUNT];
        let mut bin_maxs = [Vec3::splat(f32::NEG_INFINITY); BIN_COUNT];

        // Защита от деления на ноль, если ось вырождена
        let inv_extent = if axis_extent > 1e-6 { 1.0 / axis_extent } else { 0.0 };

        // 3. Заполнение бинов за O(N)
        for poly in polygons.iter() {
            let center = (poly.v1[axis] + poly.v2[axis] + poly.v3[axis]) / 3.0;
            
            // Безопасное вычисление индекса бина с защитой от float-погрешностей
            let mut bin_idx = ((center - axis_min) * inv_extent * BIN_COUNT as f32) as usize;
            if bin_idx >= BIN_COUNT { 
                bin_idx = BIN_COUNT - 1; 
            }

            bin_counts[bin_idx] += 1;
            bin_mins[bin_idx] = bin_mins[bin_idx].min(poly.min);
            bin_maxs[bin_idx] = bin_maxs[bin_idx].max(poly.max);
        }

        // 4. Поиск границ валидных бинов (первый и последний непустой)
        let mut first_valid = 0;
        while first_valid < BIN_COUNT && bin_counts[first_valid] == 0 {
            first_valid += 1;
        }
        
        let mut last_valid = BIN_COUNT - 1;
        while last_valid > 0 && bin_counts[last_valid] == 0 {
            last_valid -= 1;
        }

        // 5. Обработка вырожденного случая (все полигоны в одном бине или ось нулевая)
        // Это НЕ костыль, а корректный fallback: если пространственно разделить нельзя, 
        // мы делим по объектной медиане.
        if first_valid == last_valid || axis_extent <= 1e-6 {
            let mid = count / 2;
            polygons.select_nth_unstable_by(mid, |a, b| {
                let ca = (a.v1[axis] + a.v2[axis] + a.v3[axis]) / 3.0;
                let cb = (b.v1[axis] + b.v2[axis] + b.v3[axis]) / 3.0;
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            });
            return (mid, Self, Self);
        }

        // 6. Предварительный расчет суффиксов (справа налево)
        let mut suffix_counts = [0u32; BIN_COUNT];
        let mut suffix_mins = [Vec3::splat(f32::INFINITY); BIN_COUNT];
        let mut suffix_maxs = [Vec3::splat(f32::NEG_INFINITY); BIN_COUNT];

        let mut curr_count = 0u32;
        let mut curr_min = Vec3::splat(f32::INFINITY);
        let mut curr_max = Vec3::splat(f32::NEG_INFINITY);
        
        for i in (0..BIN_COUNT).rev() {
            curr_count += bin_counts[i];
            curr_min = curr_min.min(bin_mins[i]);
            curr_max = curr_max.max(bin_maxs[i]);
            suffix_counts[i] = curr_count;
            suffix_mins[i] = curr_min;
            suffix_maxs[i] = curr_max;
        }

        // 7. Поиск оптимального разреза (SAH)
        // ВАЖНО: Мы ищем разрез ТОЛЬКО в диапазоне от first_valid до last_valid - 1.
        // Это математически гарантирует, что слева и справа всегда будет > 0 полигонов.
        let mut min_cost = f32::INFINITY;
        let mut best_split_bin = first_valid; 

        let mut prefix_count = 0u32;
        let mut prefix_min = Vec3::splat(f32::INFINITY);
        let mut prefix_max = Vec3::splat(f32::NEG_INFINITY);

        for i in first_valid..last_valid {
            prefix_count += bin_counts[i];
            prefix_min = prefix_min.min(bin_mins[i]);
            prefix_max = prefix_max.max(bin_maxs[i]);

            let right_count = suffix_counts[i + 1];
            
            let cost = box_surface_area(prefix_min, prefix_max) * prefix_count as f32
                     + box_surface_area(suffix_mins[i + 1], suffix_maxs[i + 1]) * right_count as f32;

            if cost < min_cost {
                min_cost = cost;
                best_split_bin = i; // Разрез проходит между бином i и i+1
            }
        }

        // 8. Физическое разделение полигонов (Partition) за O(N)
        // Все, чей центр попал в бин <= best_split_bin, идут влево.
        let mut left_ptr = 0;
        let mut right_ptr = count - 1;
        
        while left_ptr <= right_ptr {
            let center = (polygons[left_ptr].v1[axis] + polygons[left_ptr].v2[axis] + polygons[left_ptr].v3[axis]) / 3.0;
            let mut bin_idx = ((center - axis_min) * inv_extent * BIN_COUNT as f32) as usize;
            if bin_idx >= BIN_COUNT { 
                bin_idx = BIN_COUNT - 1; 
            }
            
            if bin_idx <= best_split_bin {
                left_ptr += 1;
            } else {
                polygons.swap(left_ptr, right_ptr);
                if right_ptr == 0 { break; } // Защита от underflow
                right_ptr -= 1;
            }
        }

        // Благодаря ограничению цикла оценки (first_valid..last_valid), 
        // left_ptr гарантированно не будет равен 0 или count. Стек не переполнится.
        (left_ptr, Self, Self)
    }
}

fn box_surface_area(box_min: Vec3, box_max: Vec3) -> f32 {
    let d = box_max - box_min;
    2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
}