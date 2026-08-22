// use crate::{shaders::compute, system::AppLogic};

// struct CompExampleApp {
//     render_pipeline: wgpu::RenderPipeline,
// }

// impl AppLogic for CompExampleApp {
//     fn new(
//         device:          &wgpu::Device,
//         queue:           &wgpu::Queue,
//         surface:         &wgpu::Surface<'static>,
//         surface_format:  wgpu::TextureFormat,
//         window:          std::sync::Arc<winit::window::Window>,
//     ) -> Self
//     {
//         let pipeline_layout = compute::create_pipeline_layout(device);
//         let current_texture = surface.get_current_texture()
//         let bindings = compute::bind_groups::BindGroupLayout0();
//     }
// }