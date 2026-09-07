use glam::Vec3;
use crate::polygon::Polygon;
use super::Splitter;

pub struct MedianSplit;

impl Splitter for MedianSplit {
    fn new(_polygons: &mut [Polygon], _bounds_min: Vec3, _bounds_max: Vec3) -> Self {
        MedianSplit
    }

    fn split(&mut self, polygons: &mut [Polygon], bounds_min: Vec3, bounds_max: Vec3) -> (usize, Self, Self) {
        let count = polygons.len();
        if count < 2 {
            return (0, MedianSplit, MedianSplit);
        }

        let size = bounds_max - bounds_min;
        let axis = if size.y > size.x { 1 } else { 0 };
        let axis = if size.z > size[axis] { 2 } else { axis };

        let mid = count / 2;
        
        // O(N) разбиение без полной сортировки. 
        // Гарантирует, что все элементы левее mid меньше или равны элементам правее.
        polygons.select_nth_unstable_by(mid, |a, b| {
            let ca = (a.v1[axis] + a.v2[axis] + a.v3[axis]) / 3.0;
            let cb = (b.v1[axis] + b.v2[axis] + b.v3[axis]) / 3.0;
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });

        (mid, MedianSplit, MedianSplit)
    }
}