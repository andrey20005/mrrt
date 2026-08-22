use std::time::Instant;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::app_prelude::{AppLogic, AppState};
use crate::shaders::compute; // Код, сгенерированный из compute.wgsl с помощью wgsl_to_wgpu
use crate::texture_mapping::{RenderTexture, TextureMapping};

pub struct TestApp {
    // Наши вспомогательные инструменты
    render_texture: RenderTexture,
    texture_mapping: TextureMapping,

    // Ресурсы для Compute-пасса
    compute_pipeline: wgpu::ComputePipeline,
    compute_bind_group: compute::bind_groups::BindGroup0,

    // Буфер для передачи времени на GPU
    time_buffer: wgpu::Buffer,
    start_time: Instant,
}

impl AppLogic for TestApp {
    fn new(state: &AppState) -> Self {
        // 1. Инициализируем текстуру-холст (например, с масштабом 50% для теста скорости)
        let render_texture = RenderTexture::new(state, 0.5);

        // 2. Инициализируем пайплайн вывода на экран
        let texture_mapping = TextureMapping::new(state);

        // 3. Создаем Uniform-буфер для хранения времени (f32 занимает 4 байта, но wgpu требует выравнивание)
        let time_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Time Uniform Buffer"),
            size: 16, // Выравнивание до 16 байт (размер vec4f в WGSL)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 4. Создаем вычислительный пайплайн (Compute Pipeline) через wgsl_to_wgpu
        let compute_layout = compute::create_pipeline_layout(&state.device);
        let compute_module = compute::create_shader_module(&state.device);

        let compute_pipeline = state.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Gradients Compute Pipeline"),
            layout: Some(&compute_layout),
            module: &compute_module,
            entry_point: Some(compute::ENTRY_MAIN),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // 5. Создаем бинд-группу для Compute-шейдера с помощью сгенерированного кода.
        // Передаем туда наш буфер времени и текстуру на запись (через её view)
        let compute_bindings = compute::bind_groups::BindGroupLayout0 {
            time: time_buffer.as_entire_buffer_binding(),
            output_texture: render_texture.texture_view(), // Используем геттер
        };
        let compute_bind_group = compute::bind_groups::BindGroup0::from_bindings(&state.device, compute_bindings);

        Self {
            render_texture,
            texture_mapping,
            compute_pipeline,
            compute_bind_group,
            time_buffer,
            start_time: Instant::now(),
        }
    }

    fn resize(&mut self, state: &AppState, new_size: PhysicalSize<u32>) {
        // Пересчитываем размеры нашей текстуры рендера
        self.render_texture.resize(state, new_size.width, new_size.height);

        // ВАЖНО: Так как текстура внутри render_texture пересоздалась, её старый TextureView
        // стал невалидным. Нам нужно обновить бинд-группу вычислительного шейдера!
        let compute_bindings = compute::bind_groups::BindGroupLayout0 {
            time: self.time_buffer.as_entire_buffer_binding(),
            output_texture: self.render_texture.texture_view(),
        };
        self.compute_bind_group = compute::bind_groups::BindGroup0::from_bindings(&state.device, compute_bindings);
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
        false
    }

    fn render(
        &mut self,
        state: &AppState,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        // Обновляем время в Uniform-буфере на GPU
        let elapsed = self.start_time.elapsed().as_secs_f32();
        state.queue.write_buffer(&self.time_buffer, 0, bytemuck::cast_slice(&[elapsed]));

        // --- ЗАПУСК ВЫЧИСЛИТЕЛЬНОГО ШЕЙДЕРА (COMPUTE PASS) ---
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Gradients Compute Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            
            // Применяем сгенерированную бинд-группу для вычислений
            compute::set_bind_groups(&mut cpass, &self.compute_bind_group);

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
    }
}
