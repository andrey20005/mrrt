use glam::{Mat3, Vec2, Vec3};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use wgpu::util::DeviceExt;
use winit::keyboard::{KeyCode, PhysicalKey};
use crate::app_prelude::{AppLogic, AppState};
use crate::camera::Camera;
use crate::fps_counter::FrameTimeCounter;
use crate::polygon::{Polygon, PolygonSliceExt};
use crate::bvh::{self, BvhNode};
use crate::obj_parser;
// Подключаем оба новых шейдера вместо старого ray
use crate::shaders::{path_splitter, compositor}; 
use crate::texture_mapping::{RenderTexture, TextureMapping};

pub struct RayApp {
    // Наши вспомогательные инструменты
    render_texture: RenderTexture,
    texture_mapping: TextureMapping,
    
    // Два пайплайна для двух шейдеров
    path_splitter_pipeline: wgpu::ComputePipeline,
    compositor_pipeline: wgpu::ComputePipeline,
    
    // Две бинд-группы (используем правильные пути из wgsl_to_wgpu)
    bind_group_pass1: path_splitter::bind_groups::BindGroup0,
    bind_group_pass2: compositor::bind_groups::BindGroup0,
    
    // Буфферы с данными
    uniform_buffer:   wgpu::Buffer,
    polygons_buffer:  wgpu::Buffer,
    bvh_buffer:       wgpu::Buffer,
    compact_buffer:   wgpu::Buffer,
    center_buffer:    wgpu::Buffer,
    
    camera: Camera,
    
    // полезные данные
    start_time:     std::time::Instant,
    aspect:         glam::Vec2,
    pixel_size:     f32,
    polygons_count: u32,
    
    // для анализа частоты кадров
    fps_counter:        FrameTimeCounter,
    last_frame_instant: std::time::Instant,
    time_accumulator:   f32,
}

impl AppLogic for RayApp {
    fn new(state: &AppState) -> Self {
        // --- ЗАГРУЗКА ВСЕХ МОДЕЛЕЙ (БЕЗ ИЗМЕНЕНИЙ) ---
        let mut scene_polygons = Vec::new();
        let models_config = [
            ("cornell_box_walls_and_floor.obj",  Vec3::new(0.8, 0.8, 0.8),      -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_red_wall.obj",         Vec3::new(0.99, 0.05, 0.05),   -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_green_wall.obj",       Vec3::new(0.05, 0.99, 0.05),   -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_blue_wall.obj",        Vec3::new(0.05, 0.05, 0.99),   -1.0, Vec3::ZERO, Mat3::IDENTITY),
            ("cornell_box_lamp.obj",             Vec3::new(1.0, 1.0, 1.0) * 5., -2.0, Vec3::ZERO, Mat3::from_diagonal(Vec3::new(2., 1., 2.))),
            ("suzanne_low.obj", Vec3::new(0.9, 0.9, 0.9), 1.0, Vec3::new(0.35, 0.001, 0.51), Mat3::IDENTITY),
            ("dragon_low.obj",  Vec3::new(0.8, 0.7, 0.4), 1.0, Vec3::new(-0.11, 0.001, -0.42), Mat3::from_rotation_y(-40.0_f32.to_radians())),
            ("sphere_low.obj", Vec3::new(0.9, 0.7, 0.8), 0.0, Vec3::new(-0.43, 0.001, -0.04), Mat3::IDENTITY),
        ];
        
        for (file_name, color, mat_type, translation, rotation) in models_config {
            let path = format!("assets/models/{}", file_name);
            if let Ok(file_data) = std::fs::read_to_string(&path) {
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
        
        if scene_polygons.is_empty() {
            scene_polygons.push(Polygon::new(
                Vec3::new(-1.0, -1.0, -1.0), Vec3::new( 1.0, -1.0, -1.0),
                Vec3::new( 0.0,  1.0, -1.0), Vec3::new(1.0, 0.5, 0.0), 1.0,
            ));
        }
        
        let polygons_count = scene_polygons.len();
        let bvh_tree = BvhNode::new_bvh::<bvh::binned_sah_split::BinnedSahSplit>(&mut scene_polygons, 25, 4);

        // Конвертируем Полигоны из ray::Polygon в path_splitter::Polygon
        let gpu_polygons: Vec<path_splitter::Polygon> = scene_polygons.iter().map(|p| {
            let src = p.to_gpu(); // Возвращает твой старый ray::Polygon
            path_splitter::Polygon {
                global_to_local: src.global_to_local,
                normal: src.normal,
                origin: src.origin,
                color: src.color,
                t: src.t,
            }
        }).collect();

        // Конвертируем BVH из ray::BvhNode в path_splitter::BvhNode
        let gpu_bvh_nodes: Vec<path_splitter::BvhNode> = bvh_tree.to_gpu().into_iter().map(|src| {
            path_splitter::BvhNode {
                box_max: src.box_max,
                sec_child_or_first_poly: src.sec_child_or_first_poly,
                box_min: src.box_min,
                poly_count: src.poly_count,
            }
        }).collect();

        // --- УПАКОВКА В ENCASE (ИСПОЛЬЗУЕМ path_splitter ТИПЫ) ---
        let mut polygons_encase = encase::StorageBuffer::new(Vec::new());
        polygons_encase.write(&gpu_polygons).unwrap();
        let polygons_buffer = state.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Polygons Storage Buffer"),
            contents: &polygons_encase.into_inner(),
            usage: wgpu::BufferUsages::STORAGE,
        });
        
        let mut bvh_encase = encase::StorageBuffer::new(Vec::new());
        bvh_encase.write(&gpu_bvh_nodes).unwrap();
        let bvh_buffer = state.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("BVH Storage Buffer"),
            contents: &bvh_encase.into_inner(),
            usage: wgpu::BufferUsages::STORAGE,
        });
        
