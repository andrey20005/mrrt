use std::ops::Range;
use glam::Vec3;
use crate::polygon::Polygon;
use crate::shaders::ray as ray_gpu; // Доступ к сгенерированной wgsl_to_wgpu структуре Box

/// Узел BVH-дерева на CPU с владеющими указателями Box
pub struct BvhNode {
    pub min: Vec3,
    pub max: Vec3,
    pub polygon_range: Range<usize>,
    pub is_leaf: bool,
    pub first_child: Option<Box<BvhNode>>,
    pub second_child: Option<Box<BvhNode>>,
}

#[derive(Debug)]
pub struct BvhStats {
    pub total_leaves: u32,
    pub min_poly_in_leaf: u32,
    pub max_poly_in_leaf: u32,
    pub avg_poly_in_leaf: f32,
    pub min_depth: u32,
    pub max_depth: u32,
    pub avg_depth: f32,
}

impl BvhNode {
    /// Создает узел-лист. Он просто рассчитывает общие границы для своего диапазона полигонов.
    pub fn new_leaf(polygons: &[Polygon], polygon_range: Range<usize>) -> Self {
        let mut min = polygons[polygon_range.start].min;
        let mut max = polygons[polygon_range.start].max;

        // Объединяем границы всех полигонов, входящих в этот диапазон
        for poly in &polygons[polygon_range.start as usize..polygon_range.end as usize] {
            min = min.min(poly.min);
            max = max.max(poly.max);
        }

        Self {
            min,
            max,
            polygon_range,
            is_leaf: true,
            first_child: None,
            second_child: None,
        }
    }

