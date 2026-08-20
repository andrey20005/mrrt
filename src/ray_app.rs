use std::sync::Arc;
use glam::{Mat3, Vec2, Vec3};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::fps_counter::FrameTimeCounter;
use crate::system::AppLogic;
use crate::polygon::{Polygon, PolygonSliceExt};
use crate::bvh::BvhNode;
use crate::obj_parser;
use crate::shaders::ray;

pub struct RayApp {
    render_pipeline: wgpu::RenderPipeline,
    
    uniform_buffer:  wgpu::Buffer,
    polygons_buffer: wgpu::Buffer,
    bvh_buffer:      wgpu::Buffer,
    camera:          Camera,
    
    bind_group0: ray::bind_groups::BindGroup0,
    start_time:  std::time::Instant,

    pw:             u32,
    ph:             u32,
    aspect:         glam::Vec2,
    pixel_size:     f32,
    polygons_count: u32,

    fps_counter:        crate::fps_counter::FrameTimeCounter,
    last_frame_instant: std::time::Instant,
    time_accumulator:   f32,
}

impl AppLogic for RayApp {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        window: Arc<Window>,
    ) -> Self {
        // --- ЗАГРУЗКА ВСЕХ МОДЕЛЕЙ ---
        let mut scene_polygons = Vec::new();

        // Конфигурация всей сцены в одном массиве
        // Кортеж содержит: (Имя файла, Цвет, Тип материала, Вектор смещения, Матрица поворота)
        let models_config = [
            // Системная коробка Корнелла (белая, красная, зеленая стены и лампа)
            ("cornell_box_walls_and_floor.obj",  Vec3::new(0.8, 0.8, 0.8),      -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_red_wall.obj",         Vec3::new(0.99, 0.05, 0.05),   -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_green_wall.obj",       Vec3::new(0.05, 0.99, 0.05),   -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_blue_wall.obj",        Vec3::new(0.05, 0.05, 0.99),   -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_lamp.obj",             Vec3::new(1.0, 1.0, 1.0) * 5., -2.0, Vec3::ZERO, Mat3::from_diagonal(Vec3::new(2., 1., 2.))),

            // Пример: Сюзанна (зеркальная, сдвинута влево)
            // ("suzanne.obj", Vec3::new(0.9, 0.9, 0.9), 1.0, Vec3::new(0.35, 0.001, 0.51), Mat4::IDENTITY),
            ("suzanne_low.obj", Vec3::new(0.9, 0.9, 0.9), 1.0, Vec3::new(0.35, 0.001, 0.51), Mat3::IDENTITY),
            
            // Пример: Дракон (полуматовый, развернут и сдвинут вправо)
            // ("dragon.obj",  Vec3::new(0.8, 0.7, 0.4), 0.5, Vec3::new(-0.11, 0.001, -0.42), Mat3::from_rotation_y(-40.0_f32.to_radians())),
            ("dragon_low.obj",  Vec3::new(0.8, 0.7, 0.4), 0.5, Vec3::new(-0.11, 0.001, -0.42), Mat3::from_rotation_y(-40.0_f32.to_radians())),
            
            // Пример: Сфера (матовая, приподнята)
            // ("sphere.obj",  Vec3::new(0.99, 0.87, 0.91), 0.0, Vec3::new(-0.43, 0.001, -0.04), Mat4::IDENTITY),
            ("sphere_low.obj", Vec3::new(0.9, 0.7, 0.8), 0.0, Vec3::new(-0.43, 0.001, -0.04), Mat3::IDENTITY),
        ];

        for (file_name, color, mat_type, translation, rotation) in models_config {
            let path = format!("assets/models/{}", file_name);
            if let Ok(file_data) = std::fs::read_to_string(&path) {
                // Парсер сразу возвращает чистый Range<usize>
                if let Ok(r) = obj_parser::parse_obj_into_vector(&file_data, color, mat_type, &mut scene_polygons) {
                    if !r.is_empty() {
                        let model_slice = &mut scene_polygons[r];
                        model_slice.transform(rotation);
                        model_slice.translate(translation);
                        model_slice.transform(Mat3::from_diagonal(Vec3::new(1., 1., -1.)));
                    }
                } else { log::error!("Ошибка: Не удалось распарсить файл {}", file_name); }
            } else { log::warn!("Предупреждение: Не удалось прочитать файл {}", path); }
        }

        // Страховочный треугольник, если папка assets пуста
        if scene_polygons.is_empty() {
            scene_polygons.push(Polygon::new(
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new( 1.0, -1.0, -1.0),
                Vec3::new( 0.0,  1.0, -1.0),
                Vec3::new(1.0, 0.5, 0.0),
                1.0,
            ));
        }
        let polygons_count = scene_polygons.len();

        // Строим BVH дерево
        let total_range = 0..polygons_count;
        let bvh_start_time = std::time::Instant::now();
        // Строим дерево
        let bvh_tree = BvhNode::new_bvh(&mut scene_polygons, total_range, 30, 3);

        let bvh_duration = bvh_start_time.elapsed();
        let bvh_stats = bvh_tree.collect_stats();
        println!("==================================================");
        println!(" СТАТИСТИКА ГЕОМЕТРИЧЕСКОГО ЯДРА ДВИЖКА ");
        println!("==================================================");
        println!("Успешно загружено полигонов: {}", polygons_count);
        println!("Время построения BVH-дерева: {:?}", bvh_duration);
        println!("--------------------------------------------------");
        println!("Всего листьев в дереве:      {}", bvh_stats.total_leaves);
        println!("Глубина дерева (слои):      Мин: {}, Макс: {}, Средняя: {:.2}", bvh_stats.min_depth, bvh_stats.max_depth, bvh_stats.avg_depth);
        println!("Полигонов в одном листе:    Мин: {}, Макс: {}, Среднее: {:.2}", bvh_stats.min_poly_in_leaf, bvh_stats.max_poly_in_leaf, bvh_stats.avg_poly_in_leaf);
        println!("==================================================");

        // --- ЧАСТЬ 2: УПАКОВКА В СТРУКТУРЫ ENCASE ДЛЯ GPU ---
        
        let gpu_polygons: Vec<ray::Polygon> = scene_polygons.iter().map(|p| p.to_gpu()).collect();
        let gpu_bvh_nodes: Vec<ray::BvhNode> = bvh_tree.to_gpu();

        // Вместо bytemuck используем encase::StorageBuffer для полигонов
        let mut polygons_encase = encase::StorageBuffer::new(Vec::new());
        polygons_encase.write(&gpu_polygons).unwrap();
        let polygons_bytes = polygons_encase.into_inner();

        let polygons_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Polygons Storage Buffer"),
            contents: &polygons_bytes, // Передаем корректные encase-байты
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Используем encase::StorageBuffer для BVH нод
        let mut bvh_encase = encase::StorageBuffer::new(Vec::new());
        bvh_encase.write(&gpu_bvh_nodes).unwrap();
        let bvh_bytes = bvh_encase.into_inner();

        let bvh_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("BVH Storage Buffer"),
            contents: &bvh_bytes, // Передаем корректные encase-байты
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Создаем Uniform-буфер
        let uniform_size = <ray::Uniform as encase::ShaderType>::min_size();
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Raytracing Uniform Buffer"),
            size: uniform_size.get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- ЧАСТЬ 3: СБОРКА ПАЙПЛАЙНА ---
        let pipeline_layout = ray::create_pipeline_layout(device);

        let bindings = ray::bind_groups::BindGroupLayout0 {
            uf: uniform_buffer.as_entire_buffer_binding(),
            polygons: polygons_buffer.as_entire_buffer_binding(),
            bvh: bvh_buffer.as_entire_buffer_binding(),
        };
        let bind_group0 = ray::bind_groups::BindGroup0::from_bindings(device, bindings);

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Raytracing Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ray::create_shader_module(device),
                entry_point: Some(ray::ENTRY_VERTEX_MAIN),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &ray::create_shader_module(device),
                entry_point: Some(ray::ENTRY_FRAGMENT_MAIN),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let size = window.inner_size();
        let w = size.width as f32;
        let h = size.height as f32;
        let aspect = Vec2::new(1.0f32.max(w / h), 1.0f32.max(h / w));
        let pixel_size = 2.0 / w.min(h);

        let camera = Camera::new(Vec3::new(-2.0, 0.9, 0.0), -6.0, 90.0, 1.5, false);

        Self {
            render_pipeline,
            uniform_buffer,
            polygons_buffer,
            bvh_buffer,
            camera,
            bind_group0,
            start_time: std::time::Instant::now(),
            pw: size.width,
            ph: size.height,
            aspect,
            pixel_size,
            polygons_count: polygons_count as u32,
            fps_counter: FrameTimeCounter::new(5.0),
            last_frame_instant: std::time::Instant::now(),
            time_accumulator: 0.0,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.pw = new_size.width;
        self.ph = new_size.height;
        let w = new_size.width as f32;
        let h = new_size.height as f32;
        self.aspect = Vec2::new(1.0f32.max(w / h), 1.0f32.max(h / w));
        self.pixel_size = 2.0 / w.min(h);
    }

    fn handle_input(&mut self, event: &WindowEvent) -> bool {
        self.camera.handle_input(event)
    }

    fn handle_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.camera.handle_mouse_motion(dx, dy);
    }

    fn render(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let now = std::time::Instant::now();
        let delta_time = now.duration_since(self.last_frame_instant).as_secs_f32();
        self.last_frame_instant = now;

        self.camera.update_position(delta_time);

        // Обновляем таймер вывода FPS
        self.time_accumulator += delta_time;
        if self.time_accumulator >= 1.0 {
            let avg_fps = self.fps_counter.get_avg_fps(2.0);
            let low_1_fps = self.fps_counter.get_percentile_fps(0.01, 2.0);
            println!("FPS: {:.1} | 1% Low: {:.1}", avg_fps, low_1_fps);
            self.time_accumulator -= 1.0;
        }

        let elapsed = self.start_time.elapsed().as_secs_f32();

        let uniform_data = ray::Uniform {
            time: elapsed,
            pw: self.pw,
            ph: self.ph, 
            aspect: self.aspect,
            camera_mat: self.camera.rotation_matrix(),
            camera_pos: self.camera.position(),
            camera_zoom: self.camera.zoom(),
            // camera_mat: Mat3::from_rotation_y(90_f32.to_radians()) * Mat3::from_rotation_x(6_f32.to_radians()),
            // camera_pos: Vec3::new(-5.0, 1.46, 0.0),
            // camera_zoom: 4.,
            pixel_size: self.pixel_size,
            background_color: Vec3::splat(0.1),
            polygons_count: self.polygons_count,
            bounces: 4,
            samples: 1,
            graphics_mode: 1,
        };

        let mut byte_buffer = encase::UniformBuffer::new(Vec::new());
        byte_buffer.write(&uniform_data).unwrap();
        queue.write_buffer(&self.uniform_buffer, 0, &byte_buffer.into_inner());

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Raytracing Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None, // ИСПРАВЛЕНО: Добавлено обязательное поле для wgpu 30.0
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        render_pass.set_pipeline(&self.render_pipeline);
        ray::set_bind_groups(&mut render_pass, &self.bind_group0);
        render_pass.draw(0..6, 0..1);

        // let now = std::time::Instant::now();

        // // println!("{:.1}", now.duration_since(self.last_frame_instant).as_secs_f32());
        // if now.duration_since(self.last_frame_instant).as_secs_f32() >= 3. {
        //     println!("FPS: {:.1} | 1% Low: {:.1}", self.fps_counter.get_avg_fps(1.), self.fps_counter.get_percentile_fps(0.01, 1.));
        //     self.last_frame_instant = now;
        // }
        self.fps_counter.tick();
    }
}
