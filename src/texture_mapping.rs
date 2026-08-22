use crate::{app_prelude::AppState, shaders::texture_mapping};

pub struct TextureMapping {
    render_pipeline: wgpu::RenderPipeline,
}

impl TextureMapping {
    pub fn new(state: &AppState) -> Self {
        let pipeline_layout = texture_mapping::create_pipeline_layout(&state.device);
        let shader_module = texture_mapping::create_shader_module(&state.device);
        let render_pipeline = state.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Raytracing Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some(texture_mapping::ENTRY_VERTEX_MAIN),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some(texture_mapping::ENTRY_FRAGMENT_MAIN),
                targets: &[Some(wgpu::ColorTargetState {
                    format: state.surface_format.add_srgb_suffix(),
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

        return Self { render_pipeline };
    }

    pub fn render(
        &mut         self, 
        bind_group0: &texture_mapping::bind_groups::BindGroup0,
        _state:      &AppState, 
        view:        &wgpu::TextureView, 
        encoder:     &mut wgpu::CommandEncoder,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Raytracing Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        render_pass.set_pipeline(&self.render_pipeline);
        texture_mapping::set_bind_groups(&mut render_pass, bind_group0);
        render_pass.draw(0..6, 0..1);

        drop(render_pass); 
    }
}


pub struct RenderTexture {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView, 
    render_bind_group: texture_mapping::bind_groups::BindGroup0,
    
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    use_pixel_art_style: bool,
    
    window_width: u32,
    window_height: u32,
    scale_percentage: f32, 
}

impl RenderTexture {
    pub fn new(state: &AppState, scale_percentage: f32) -> Self {
        let linear_sampler = state.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Render Buffer Linear Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest, 
            ..Default::default()
        });

        let nearest_sampler = state.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Render Buffer Nearest Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest, 
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest, 
            ..Default::default()
        });

        let use_pixel_art_style = false;

        let (texture, texture_view, render_bind_group) = 
            Self::create_resources(state, scale_percentage, &linear_sampler);

        Self {
            texture,
            texture_view,
            render_bind_group,
            linear_sampler,
            nearest_sampler,
            use_pixel_art_style,
            window_width: state.size.width,
            window_height: state.size.height,
            scale_percentage,
        }
    }

    fn create_resources(
        state: &AppState,
        scale_percentage: f32,
        sampler: &wgpu::Sampler,
    ) -> (wgpu::Texture, wgpu::TextureView, texture_mapping::bind_groups::BindGroup0) {
        let virtual_width = ((state.size.width as f32 * scale_percentage) as u32).max(1);
        let virtual_height = ((state.size.height as f32 * scale_percentage) as u32).max(1);

        let texture = state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render Buffer Texture"),
            size: wgpu::Extent3d {
                width: virtual_width,
                height: virtual_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, 
            // TEXTURE_BINDING нужна для фрагментного шейдера, STORAGE_BINDING — для вычислительного
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        // Создаем одну View, которая будет использоваться и на чтение, и на запись
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Привязываем имена строго в соответствии с вашим WGSL (my_texture и my_sampler)
        let bindings = texture_mapping::bind_groups::BindGroupLayout0 {
            my_texture: &texture_view,
            my_sampler: sampler,
        };
        
        let render_bind_group = texture_mapping::bind_groups::BindGroup0::from_bindings(&state.device, bindings);

        (texture, texture_view, render_bind_group)
    }

    pub fn set_pixel_art_mode(&mut self, state: &AppState, enable_pixel_art: bool) {
        if self.use_pixel_art_style == enable_pixel_art {
            return;
        }

        self.use_pixel_art_style = enable_pixel_art;

        let active_sampler = if self.use_pixel_art_style {
            &self.nearest_sampler
        } else {
            &self.linear_sampler
        };

        // Текстура остается прежней, обновляем только сэмплер в бинд-группе
        let bindings = texture_mapping::bind_groups::BindGroupLayout0 {
            my_texture: &self.texture_view,
            my_sampler: active_sampler,
        };
        
        self.render_bind_group = texture_mapping::bind_groups::BindGroup0::from_bindings(&state.device, bindings);
    }

    pub fn resize(&mut self, state: &AppState, new_width: u32, new_height: u32) {
        self.window_width = new_width;
        self.window_height = new_height;
        
        let active_sampler = if self.use_pixel_art_style {
            &self.nearest_sampler
        } else {
            &self.linear_sampler
        };

        let (texture, texture_view, render_bind_group) = 
            Self::create_resources(state, self.scale_percentage, active_sampler);
            
        self.texture = texture;
        self.texture_view = texture_view;
        self.render_bind_group = render_bind_group;
    }

    pub fn set_scale(&mut self, state: &AppState, scale_percentage: f32) {
        self.scale_percentage = scale_percentage.clamp(0.01, 1.0);
        self.resize(state, self.window_width, self.window_height);
    }

    // Ваш переименованный геттер
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    pub fn render_bind_group(&self) -> &texture_mapping::bind_groups::BindGroup0 {
        &self.render_bind_group
    }

    pub fn virtual_size(&self) -> (u32, u32) {
        let w = ((self.window_width as f32 * self.scale_percentage) as u32).max(1);
        let h = ((self.window_height as f32 * self.scale_percentage) as u32).max(1);
        (w, h)
    }
}
