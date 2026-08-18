use glam::{Mat3, Mat4, Vec3};
use crate::shaders::ray as ray_gpu; // Доступ к сгенерированной wgsl_to_wgpu структуре Polygon

/// Тяжелая CPU-структура полигона для логики, парсинга и построения BVH
#[derive(Debug, Clone)]
pub struct Polygon {
    // Входные базовые данные
    pub v1: Vec3,
    pub v2: Vec3,
    pub v3: Vec3,
    pub color: Vec3,
    pub t: f32,

    // Вычисляемые на CPU данные (зависят от координат вершин)
    pub normal: Vec3,
    pub min: Vec3,
    pub max: Vec3,
    pub global_to_local: Mat3,
}

impl Polygon {
    /// Конструктор принимает только базовые данные, а остальное считает сам
    pub fn new(v1: Vec3, v2: Vec3, v3: Vec3, color: Vec3, t: f32) -> Self {
        let mut poly = Self {
            v1,
            v2,
            v3,
            color,
            t,
            // Инициализируем временными дефолтными значениями перед вызовом update
            normal: Vec3::ZERO,
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            global_to_local: Mat3::IDENTITY,
        };

        // Принудительно запускаем пересчет зависимых полей
        poly.update();
        poly
    }

    /// Пересчитывает нормаль, границы AABB и матрицу трансформации на основе текущих координат вершин
    pub fn update(&mut self) {
        // 1. Вычисляем ребра треугольника
        let edge1 = self.v2 - self.v1;
        let edge2 = self.v3 - self.v1;

        // 2. Считаем и нормализуем нормаль
        self.normal = edge1.cross(edge2).normalize_or_zero() * -1.0;

        // 3. Считаем ограничивающий контейнер (AABB) для BVH
        self.min = self.v1.min(self.v2).min(self.v3);
        self.max = self.v1.max(self.v2).max(self.v3);

        // 4. Строим матрицу перехода global_to_local.
        // Нам нужен базис, где edge1 и edge2 станут осями X и Y локального пространства, 
        // а нормаль — осью Z.
        let local_to_global = Mat3::from_cols(edge1, edge2, self.normal);
        
        // Инвертируем её, чтобы получить матрицу перехода ИЗ глобальных В локальные координаты
        // Если треугольник вырожденный (вершины на одной линии), обратная матрица вернет IDENTITY
        self.global_to_local = local_to_global.inverse();
    }

    /// Конвертирует тяжелый CPU полигон в ультра-компактный GPU вариант для WebGPU Storage буфера
    pub fn to_gpu(&self) -> ray_gpu::Polygon {
        ray_gpu::Polygon {
            global_to_local: self.global_to_local,
            normal: self.normal,
            origin: self.v1, // Берем первую вершину как точку отсчета пространства
            color: self.color,
            t: self.t,
        }
    }
}

// --- ТРЕЙТ РАСШИРЕНИЯ ДЛЯ УДОБНОГО СИНТАКСИСА СРЕЗОВ ---

pub trait PolygonSliceExt {
    /// Сдвигает все полигоны в срезе на вектор сдвига
    fn translate(&mut self, offset: Vec3);
    /// Трансформирует все полигоны в срезе с помощью матрицы 4х4
    fn transform(&mut self, matrix: Mat4);
}

// Реализуем этот трейт для любого изменяемого среза полигонов
impl PolygonSliceExt for [Polygon] {
    fn translate(&mut self, offset: Vec3) {
        for poly in self.iter_mut() {
            poly.v1 += offset;
            poly.v2 += offset;
            poly.v3 += offset;
            
            // Обязательный вызов пересчета зависимых полей
            poly.update();
        }
    }

    fn transform(&mut self, matrix: Mat4) {
        for poly in self.iter_mut() {
            // Умножаем Vec3 на Mat4 (с учетом четвертой компоненты w = 1.0 для корректного переноса)
            poly.v1 = matrix.transform_point3(poly.v1);
            poly.v2 = matrix.transform_point3(poly.v2);
            poly.v3 = matrix.transform_point3(poly.v3);
            
            // Обязательный вызов пересчета зависимых полей
            poly.update();
        }
    }
}
