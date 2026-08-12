use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;
use wgpu::util::DeviceExt; // Крейт утилиты для удобного создания буферов из данных

use crate::system::AppLogic;
use crate::shaders::instance_cube; // Наш сгенерированный генератором модуль

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TriangleInstance {
    pub v1: glam::Vec3,
    pub v2: glam::Vec3,
    pub v3: glam::Vec3,
    pub normal: glam::Vec3,
}

impl TriangleInstance {
    pub fn new(v1: glam::Vec3, v2: glam::Vec3, v3: glam::Vec3) -> Self {
        // Считаем два ребра треугольника
        let edge1 = v2 - v1;
        let edge2 = v3 - v1;
        // Векторное произведение дает перпендикуляр (нормаль)
        // .normalize_or_zero() гарантирует, что длина вектора станет равной 1.0
        let normal = edge1.cross(edge2).normalize_or_zero();

        Self { v1, v2, v3, normal }
    }
}

pub struct CubeApp {
    instance_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group0: instance_cube::bind_groups::BindGroup0,
    render_pipeline: wgpu::RenderPipeline,
    start_time: std::time::Instant,
}

impl AppLogic for CubeApp {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        _window: Arc<Window>,
    ) -> Self {
        let instance_cube_tris = [
            // 1. ПЕРЕДНЯЯ ГРАНЬ (Z = -0.5)
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5, -0.5), glam::Vec3::new(-0.5,  0.5, -0.5), glam::Vec3::new( 0.5,  0.5, -0.5)),
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5, -0.5), glam::Vec3::new( 0.5,  0.5, -0.5), glam::Vec3::new( 0.5, -0.5, -0.5)),

            // 2. ЗАДНЯЯ ГРАНЬ (Z = 0.5)
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5,  0.5), glam::Vec3::new( 0.5,  0.5,  0.5), glam::Vec3::new(-0.5,  0.5,  0.5)),
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5,  0.5), glam::Vec3::new( 0.5, -0.5,  0.5), glam::Vec3::new( 0.5,  0.5,  0.5)),

            // 3. ЛЕВАЯ ГРАНЬ (X = -0.5)
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5,  0.5), glam::Vec3::new(-0.5,  0.5, -0.5), glam::Vec3::new(-0.5, -0.5, -0.5)),
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5,  0.5), glam::Vec3::new(-0.5,  0.5,  0.5), glam::Vec3::new(-0.5,  0.5, -0.5)),

            // 4. ПРАВАЯ ГРАНЬ (X = 0.5)
            TriangleInstance::new(glam::Vec3::new( 0.5, -0.5, -0.5), glam::Vec3::new( 0.5,  0.5, -0.5), glam::Vec3::new( 0.5, -0.5,  0.5)),
            TriangleInstance::new(glam::Vec3::new( 0.5, -0.5,  0.5), glam::Vec3::new( 0.5,  0.5, -0.5), glam::Vec3::new( 0.5,  0.5,  0.5)),

            // 5. ВЕРХНЯЯ ГРАНЬ (Y = 0.5)
            TriangleInstance::new(glam::Vec3::new(-0.5,  0.5, -0.5), glam::Vec3::new(-0.5,  0.5,  0.5), glam::Vec3::new( 0.5,  0.5,  0.5)),
            TriangleInstance::new(glam::Vec3::new(-0.5,  0.5, -0.5), glam::Vec3::new( 0.5,  0.5,  0.5), glam::Vec3::new( 0.5,  0.5, -0.5)),

            // 6. НИЖНЯЯ ГРАНЬ (Y = -0.5)
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5, -0.5), glam::Vec3::new( 0.5, -0.5,  0.5), glam::Vec3::new(-0.5, -0.5,  0.5)),
            TriangleInstance::new(glam::Vec3::new(-0.5, -0.5, -0.5), glam::Vec3::new( 0.5, -0.5, -0.5), glam::Vec3::new( 0.5, -0.5,  0.5)),

            TriangleInstance::new(glam::Vec3::new(-0.45, -0.55, -0.45), glam::Vec3::new( 0.45, -0.55, -0.45), glam::Vec3::new( 0.45, -0.55,  0.45)),
        ];
        
        // Создаем буфер треугольников
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube Vertex Buffer"),
            contents: bytemuck::cast_slice(&instance_cube_tris), // Безопасно превращаем вершины в байты
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Создаем Uniform-буфер для матриц и времени
        // Используем сгенерированный тип cube::CubeUniform!
        let uniform_size = std::num::NonZeroU64::new(
            <instance_cube::CubeUniform as encase::ShaderType>::min_size().get()
        ).unwrap();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cube Uniform Buffer"),
            size: uniform_size.get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Привязываем физический Uniform-буфер к BindGroup
        // Генератор кода wgsl_to_wgpu создал для нас готовую структуру разметки!
        let bindings = instance_cube::bind_groups::BindGroupLayout0 {
            uf: uniform_buffer.as_entire_buffer_binding(),
        };
        let bind_group0 = instance_cube::bind_groups::BindGroup0::from_bindings(device, bindings);

        // Описываем разметку вершин для графического конвейера
        // Сколько байт весит одна вершина? 3 поплавка позиции + 3 поплавка нормали = 24 байта.
        let instance_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TriangleInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance, // ИСПРАВЛЕНО НА INSTANCE!
            attributes: &[
                // location(0): v1
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                // location(1): v2 (через 12 байт от старта)
                wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                // location(2): v3 (через 24 байта от старта)
                wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
                // location(3): normal (через 36 байт от старта)
                wgpu::VertexAttribute { offset: 36, shader_location: 3, format: wgpu::VertexFormat::Float32x3 },
            ],
        };

        // Создаем Layout пайплайна (схему привязок) через генератор в одну строчку
        let pipeline_layout = instance_cube::create_pipeline_layout(device);

        // Собираем сам графический конвейер (Render Pipeline)
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cube Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &instance_cube::create_shader_module(device),
                entry_point: Some(instance_cube::ENTRY_VERTEX_MAIN),
                buffers: &[Some(instance_buffer_layout)], 
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &instance_cube::create_shader_module(device),
                entry_point: Some(instance_cube::ENTRY_FRAGMENT_MAIN),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Запоминаем текущее время для анимации вращения
        let start_time = std::time::Instant::now();

        // Возвращаем полностью готовый экземпляр нашего приложения
        CubeApp {
            instance_buffer,
            uniform_buffer,
            bind_group0,
            render_pipeline,
            start_time,
        }
    }

    fn resize(&mut self, _new_size: PhysicalSize<u32>) {}
    fn handle_input(&mut self, _event: &WindowEvent) -> bool { false }
        fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        // ---- ЧАСТЬ 1: МАТЕМАТИКА И ОБНОВЛЕНИЕ UNIFORM НА CPU ----

        // Считаем время в секундах с момента старта приложения
        let elapsed_time = self.start_time.elapsed().as_secs_f32();

        // Считаем углы вращения куба (пусть по оси X крутится чуть медленнее, чем по Y)
        let angle_x = elapsed_time * 0.5;
        let angle_y = elapsed_time * 0.8;

        // Создаем матрицы вращения с помощью библиотеки glam
        let rotation_x = glam::Mat3::from_rotation_x(angle_x);
        let rotation_y = glam::Mat3::from_rotation_y(angle_y);
        
        // Объединяем их в одну общую матрицу вращения куба
        let final_rotation_mat = rotation_y * rotation_x;

        // Позиция камеры: отодвинем её назад по оси Z на 5 единиц, чтобы видеть куб целиком
        let camera_position = glam::Vec3::new(0.0, 0.0, 8.0);

        // Заполняем нашу сгенерированную структуру CubeUniform данными
        let uniform_data = instance_cube::CubeUniform {
            camera_mat: final_rotation_mat,
            camera_pos: camera_position,
            time: elapsed_time,
        };

        // Сериализуем структуру в байты по стандарту WGSL с помощью библиотеки encase
        let mut byte_buffer = encase::UniformBuffer::new(Vec::new());
        byte_buffer.write(&uniform_data).unwrap();

        // Отправляем получившиеся байты в Uniform-буфер на видеокарту через очередь (queue)
        queue.write_buffer(&self.uniform_buffer, 0, &byte_buffer.into_inner());


        // ---- ЧАСТЬ 2: ЗАПИСЬ КОМАНД ОТРИСОВКИ ДЛЯ GPU ----

        // Открываем рендер-пасс. В отличие от GreenApp, теперь мы не просто чистим экран,
        // а готовим холст для рисования геометрии.
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Cube Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view, // Переданный системный вид экрана
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), // Чистим экран в черный цвет каждый кадр
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Устанавливаем скомпилированный графический конвейер куба
        render_pass.set_pipeline(&self.render_pipeline);

        // Привязываем буфер вершин к слоту 0 (как указано в разметке vertex_buffer_layout)
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));

        // Подключаем нашу группу привязок (Uniform-буфер) через сгенерированную функцию
        instance_cube::set_bind_groups(&mut render_pass, &self.bind_group0);

        render_pass.draw(0..3, 0..13);
    }
}
