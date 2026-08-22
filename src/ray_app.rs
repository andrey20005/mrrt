use glam::{Mat3, Vec2, Vec3};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use wgpu::util::DeviceExt;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::app_prelude::{AppLogic, AppState};
use crate::camera::Camera;
use crate::fps_counter::FrameTimeCounter;
use crate::polygon::{Polygon, PolygonSliceExt};
use crate::bvh::BvhNode;
use crate::obj_parser;
use crate::shaders::ray;
use crate::texture_mapping::{RenderTexture, TextureMapping};

pub struct RayApp {
    // Наши вспомогательные инструменты
    render_texture: RenderTexture,
    texture_mapping: TextureMapping,

    // Ресурсы для Compute-пасса
    compute_pipeline: wgpu::ComputePipeline,
    bind_group0: ray::bind_groups::BindGroup0,

    // буфферы с данными
    uniform_buffer:  wgpu::Buffer,
    polygons_buffer: wgpu::Buffer,
    bvh_buffer:      wgpu::Buffer,
    camera:          Camera,
    
    // полезные данные
    start_time:  std::time::Instant,
    aspect:         glam::Vec2,
    pixel_size:     f32,
    polygons_count: u32,

    // для анализа частоты кадров
    fps_counter:        crate::fps_counter::FrameTimeCounter,
    last_frame_instant: std::time::Instant,
    time_accumulator:   f32,
}

impl AppLogic for RayApp {
    fn new(state: &AppState) -> Self {
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

        // --- УПАКОВКА В СТРУКТУРЫ ENCASE ДЛЯ GPU ---
        
        let gpu_polygons: Vec<ray::Polygon> = scene_polygons.iter().map(|p| p.to_gpu()).collect();
        let gpu_bvh_nodes: Vec<ray::BvhNode> = bvh_tree.to_gpu();

        let mut polygons_encase = encase::StorageBuffer::new(Vec::new());
        polygons_encase.write(&gpu_polygons).unwrap();
        let polygons_bytes = polygons_encase.into_inner();

        let polygons_buffer = state.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Polygons Storage Buffer"),
            contents: &polygons_bytes, // Передаем корректные encase-байты
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Используем encase::StorageBuffer для BVH нод
        let mut bvh_encase = encase::StorageBuffer::new(Vec::new());
        bvh_encase.write(&gpu_bvh_nodes).unwrap();
        let bvh_bytes = bvh_encase.into_inner();

        let bvh_buffer = state.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("BVH Storage Buffer"),
            contents: &bvh_bytes, // Передаем корректные encase-байты
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Создаем Uniform-буфер
        let uniform_size = <ray::Uniform as encase::ShaderType>::min_size();
        let uniform_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Raytracing Uniform Buffer"),
            size: uniform_size.get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- СБОРКА ПАЙПЛАЙНА ---
        let render_texture = RenderTexture::new(state, 0.5);
        let texture_mapping = TextureMapping::new(state);

        let pipeline_layout = ray::create_pipeline_layout(&state.device);
        let compute_module = ray::create_shader_module(&state.device);

        let bindings = ray::bind_groups::BindGroupLayout0 {
            uf: uniform_buffer.as_entire_buffer_binding(),
            polygons: polygons_buffer.as_entire_buffer_binding(),
            bvh: bvh_buffer.as_entire_buffer_binding(),
            output_texture: render_texture.texture_view(),
        };
        let bind_group0 = ray::bind_groups::BindGroup0::from_bindings(&state.device, bindings);

        let compute_pipeline = state.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Gradients Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &compute_module,
            entry_point: Some(ray::ENTRY_MAIN),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let w = state.size.width as f32;
        let h = state.size.height as f32;
        let aspect = Vec2::new(1.0f32.max(w / h), 1.0f32.max(h / w));
        let pixel_size = 2.0 / w.min(h);

        let camera = Camera::new(Vec3::new(-2.0, 0.9, 0.0), -6.0, 90.0, 1.5, false);

        Self {
            render_texture,
            texture_mapping,
            compute_pipeline,
            bind_group0,
            uniform_buffer,
            polygons_buffer,
            bvh_buffer,
            camera,
            start_time: std::time::Instant::now(),
            aspect,
            pixel_size,
            polygons_count: polygons_count as u32,
            fps_counter: FrameTimeCounter::new(5.0),
            last_frame_instant: std::time::Instant::now(),
            time_accumulator: 0.0,
        }
    }

