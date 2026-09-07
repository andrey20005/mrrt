use std::ops::Range;
use glam::Vec3;
use crate::polygon::Polygon;
use crate::shaders::ray as ray_gpu;

pub mod sah_split;
pub mod binned_sah_split;
pub mod median_split;

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

/// Трейт для стратегий разделения BVH
pub trait Splitter: Sized + Send + Sync {
    fn new(polygons: &mut [Polygon], bounds_min: Vec3, bounds_max: Vec3) -> Self;
    /// Разделяет срез на две части. 
    /// Возвращает индекс `mid`, так что [0..mid] идет в левое поддерево, а [mid..] в правое.
    /// Срез мутируется (полигоны переставляются).
    /// Возвращает дополнительные данные для обоих детей
    fn split(&mut self, polygons: &mut [Polygon], bounds_min: Vec3, bounds_max: Vec3) -> (usize, Self, Self);
}

const RAYON_THRESHOLD: usize = 5_000;

impl BvhNode {
    /// Создает узел-лист. Он просто рассчитывает общие границы для своего диапазона полигонов.
    pub fn new_leaf(slice: &[Polygon], polygon_range: Range<usize>) -> Self {
        // Итерируемся по всему переданному локальному срезу, не используя polygon_range для доступа
        let mut min = slice[0].min;
        let mut max = slice[0].max;
        for poly in slice.iter() {
            min = min.min(poly.min);
            max = max.max(poly.max);
        }
        
        Self {
            min,
            max,
            polygon_range, // Сохраняем глобальные индексы для корректной работы to_gpu()
            is_leaf: true,
            first_child: None,
            second_child: None,
        }
    }

    /// Рекурсивно строит BVH-дерево по медиане вдоль самой длинной оси контейнера, с поддержкой многопоточности.
    pub fn new_bvh<S: Splitter + Send + Sync>(
        polygons: &mut [Polygon],
        max_layers: usize,
        max_polygons: usize,
    ) -> Self {
        let mut total_min = polygons[0].min;
        let mut total_max = polygons[0].max;
        for poly in polygons.iter() {
            total_min = total_min.min(poly.min);
            total_max = total_max.max(poly.max);
        }

        let root_splitter = S::new(polygons, total_min, total_max);

        Self::build_recursive::<S>(polygons, 0, max_layers, max_polygons, root_splitter)
    }

    /// Рекурсивное построение с поддержкой многопоточности.
    fn build_recursive<S: Splitter + Send + Sync>(
        slice: &mut [Polygon],
        global_start: usize,
        max_layers: usize,
        max_polygons: usize,
        mut splitter: S,
    ) -> Self {
        let count = slice.len();
        let polygon_range = global_start..(global_start + count);

        if max_layers == 0 || count <= max_polygons {
            return Self::new_leaf(slice, polygon_range);
        }

        let mut total_min = slice[0].min;
        let mut total_max = slice[0].max;
        for poly in slice.iter() {
            total_min = total_min.min(poly.min);
            total_max = total_max.max(poly.max);
        }

        let (local_mid, left_splitter, right_splitter) = splitter.split(slice, total_min, total_max);
        
        let (left_slice, right_slice) = slice.split_at_mut(local_mid);

        let (first_child, second_child) = if count > RAYON_THRESHOLD {
            rayon::join(
                || Self::build_recursive::<S>(left_slice, global_start, max_layers - 1, max_polygons, left_splitter),
                || Self::build_recursive::<S>(right_slice, global_start + local_mid, max_layers - 1, max_polygons, right_splitter),
            )
        } else {
            (
                Self::build_recursive::<S>(left_slice, global_start, max_layers - 1, max_polygons, left_splitter),
                Self::build_recursive::<S>(right_slice, global_start + local_mid, max_layers - 1, max_polygons, right_splitter),
            )
        };

        Self {
            min: total_min,
            max: total_max,
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
