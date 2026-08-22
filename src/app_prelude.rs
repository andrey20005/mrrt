use std::sync::Arc;

use winit::{
    application::ApplicationHandler, dpi::PhysicalSize, event::WindowEvent, event_loop::{ActiveEventLoop}, window::{Window, WindowId},
};

pub trait AppLogic: Sized {
    /// Инициализация: создание пайплайнов и буферов на GPU
    fn new(stage: &AppState) -> Self;

    /// Изменение размера окна: здесь пересчитываем aspect ratio или размер внутренних текстур
    fn resize(&mut self, state: &AppState, new_size: PhysicalSize<u32>);

    /// Обработка ввода и системных событий окна (движение мыши, клавиатура и т.д.)
    /// Возвращает bool: true, если событие перехвачено вашей логикой и winit не должен обрабатывать его дальше
    fn handle_input(&mut self, state: &AppState, _event: &WindowEvent) -> bool { false }
    fn handle_mouse_motion(&mut self, state: &AppState, _dx: f64, _dy: f64) {}

    /// Отрисовка кадра на GPU
    fn render(
        &mut     self, 
        state:   &AppState, 
        view:    &wgpu::TextureView, 
        encoder: &mut wgpu::CommandEncoder,
    );
}

pub struct AppState {
    pub instance:        wgpu::Instance,
    pub device:          wgpu::Device,
    pub queue:           wgpu::Queue,
    pub window:          Arc<Window>,
    pub surface:         wgpu::Surface<'static>,
    pub surface_format:  wgpu::TextureFormat,
    pub size:            winit::dpi::PhysicalSize<u32>,
}

impl AppState {
    fn configure_surface(&self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            // Request compatibility with the sRGB-format texture view we‘re going to create later.
            view_formats: vec![self.surface_format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }
}

pub struct App<T: AppLogic> {
    state: Option<AppState>,
    logic: Option<T>,
}

impl<T: AppLogic> App<T> {
    pub fn new() -> Self {
        Self { state: None, logic: None }
    }
}

impl<T: AppLogic> ApplicationHandler for App<T> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window object
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );

        let display = event_loop.owned_display_handle();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let adapter = pollster::block_on(instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
        ).unwrap();
        let (device, queue) = pollster::block_on(adapter
            .request_device(&wgpu::DeviceDescriptor::default())
        ).unwrap();

        let size = window.inner_size();
        
        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        let stage = AppState{
            instance,
            device,
            queue,
            window: window.clone(),
            surface,
            surface_format,
            size
        };

        let logic = T::new(&stage);
        
        self.logic = Some(logic);
        self.state = Some(stage);

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = self.state.as_mut().unwrap();
        let logic = self.logic.as_mut().unwrap();

        // Сначала отдаем ввод в прикладную логику. 
        // Если метод вернул true — прерываем выполнение и игнорируем системные события.
        if logic.handle_input(state, &event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                println!("Закрытие окна");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let surface_texture = match state.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture) => texture,
                    wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
                    wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                        drop(texture);
                        state.configure_surface();
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Outdated => {
                        state.configure_surface();
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Validation => {
                        unreachable!("No error scope registered, so validation errors will panic")
                    }
                    wgpu::CurrentSurfaceTexture::Lost => {
                        state.surface = state.instance.create_surface(state.window.clone()).unwrap();
                        state.configure_surface();
                        return;
                    }
                };
                let texture_view = surface_texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor {
                        // Without add_srgb_suffix() the image we will be working with
                        // might not be "gamma correct".
                        format: Some(state.surface_format.add_srgb_suffix()),
                        ..Default::default()
                    });
                let mut encoder = state.device.create_command_encoder(&Default::default());
                
                // Отдаем управление прикладной логике. 
                // Она сама запишет в encoder нужные команды (RenderPass/ComputePass).
                logic.render(state, &texture_view, &mut encoder);

                // Submit the command in the queue to execute
                state.queue.submit([encoder.finish()]);
                state.window.pre_present_notify();
                state.queue.present(surface_texture);

                // Emits a new redraw requested event.
                state.window.request_redraw();
            }
            WindowEvent::Resized(size) => {
                // Reconfigures the size of the surface. We do not re-render
                // here as this event is always followed up by redraw request.
                state.size = size;
                state.configure_surface();
                logic.resize(state, size);
            }
            _ => (),
        }
    }

    fn device_event(
        &mut         self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device_id:  winit::event::DeviceId,
        event:       winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if let Some(ref mut logic) = self.logic {
                let state = self.state.as_mut().unwrap();
                // Перенаправляем дельту движения мыши в нашу камеру, если она там есть!
                // (Для этого мы добавим метод handle_mouse_motion в ваш трейт AppLogic)
                logic.handle_mouse_motion(state, dx, dy);
            }
        }
    }
}