    fn resize(&mut self, state: &AppState, new_size: PhysicalSize<u32>) {
        // Пересчитываем размеры нашей текстуры рендера
        self.render_texture.resize(state, new_size.width, new_size.height);

        let w = state.size.width as f32;
        let h = state.size.height as f32;
        self.aspect = Vec2::new(1.0f32.max(w / h), 1.0f32.max(h / w));
        self.pixel_size = 2.0 / w.min(h);

        // ВАЖНО: Так как текстура внутри render_texture пересоздалась, её старый TextureView
        // стал невалидным. Нам нужно обновить бинд-группу вычислительного шейдера!
        let bindings = ray::bind_groups::BindGroupLayout0 {
            uf: self.uniform_buffer.as_entire_buffer_binding(),
            polygons: self.polygons_buffer.as_entire_buffer_binding(),
            bvh: self.bvh_buffer.as_entire_buffer_binding(),
            output_texture: self.render_texture.texture_view(),
        };
        self.bind_group0 = ray::bind_groups::BindGroup0::from_bindings(&state.device, bindings);
    }

    fn handle_input(&mut self, state: &AppState, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if key_event.state == winit::event::ElementState::Pressed {
                    match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::F1) => {
                            self.render_texture.set_pixel_art_mode(state, true);
                            let (vw, vh) = self.render_texture.virtual_size();
                            println!("🎨 Сглаживание: Nearest. Разрешение: {}x{}", vw, vh);
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::F2) => {
                            self.render_texture.set_pixel_art_mode(state, false);
                            let (vw, vh) = self.render_texture.virtual_size();
                            println!("🎬 Сглаживание:  Linear. Разрешение: {}x{}", vw, vh);
                            return true;
                        }
                        // Изменение масштаба рендера для теста производительности (F3 - 50%, F4 - 100%)
                        PhysicalKey::Code(KeyCode::F3) => {
                            self.render_texture.set_scale(state, 0.5);
                            self.resize(state, state.size); // Пересоздаем бинд-группы
                            let (vw, vh) = self.render_texture.virtual_size();
                            println!("🚀 Масштаб рендера:  50%. Разрешение: {}x{}", vw, vh);
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::F4) => {
                            self.render_texture.set_scale(state, 1.0);
                            self.resize(state, state.size);
                            let (vw, vh) = self.render_texture.virtual_size();
                            println!("🖥️ Масштаб рендера: 100%. Разрешение: {}x{}", vw, vh);
                            return true;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        self.camera.handle_input(event)
    }

    fn handle_mouse_motion(&mut self, _state: &AppState, dx: f64, dy: f64) {
        self.camera.handle_mouse_motion(dx, dy);
    }

    fn render(
        &mut self,
        state: &AppState,
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
        state.queue.write_buffer(&self.uniform_buffer, 0, &byte_buffer.into_inner());

        // --- ЗАПУСК ВЫЧИСЛИТЕЛЬНОГО ШЕЙДЕРА (COMPUTE PASS) ---
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Gradients Compute Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            
            // Применяем сгенерированную бинд-группу для вычислений
            ray::set_bind_groups(&mut cpass, &self.bind_group0);

            // Получаем виртуальный размер нашей текстуры рендера
            let (v_width, v_height) = self.render_texture.virtual_size();
            
            // Считаем сетку рабочих групп (делим размер текстуры на размер группы 16х16 с округлением вверх)
            let workgroup_x = (v_width + 15) / 16;
            let workgroup_y = (v_height + 15) / 16;
            
            cpass.dispatch_workgroups(workgroup_x, workgroup_y, 1);
        }

        // --- ВЫВОД РЕЗУЛЬТАТА НА ЭКРАН (RENDER PASS) ---
        // Передаем готовую бинд-группу фрагментного шейдера из текстуры в наш отрисовщик
        self.texture_mapping.render(
            self.render_texture.render_bind_group(),
            state,
            view,
            encoder,
        );

        self.fps_counter.tick();
    }
}
