use std::ops::Range;
use glam::Vec3;
use anyhow::{Context, Result, anyhow};
use crate::polygon::Polygon;

/// Парсит текст OBJ-файла и ДОПИСЫВАЕТ полигоны в существующий вектор.
/// Возвращает диапазон индексов (Range<usize>), куда была записана модель.
pub fn parse_obj_into_vector(
    obj_data: &str,
    color: Vec3,
    material_type: f32,
    destination: &mut Vec<Polygon>,
) -> Result<Range<usize>> { // ИСПРАВЛЕНО: Теперь возвращаем Range<usize>
    // Запоминаем стартовый индекс как usize
    let start_index = destination.len(); 

    let mut vertices: Vec<Vec3> = Vec::new();

    for (line_idx, line) in obj_data.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();
        
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        match parts.next() {
            Some("v") => {
                let x_str = parts.next().ok_or_else(|| anyhow!("Строка {}: отсутствует координата X", line_num))?;
                let y_str = parts.next().ok_or_else(|| anyhow!("Строка {}: отсутствует координата Y", line_num))?;
                let z_str = parts.next().ok_or_else(|| anyhow!("Строка {}: отсутствует координата Z", line_num))?;

                let x: f32 = x_str.parse().with_context(|| format!("Строка {}: ошибка парсинга X '{}'", line_num, x_str))?;
                let y: f32 = y_str.parse().with_context(|| format!("Строка {}: ошибка парсинга Y '{}'", line_num, y_str))?;
                let z: f32 = z_str.parse().with_context(|| format!("Строка {}: ошибка парсинга Z '{}'", line_num, z_str))?;

                vertices.push(Vec3::new(x, y, z));
            }
            Some("f") => {
                let mut face_indices = Vec::new();
                for part in parts {
                    let idx_str = part.split('/').next().unwrap_or("0");
                    if let Ok(mut idx) = idx_str.parse::<usize>() {
                        if idx > 0 {
                            idx -= 1;
                        }
                        face_indices.push(idx);
                    } else {
                        return Err(anyhow!("Строка {}: неверный формат индексa грани '{}'", line_num, part));
                    }
                }

                if face_indices.len() < 3 {
                    continue;
                }

                let v0_idx = face_indices[0];
                if v0_idx >= vertices.len() {
                    return Err(anyhow!("Строка {}: Индекс указывает на несуществующую вершину", line_num));
                }

                for i in 1..(face_indices.len() - 1) {
                    let v1_idx = face_indices[i];
                    let v2_idx = face_indices[i + 1];

                    if v1_idx >= vertices.len() || v2_idx >= vertices.len() {
                        return Err(anyhow!("Строка {}: Индекс указывает на несуществующую вершину", line_num));
                    }

                    let v1 = vertices[v0_idx];
                    let v2 = vertices[v1_idx];
                    let v3 = vertices[v2_idx];

                    let polygon = Polygon::new(v1, v2, v3, color, material_type);
                    destination.push(polygon);
                }
            }
            _ => {}
        }
    }

    let end_index = destination.len();
    
    // Возвращаем чистый диапазон usize
    Ok(start_index..end_index)
}