    /// Рекурсивно строит BVH-дерево по медиане вдоль самой длинной оси контейнера
    pub fn new_bvh(
        polygons: &mut [Polygon],
        polygon_range: Range<usize>,
        max_layers: usize,
        max_polygons: usize,
    ) -> Self {
        let count = polygon_range.end - polygon_range.start;

        // Базовый случай: если достигнут лимит слоев или полигонов мало, делаем лист
        if max_layers == 0 || count <= max_polygons {
            return Self::new_leaf(polygons, polygon_range);
        }

        // Считаем общие границы для текущего диапазона (они нужны для создания текущего узла)
        let mut total_min = polygons[polygon_range.start].min;
        let mut total_max = polygons[polygon_range.start].max;
        for poly in &polygons[polygon_range.start..polygon_range.end] {
            total_min = total_min.min(poly.min);
            total_max = total_max.max(poly.max);
        }

        // Находим самую длинную ось нашей коробки (0 = X, 1 = Y, 2 = Z)
        let size = total_max - total_min;
        let mut axis = 0;
        if size.y > size.x { axis = 1; }
        if size.z > size[axis] { axis = 2; }

        // Сортируем полигоны по центрам вдоль выбранной оси
        let sub_slice = &mut polygons[polygon_range.start..polygon_range.end];
        sub_slice.sort_by(|a, b| {
            let center_a = (a.v1[axis] + a.v2[axis] + a.v3[axis]) / 3.0;
            let center_b = (b.v1[axis] + b.v2[axis] + b.v3[axis]) / 3.0;
            center_a.partial_cmp(&center_b).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Массив для накопления стоимостей каждого сплита. 
        // Кандидатов на разрез всего (count - 1), так как пустые дочерние узлы создавать нельзя.
        let mut all_costs = vec![0.0; count - 1];

        // Прямой проход: считаем площади левых коробок (от 0 до i включительно)
        let mut left_min = sub_slice[0].min;
        let mut left_max = sub_slice[0].max;
        for i in 0..(count - 1) {
            left_min = left_min.min(sub_slice[i].min);
            left_max = left_max.max(sub_slice[i].max);
            let left_count = (i + 1) as f32;
            all_costs[i] += box_surface_area(left_min, left_max) * left_count;
        }

        // Обратный проход: добавляем площади правых коробок (от i+1 до конца)
        let mut right_min = sub_slice[count - 1].min;
        let mut right_max = sub_slice[count - 1].max;
        for i in (0..(count - 1)).rev() {
            right_min = right_min.min(sub_slice[i + 1].min);
            right_max = right_max.max(sub_slice[i + 1].max);
            let right_count = (count - 1 - i) as f32;
            all_costs[i] += box_surface_area(right_min, right_max) * right_count;
        }

        // Ищем индекс с минимальной стоимостью
        let mut min_cost = f32::INFINITY;
        let mut local_mid = 0;
        for i in 0..(count - 1) {
            if all_costs[i] < min_cost {
                min_cost = all_costs[i];
                local_mid = i;
            }
        }

        // Точка разделения: в левую часть уйдут элементы 0..=local_mid (всего local_mid + 1 штук)
        let mid = polygon_range.start + local_mid + 1;
        // let mid = (polygon_range.start + polygon_range.end) / 2;

        // Эвристический критерий остановки (необязательно, но полезно):
        // Если стоимость лучшего разделения выше, чем стоимость создания листа, делаем лист.
        // Для этого нужно ввести веса Ctrav и Cisect, пока считаем грубо:
        let leaf_cost = box_surface_area(total_min, total_max) * count as f32;
        if min_cost >= leaf_cost {
            return Self::new_leaf(polygons, polygon_range);
        }

        let left_range = polygon_range.start..mid;
        let right_range = mid..polygon_range.end;

        // Рекурсивно создаем детей
        let first_child = Self::new_bvh(polygons, left_range, max_layers - 1, max_polygons);
        let second_child = Self::new_bvh(polygons, right_range, max_layers - 1, max_polygons);

        Self {
            min: total_min - Vec3::splat(max_layers as f32 / 32. * 0.0001),
            max: total_max + Vec3::splat(max_layers as f32 / 32. * 0.0001),
            polygon_range,
            is_leaf: false,
            first_child: Some(Box::new(first_child)),
            second_child: Some(Box::new(second_child)),
        }
    }

    /// Превращает древовидную структуру в плоский массив для буфера видеокарты.
    /// Забирает дерево по значению (self), полностью разбирая его в процессе.
    pub fn to_gpu(self) -> Vec<ray_gpu::BvhNode> {
        let mut gpu_nodes = Vec::new();
        Self::push_me_and_my_children(self, &mut gpu_nodes);
        gpu_nodes
    }

    /// Рекурсивный метод сборки плоского массива.
    /// Метод забирает владение узлом (node) и уничтожает указатель Box, вытаскивая данные наружу.
    fn push_me_and_my_children(node: Self, gpu_nodes: &mut Vec<ray_gpu::BvhNode>) -> usize {
        // Запоминаем индекс, куда мы положим текущего родителя
        let my_index = gpu_nodes.len();

        // Создаем временную дефолтную GPU-ноду и сразу пушим её, чтобы занять место в векторе
        let dummy_box = ray_gpu::BvhNode {
            box_max: node.max,
            box_min: node.min,
            sec_child_or_first_poly: 0,
            poly_count: 0,
        };
        gpu_nodes.push(dummy_box);

        if node.is_leaf {
            // Если это лист, записываем параметры диапазона полигонов
            let count = (node.polygon_range.end - node.polygon_range.start) as i32;
            gpu_nodes[my_index].poly_count = count;
            gpu_nodes[my_index].sec_child_or_first_poly = node.polygon_range.start as i32;
        } else {
            // Если это внутренняя нода, у неё гарантированно есть дети.
            // Распаковываем Box-указатели из Option, забирая данные по значению (*child)
            let left_child = *node.first_child.unwrap();
            let right_child = *node.second_child.unwrap();

            // По вашей логике: левый (первый) ребенок ВСЕГДА ложится в массив сразу следующим:
            let left_idx = Self::push_me_and_my_children(left_child, gpu_nodes);
            assert_eq!(left_idx, my_index + 1, "Левый ребенок должен лежать строго на +1");

            // Правый ребенок уходит вглубь вектора. Метод вернет его итоговый индекс
            let right_idx = Self::push_me_and_my_children(right_child, gpu_nodes);

            // Записываем во внутреннюю ноду отрицательный маркер и адрес правого ребенка
            gpu_nodes[my_index].poly_count = -1; // Маркер, что это внутренняя нода
            gpu_nodes[my_index].sec_child_or_first_poly = right_idx as i32;
        }

        my_index
    }

    pub fn collect_stats(&self) -> BvhStats {
        let mut total_leaves = 0;
        let mut min_poly_in_leaf = u32::MAX;
        let mut max_poly_in_leaf = 0;
        let mut sum_polygons = 0u64;
        let mut min_depth = u32::MAX;
        let mut max_depth = 0;
        let mut sum_depths = 0u64;

        // Встроенная лямбда-функция (замыкание) для рекурсивного обхода без лишних структур
        fn walk(
            node: &BvhNode, 
            depth: u32,
            leaves: &mut u32,
            min_p: &mut u32, max_p: &mut u32, sum_p: &mut u64,
            min_d: &mut u32, max_d: &mut u32, sum_d: &mut u64,
        ) {
            if node.is_leaf {
                let count = node.polygon_range.end - node.polygon_range.start;
                *leaves += 1;
                *min_p = (*min_p).min(count as u32);
                *max_p = (*max_p).max(count as u32);

                *sum_p += count as u64;
                *min_d = (*min_d).min(depth);
                *max_d = (*max_d).max(depth);
                *sum_d += depth as u64;
            } else {
                if let Some(ref left) = node.first_child {
                    walk(left, depth + 1, leaves, min_p, max_p, sum_p, min_d, max_d, sum_d);
                }
                if let Some(ref right) = node.second_child {
                    walk(right, depth + 1, leaves, min_p, max_p, sum_p, min_d, max_d, sum_d);
                }
            }
        }

        walk(
            self, 0, 
            &mut total_leaves, 
            &mut min_poly_in_leaf, &mut max_poly_in_leaf, &mut sum_polygons,
            &mut min_depth, &mut max_depth, &mut sum_depths
        );

        let total_f = total_leaves as f32;
        let (avg_poly, avg_depth) = if total_leaves > 0 {
            (sum_polygons as f32 / total_f, sum_depths as f32 / total_f)
        } else {
            (0.0, 0.0)
        };

        BvhStats {
            total_leaves,
            min_poly_in_leaf,
            max_poly_in_leaf,
            avg_poly_in_leaf: avg_poly,
            min_depth,
            max_depth,
            avg_depth,
        }
    }
}

fn box_surface_area(box_min: Vec3, box_max: Vec3) -> f32 {
    let d = box_max - box_min; // Размеры коробки по осям X, Y, Z
    return 2.0 * (d.x * d.y + d.y * d.z + d.z * d.x);
}
