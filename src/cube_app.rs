use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;
use wgpu::util::DeviceExt; // Крейт утилиты для удобного создания буферов из данных

use crate::system::AppLogic;
use crate::shaders::cube; // Наш сгенерированный генератором модуль

// 1. Описываем структуру вершины на процессоре
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: glam::Vec3,
    normal: glam::Vec3,
}


const CUBE_VERTICES: &[Vertex] = &[
    // 1. ПЕРЕДНЯЯ ГРАНЬ (Z = -0.5, нормаль Z = -1)
    Vertex { position: glam::Vec3::new(-0.5, -0.5, -0.5), normal: glam::Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: glam::Vec3::new( 0.5, -0.5, -0.5), normal: glam::Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: glam::Vec3::new( 0.5,  0.5, -0.5), normal: glam::Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: glam::Vec3::new(-0.5,  0.5, -0.5), normal: glam::Vec3::new(0.0, 0.0, -1.0) },

    // 2. ЗАДНЯЯ ГРАНЬ (Z = 0.5, нормаль Z = 1) — обход изнутри наружу
    Vertex { position: glam::Vec3::new( 0.5, -0.5,  0.5), normal: glam::Vec3::new(0.0, 0.0,  1.0) },
    Vertex { position: glam::Vec3::new(-0.5, -0.5,  0.5), normal: glam::Vec3::new(0.0, 0.0,  1.0) },
    Vertex { position: glam::Vec3::new(-0.5,  0.5,  0.5), normal: glam::Vec3::new(0.0, 0.0,  1.0) },
    Vertex { position: glam::Vec3::new( 0.5,  0.5,  0.5), normal: glam::Vec3::new(0.0, 0.0,  1.0) },

    // 3. ЛЕВАЯ ГРАНЬ (X = -0.5, нормаль X = -1)
    Vertex { position: glam::Vec3::new(-0.5, -0.5,  0.5), normal: glam::Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: glam::Vec3::new(-0.5, -0.5, -0.5), normal: glam::Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: glam::Vec3::new(-0.5,  0.5, -0.5), normal: glam::Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: glam::Vec3::new(-0.5,  0.5,  0.5), normal: glam::Vec3::new(-1.0, 0.0, 0.0) },

    // 4. ПРАВАЯ ГРАНЬ (X = 0.5, нормаль X = 1)
    Vertex { position: glam::Vec3::new( 0.5, -0.5, -0.5), normal: glam::Vec3::new( 1.0, 0.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5, -0.5,  0.5), normal: glam::Vec3::new( 1.0, 0.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5,  0.5,  0.5), normal: glam::Vec3::new( 1.0, 0.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5,  0.5, -0.5), normal: glam::Vec3::new( 1.0, 0.0, 0.0) },

    // 5. ВЕРХНЯЯ ГРАНЬ (Y = 0.5, normal Y = 1)
    Vertex { position: glam::Vec3::new(-0.5,  0.5, -0.5), normal: glam::Vec3::new(0.0,  1.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5,  0.5, -0.5), normal: glam::Vec3::new(0.0,  1.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5,  0.5,  0.5), normal: glam::Vec3::new(0.0,  1.0, 0.0) },
    Vertex { position: glam::Vec3::new(-0.5,  0.5,  0.5), normal: glam::Vec3::new(0.0,  1.0, 0.0) },

    // 6. НИЖНЯЯ ГРАНЬ (Y = -0.5, normal Y = -1)
    Vertex { position: glam::Vec3::new(-0.5, -0.5,  0.5), normal: glam::Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5, -0.5,  0.5), normal: glam::Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: glam::Vec3::new( 0.5, -0.5, -0.5), normal: glam::Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: glam::Vec3::new(-0.5, -0.5, -0.5), normal: glam::Vec3::new(0.0, -1.0, 0.0) },
];

const CUBE_INDICES: &[u16] = &[
     0,  2,  1,  0,  3,  2, // Передняя (Перевернута наружу)
     4,  6,  5,  4,  7,  6, // Задняя (Перевернута наружу)
     8, 10,  9,  8, 11, 10, // Левая (Перевернута наружу)
    12, 14, 13, 12, 15, 14, // Правая (Перевернута наружу)
    16, 18, 17, 16, 19, 18, // Верхняя (Перевернута наружу)
    20, 22, 21, 20, 23, 22, // Нижняя (Перевернута наружу)
];

