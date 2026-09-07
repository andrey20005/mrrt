use glam::Vec3;
use crate::polygon::Polygon;
use super::Splitter;

pub struct SahSplit {
    // Владеющий буфер. Чтобы вернуть два новых Self без параметров времени жизни <'a>,
    // мы используем Vec::split_off. Это дает минимальную стоимость: левый ребенок 
    // забирает оригинальный буфер (0 аллокаций), а правый получает новый (1 быстрая аллокация).
    scratch: Vec<f32>,
}

impl Splitter for SahSplit {
    fn new(polygons: &mut [Polygon], _bounds_min: Vec3, _bounds_max: Vec3) -> Self {
        // Выделяем буфер ОДИН раз при создании корневого сплиттера
        Self {
            scratch: vec![0.0; polygons.len()],
        }
    }

    fn split(&mut self, polygons: &mut [Polygon], bounds_min: Vec3, bounds_max: Vec3) -> (usize, Self, Self) {
        let count = polygons.len();
        if count < 2 {
            return (0, Self { scratch: Vec::new() }, Self { scratch: Vec::new() });
        }

        let size = bounds_max - bounds_min;
        let axis = if size.y > size.x { 1 } else { 0 };
        let axis = if size.z > size[axis] { 2 } else { axis };

        // Сортировка
        polygons.sort_by(|a, b| {
            let ca = (a.v1[axis] + a.v2[axis] + a.v3[axis]) / 3.0;
            let cb = (b.v1[axis] + b.v2[axis] + b.v3[axis]) / 3.0;
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut left_min = polygons[0].min;
        let mut left_max = polygons[0].max;
        
        for i in 0..(count - 1) {
            left_min = left_min.min(polygons[i].min);
            left_max = left_max.max(polygons[i].max);
            let left_count = (i + 1) as f32;
            self.scratch[i] = box_surface_area(left_min, left_max) * left_count;
        }

        let mut right_min = polygons[count - 1].min;
        let mut right_max = polygons[count - 1].max;
        for i in (0..(count - 1)).rev() {
            right_min = right_min.min(polygons[i + 1].min);
            right_max = right_max.max(polygons[i + 1].max);
            let right_count = (count - 1 - i) as f32;
            self.scratch[i] += box_surface_area(right_min, right_max) * right_count;
        }

        let mut min_cost = f32::INFINITY;
        let mut local_mid = 0;
        for i in 0..(count - 1) {
            if self.scratch[i] < min_cost {
                min_cost = self.scratch[i];
                local_mid = i;
            }
        }

        let mid = local_mid + 1;

        // 🚀 МАГИЯ РАЗДЕЛЕНИЯ:
        // Забираем вектор из self, делим его, и создаем двух новых детей.
        // left_child получает оригинальный буфер (аллокаций = 0).
        // right_child получает новый вектор через split_off (аллокация = 1, но очень быстрая).
        let mut taken_scratch = std::mem::take(&mut self.scratch);
        let right_scratch = taken_scratch.split_off(mid);
        
        let left_splitter = SahSplit { scratch: taken_scratch };
        let right_splitter = SahSplit { scratch: right_scratch };

        (mid, left_splitter, right_splitter)
    }
}

fn box_surface_area(box_min: Vec3, box_max: Vec3) -> f32 {
    let d = box_max - box_min;
    2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
}