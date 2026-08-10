use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::window::Window;
use crate::system::AppLogic; // Импортируем ваш переименованный трейт
use winit::event::{WindowEvent, KeyEvent, ElementState};
use winit::keyboard::{PhysicalKey, KeyCode};

pub struct GreenApp;

impl AppLogic for GreenApp {
    fn new(
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _surface_format: wgpu::TextureFormat,
        _window: Arc<Window>,
    ) -> Self {
        // На этапе старта нам пока ничего не нужно создавать на GPU
        GreenApp
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        println!("Окно изменило размер: {}x{}", new_size.width, new_size.height);
    }

    fn handle_input(&mut self, event: &WindowEvent) -> bool {
        match event {
            // Перехватываем ввод с клавиатуры
            WindowEvent::KeyboardInput { 
                event: KeyEvent {
                    physical_key: PhysicalKey::Code(key_code),
                    state,
                    ..
                },
                ..
            } => {
                // state показывает: Pressed (нажата) или Released (отпущена)
                let state_str = match state {
                    ElementState::Pressed => "НАЖАТА",
                    ElementState::Released => "ОТПУЩЕНА",
                };

                println!("Клавиша {:?} теперь {}", key_code, state_str);

                // Если нажат Escape, возвращаем false. 
                // Системная обвязка winit увидит false, пойдет дальше по коду 
                // и закроет приложение (если вы добавите обработку Escape в system.rs).
                // Или вы можете обрабатывать закрытие прямо здесь.
                if *key_code == KeyCode::Escape {
                    println!("Нажат Escape, передаем управление системе для выхода...");
                    return false; 
                }

                // Для всех остальных клавиш возвращаем true — мы их полностью перехватили
                true
            }
            
            // Любые другие события окна (движение мыши, фокус и т.д.) пропускаем дальше
            _ => false,
        }
    }

    fn render(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        // Открываем рендер-пасс прямо здесь! 
        // Система передала нам энкодер, и мы пишем в него инструкцию очистки экрана.
        let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Green Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view, // Рисуем в переданный вид экрана
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN), // Красим в зеленый
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Закрываем пас, чтобы вернуть управление энкодеру системы
        drop(render_pass);
    }
}