        let uniform_size = <path_splitter::Uniform as encase::ShaderType>::min_size();
        let uniform_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Raytracing Uniform Buffer"),
            size: uniform_size.get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- СОЗДАНИЕ PATH DATA BUFFER ---
        let render_texture = RenderTexture::new(state, 0.5);
        let (v_width, v_height) = render_texture.virtual_size();
        let pixel_count = (v_width * v_height) as u64;
        
        let compact_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Compact Buffer"),
            size: pixel_count * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let center_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Center Buffer"),
            size: pixel_count * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });


        // --- СБОРКА ПАЙПЛАЙНОВ ---
        let texture_mapping = TextureMapping::new(state);
        
        let path_splitter_pipeline = state.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Path Splitter Pipeline"),
            layout: Some(&path_splitter::create_pipeline_layout(&state.device)),
            module: &path_splitter::create_shader_module(&state.device),
            entry_point: Some(path_splitter::ENTRY_MAIN_PASS1),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        
        let compositor_pipeline = state.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compositor Pipeline"),
            layout: Some(&compositor::create_pipeline_layout(&state.device)),
            module: &compositor::create_shader_module(&state.device),
            // entry_point: Some(compositor::ENTRY_MAIN_PASS2),
            entry_point: Some(compositor::ENTRY_MAIN_DENOISE),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let bindings1 = path_splitter::bind_groups::BindGroupLayout0 {
            uf: uniform_buffer.as_entire_buffer_binding(),
            polygons: polygons_buffer.as_entire_buffer_binding(),
            bvh: bvh_buffer.as_entire_buffer_binding(),
            output_texture: render_texture.texture_view(),
            compact_buffer: compact_buffer.as_entire_buffer_binding(),
            center_buffer: center_buffer.as_entire_buffer_binding(),
        };
        let bind_group_pass1 = path_splitter::bind_groups::BindGroup0::from_bindings(&state.device, bindings1);

        let bindings2 = compositor::bind_groups::BindGroupLayout0 {
            uf: uniform_buffer.as_entire_buffer_binding(),
            compact_buffer: compact_buffer.as_entire_buffer_binding(),
            center_buffer: center_buffer.as_entire_buffer_binding(),
            output_texture: render_texture.texture_view(),
        };
        let bind_group_pass2 = compositor::bind_groups::BindGroup0::from_bindings(&state.device, bindings2);

        let w = state.size.width as f32;
        let h = state.size.height as f32;
        let aspect = Vec2::new(1.0f32.max(w / h), 1.0f32.max(h / w));
        let pixel_size = 2.0 / w.min(h);
        let camera = Camera::new(Vec3::new(-2.0, 0.9, 0.0), -6.0, 90.0, 1.5, false);

        Self {
            render_texture,
            texture_mapping,
            path_splitter_pipeline,
            compositor_pipeline,
            bind_group_pass1,
            bind_group_pass2,
            uniform_buffer,
            polygons_buffer,
            bvh_buffer,
            center_buffer,
            compact_buffer,
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
        self.render_texture.resize(state, new_size.width, new_size.height);
        let w = state.size.width as f32;
        let h = state.size.height as f32;
        self.aspect = Vec2::new(1.0f32.max(w / h), 1.0f32.max(h / w));
        self.pixel_size = 2.0 / w.min(h);

        let (v_width, v_height) = self.render_texture.virtual_size();
        let pixel_count = (v_width * v_height) as u64;

        self.compact_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Compact Buffer"),
            size: pixel_count * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        self.center_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Center Buffer"),
            size: pixel_count * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bindings1 = path_splitter::bind_groups::BindGroupLayout0 {
            uf: self.uniform_buffer.as_entire_buffer_binding(),
            polygons: self.polygons_buffer.as_entire_buffer_binding(),
            bvh: self.bvh_buffer.as_entire_buffer_binding(),
            output_texture: self.render_texture.texture_view(),
            compact_buffer: self.compact_buffer.as_entire_buffer_binding(),
            center_buffer: self.center_buffer.as_entire_buffer_binding(),
        };
        self.bind_group_pass1 = path_splitter::bind_groups::BindGroup0::from_bindings(&state.device, bindings1);

        let bindings2 = compositor::bind_groups::BindGroupLayout0 {
            uf: self.uniform_buffer.as_entire_buffer_binding(),
            compact_buffer: self.compact_buffer.as_entire_buffer_binding(),
            center_buffer: self.center_buffer.as_entire_buffer_binding(),
            output_texture: self.render_texture.texture_view(),
        };
        self.bind_group_pass2 = compositor::bind_groups::BindGroup0::from_bindings(&state.device, bindings2);
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
                        PhysicalKey::Code(KeyCode::F3) => {
                            self.render_texture.set_scale(state, 0.5);
                            self.resize(state, state.size);
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

        self.time_accumulator += delta_time;
        if self.time_accumulator >= 1.0 {
            let avg_fps = self.fps_counter.get_avg_fps(2.0);
            let low_1_fps = self.fps_counter.get_percentile_fps(0.01, 2.0);
            println!("FPS: {:.1} | 1% Low: {:.1}", avg_fps, low_1_fps);
            self.time_accumulator -= 1.0;
        }

        let elapsed = self.start_time.elapsed().as_secs_f32();
        
        // Uniform теперь берем из path_splitter
        let uniform_data = path_splitter::Uniform {
            time: elapsed,
            aspect: self.aspect,
            camera_mat: self.camera.rotation_matrix(),
            camera_pos: self.camera.position(),
            camera_zoom: self.camera.zoom(),
            pixel_size: self.pixel_size,
            background_color: Vec3::splat(0.1),
            polygons_count: self.polygons_count,
            bounces: 4,
            samples: 1,
        };

        let mut byte_buffer = encase::UniformBuffer::new(Vec::new());
        byte_buffer.write(&uniform_data).unwrap();
        state.queue.write_buffer(&self.uniform_buffer, 0, &byte_buffer.into_inner());

        let (v_width, v_height) = self.render_texture.virtual_size();
        let workgroup_x = (v_width + 15) / 16;
        let workgroup_y = (v_height + 15) / 16;

        // ==========================================
        // ШАГ 1: PATH SPLITTER (Сбор данных)
        // ==========================================
        {
            let mut cpass1 = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Pass 1: Path Splitter"),
                timestamp_writes: None,
            });
            cpass1.set_pipeline(&self.path_splitter_pipeline);
            path_splitter::set_bind_groups(&mut cpass1, &self.bind_group_pass1);
            cpass1.dispatch_workgroups(workgroup_x, workgroup_y, 1);
        } // <-- Завершение скобки создает неявный барьер синхронизации!

        // ==========================================
        // ШАГ 2: COMPOSITOR (Сборка изображения)
        // ==========================================
        {
            let mut cpass2 = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Pass 2: Compositor"),
                timestamp_writes: None,
            });
            cpass2.set_pipeline(&self.compositor_pipeline);
            compositor::set_bind_groups(&mut cpass2, &self.bind_group_pass2);
            cpass2.dispatch_workgroups(workgroup_x, workgroup_y, 1);
        }

        // --- ВЫВОД РЕЗУЛЬТАТА НА ЭКРАН (RENDER PASS) ---
        self.texture_mapping.render(
            self.render_texture.render_bind_group(),
            state,
            view,
            encoder,
        );
        self.fps_counter.tick();
    }
}