pub struct CubeApp {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group0: cube::bind_groups::BindGroup0,
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
        // ШАГ 1. Создаем буфер вершин на видеокарте
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube Vertex Buffer"),
            contents: bytemuck::cast_slice(CUBE_VERTICES), // Безопасно превращаем вершины в байты
            usage: wgpu::BufferUsages::VERTEX,
        });

        // ШАГ 2. Создаем буфер индексов на видеокарте
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube Index Buffer"),
            contents: bytemuck::cast_slice(CUBE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // ШАГ 3. Создаем Uniform-буфер для матриц и времени
        // Используем сгенерированный тип cube::CubeUniform!
        let uniform_size = std::num::NonZeroU64::new(
            <cube::CubeUniform as encase::ShaderType>::min_size().get()
        ).unwrap();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cube Uniform Buffer"),
            size: uniform_size.get(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ШАГ 4. Привязываем физический Uniform-буфер к BindGroup
        // Генератор кода wgsl_to_wgpu создал для нас готовую структуру разметки!
        let bindings = cube::bind_groups::BindGroupLayout0 {
            uf: uniform_buffer.as_entire_buffer_binding(),
        };
        let bind_group0 = cube::bind_groups::BindGroup0::from_bindings(device, bindings);

        // ШАГ 5. Описываем разметку вершин для графического конвейера
        // Сколько байт весит одна вершина? 3 поплавка позиции + 3 поплавка нормали = 24 байта.
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // location(0) в шейдере: position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // location(1) в шейдере: normal (начинается после 12 байт позиции)
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<glam::Vec3>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        };

        // ШАГ 6. Создаем Layout пайплайна (схему привязок) через генератор в одну строчку
        let pipeline_layout = cube::create_pipeline_layout(device);

        // ШАГ 7. Собираем сам графический конвейер (Render Pipeline)
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cube Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &cube::create_shader_module(device),
                entry_point: Some(cube::ENTRY_VERTEX_MAIN),
                buffers: &[Some(vertex_buffer_layout)], 
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &cube::create_shader_module(device),
                entry_point: Some(cube::ENTRY_FRAGMENT_MAIN),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // УБРАНО: Строку constants: &[] полностью удалили, так как этого поля больше нет
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
            vertex_buffer,
            index_buffer,
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

        // 1. Считаем время в секундах с момента старта приложения
        let elapsed_time = self.start_time.elapsed().as_secs_f32();

        // 2. Считаем углы вращения куба (пусть по оси X крутится чуть медленнее, чем по Y)
        let angle_x = elapsed_time * 0.5;
        let angle_y = elapsed_time * 0.8;

        // 3. Создаем матрицы вращения с помощью библиотеки glam
        let rotation_x = glam::Mat3::from_rotation_x(angle_x);
        let rotation_y = glam::Mat3::from_rotation_y(angle_y);
        
        // Объединяем их в одну общую матрицу вращения куба
        let final_rotation_mat = rotation_y * rotation_x;

        // Позиция камеры: отодвинем её назад по оси Z на 5 единиц, чтобы видеть куб целиком
        let camera_position = glam::Vec3::new(0.0, 0.0, 8.0);

        // 4. Заполняем нашу сгенерированную структуру CubeUniform данными
        let uniform_data = cube::CubeUniform {
            camera_mat: final_rotation_mat,
            camera_pos: camera_position,
            time: elapsed_time,
        };

        // 5. Сериализуем структуру в байты по стандарту WGSL с помощью библиотеки encase
        let mut byte_buffer = encase::UniformBuffer::new(Vec::new());
        byte_buffer.write(&uniform_data).unwrap();

        // 6. Отправляем получившиеся байты в Uniform-буфер на видеокарту через очередь (queue)
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

        // 1. Устанавливаем скомпилированный графический конвейер куба
        render_pass.set_pipeline(&self.render_pipeline);

        // 2. Привязываем буфер вершин к слоту 0 (как указано в разметке vertex_buffer_layout)
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // 3. Привязываем буфер индексов. Мы использовали тип u16, поэтому указываем IndexFormat::Uint16
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

        // 4. Подключаем нашу группу привязок (Uniform-буфер) через сгенерированную функцию
        cube::set_bind_groups(&mut render_pass, &self.bind_group0);

        // 5. Финальная команда: рисуем куб по индексам!
        // 36 — это общее количество индексов в массиве CUBE_INDICES (6 граней по 2 треугольника по 3 вершины)
        // 0..1 — означает, что мы рисуем ровно один экземпляр куба (без инстансинга)
        render_pass.draw_indexed(0..36, 0, 0..1);

        // Автоматически закрываем рендер-пасс при выходе из области видимости метода,
        // возвращая управление системному энкодеру.
    }
}
