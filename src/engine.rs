use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub sun_dir: [f32; 4],
    pub params: [f32; 4],
    pub screen: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    pub model: [[f32; 4]; 4],
    pub color: [f32; 4],
    pub params: [f32; 4], // uv_x, uv_y, tex_layer (<0 = flat), emissive
}

pub const HUD_W: u32 = 640;
pub const HUD_H: u32 = 360;
const MAX_INSTANCES: usize = 20000;

pub struct Engine {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    hud_pipeline: wgpu::RenderPipeline,
    scene_bind: wgpu::BindGroup,
    hud_bind: wgpu::BindGroup,
    menu_pipeline: wgpu::RenderPipeline,
    menu_bind: wgpu::BindGroup,
    _menu_tex: wgpu::Texture,
    globals_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    instance_buf: wgpu::Buffer,
    hud_tex: wgpu::Texture,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pub instances: Vec<Instance>,
}

fn make_atlas() -> Vec<u8> {
    const S: usize = 256;
    const LAYERS: usize = 5;
    let mut data = vec![0u8; S * S * 4 * LAYERS];
    let mut seed = 0x1234_5678u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        ((seed >> 16) & 0xff) as i32 - 128
    };
    // 16x16 chunky pixels per texture — deliberately low resolution.
    const CELL: usize = 16;
    for layer in 0..LAYERS {
        for cy in 0..(S / CELL) {
            for cx in 0..(S / CELL) {
                let n = rnd() / 4;
                let mortar = cx % 4 == 0 || cy % 4 == 0;
                let (r, g, b) = match layer {
                    0 => { // cobblestone road
                        if mortar { (76, 79, 84) } else { (102 + n / 2, 105 + n / 2, 110 + n / 2) }
                    }
                    1 => { // cut stone sidewalk
                        if mortar { (142, 146, 152) } else { (172 + n / 3, 175 + n / 3, 180 + n / 3) }
                    }
                    2 => { // grass + dirt flecks
                        if n > 26 { (122, 96, 60) } else { (74 + n / 3, 138 + n / 2, 64 + n / 3) }
                    }
                    3 => { // stone brick
                        let offset = if cy % 2 == 0 { 0 } else { 2 };
                        let seam = (cx + offset) % 4 == 0 || cy % 2 == 0;
                        if seam { (95, 98, 104) } else { (132 + n / 2, 135 + n / 2, 140 + n / 2) }
                    }
                    _ => { // water
                        let wave = if (cx + cy) % 5 == 0 { 28 } else { 0 };
                        (26 + n / 3, 108 + wave + n / 3, 158 + wave + n / 3)
                    }
                };
                for py in 0..CELL {
                    for px in 0..CELL {
                        let i = (layer * S * S + (cy * CELL + py) * S + cx * CELL + px) * 4;
                        data[i] = r.clamp(0, 255) as u8;
                        data[i + 1] = g.clamp(0, 255) as u8;
                        data[i + 2] = b.clamp(0, 255) as u8;
                        data[i + 3] = 255;
                    }
                }
            }
        }
    }
    data
}

fn cube_verts() -> (Vec<f32>, Vec<u16>) {
    // pos(3), normal(3), uv(2)
    #[rustfmt::skip]
    let v: &[f32] = &[
        // +Z
        -0.5,-0.5, 0.5,  0.0,0.0,1.0, 0.0,0.0,   0.5,-0.5, 0.5,  0.0,0.0,1.0, 1.0,0.0,   0.5, 0.5, 0.5,  0.0,0.0,1.0, 1.0,1.0,  -0.5, 0.5, 0.5,  0.0,0.0,1.0, 0.0,1.0,
        // -Z
         0.5,-0.5,-0.5,  0.0,0.0,-1.0,0.0,0.0,  -0.5,-0.5,-0.5,  0.0,0.0,-1.0,1.0,0.0,  -0.5, 0.5,-0.5,  0.0,0.0,-1.0,1.0,1.0,   0.5, 0.5,-0.5,  0.0,0.0,-1.0,0.0,1.0,
        // +X
         0.5,-0.5, 0.5,  1.0,0.0,0.0, 0.0,0.0,   0.5,-0.5,-0.5,  1.0,0.0,0.0, 1.0,0.0,   0.5, 0.5,-0.5,  1.0,0.0,0.0, 1.0,1.0,   0.5, 0.5, 0.5,  1.0,0.0,0.0, 0.0,1.0,
        // -X
        -0.5,-0.5,-0.5, -1.0,0.0,0.0, 0.0,0.0,  -0.5,-0.5, 0.5, -1.0,0.0,0.0, 1.0,0.0,  -0.5, 0.5, 0.5, -1.0,0.0,0.0, 1.0,1.0,  -0.5, 0.5,-0.5, -1.0,0.0,0.0, 0.0,1.0,
        // +Y
        -0.5, 0.5, 0.5,  0.0,1.0,0.0, 0.0,0.0,   0.5, 0.5, 0.5,  0.0,1.0,0.0, 1.0,0.0,   0.5, 0.5,-0.5,  0.0,1.0,0.0, 1.0,1.0,  -0.5, 0.5,-0.5,  0.0,1.0,0.0, 0.0,1.0,
        // -Y
        -0.5,-0.5,-0.5,  0.0,-1.0,0.0,0.0,0.0,   0.5,-0.5,-0.5,  0.0,-1.0,0.0,1.0,0.0,   0.5,-0.5, 0.5,  0.0,-1.0,0.0,1.0,1.0,  -0.5,-0.5, 0.5,  0.0,-1.0,0.0,0.0,1.0,
    ];
    let mut idx = Vec::new();
    for f in 0..6u16 {
        let b = f * 4;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    (v.to_vec(), idx)
}

impl Engine {
    pub fn new(window: &winit::window::Window) -> Self {
        let instance = wgpu::Instance::default();
        let surface: wgpu::Surface<'static> = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::from_window(window).unwrap())
                .unwrap()
        };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no GPU adapter found");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("gta7 device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ))
        .unwrap();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // atlas
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: 256, height: 256, depth_or_array_layers: 5 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &atlas, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &make_atlas(),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(256 * 4), rows_per_image: Some(256) },
            wgpu::Extent3d { width: 256, height: 256, depth_or_array_layers: 5 },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let hud_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hud"),
            size: wgpu::Extent3d { width: HUD_W, height: HUD_H, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let hud_view = hud_tex.create_view(&Default::default());

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let scene_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind"),
            layout: &scene_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let hud_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let hud_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud bind"),
            layout: &hud_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hud_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let hud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hud shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("hud.wgsl").into()),
        });

        let (verts, idx) = cube_verts();
        let index_count = idx.len() as u32;
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube verts"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube idx"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene layout"),
            bind_group_layouts: &[&scene_bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene pipeline"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &scene_shader,
                entry_point: "vs_scene",
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 2 },
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 3 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 4 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 5 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 7 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 80, shader_location: 8 },
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &scene_shader,
                entry_point: "fs_scene",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky pipeline"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState { module: &scene_shader, entry_point: "vs_full", compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &scene_shader,
                entry_point: "fs_sky",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
        });

        let hud_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud layout"),
            bind_group_layouts: &[&hud_bgl],
            push_constant_ranges: &[],
        });
        let hud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud pipeline"),
            layout: Some(&hud_layout),
            vertex: wgpu::VertexState { module: &hud_shader, entry_point: "vs_full", compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &hud_shader,
                entry_point: "fs_hud",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
        });

        // Home screen image (JPEG stored in the project folder).
        let menu_img = image::load_from_memory(include_bytes!("../home_screen.jpeg"))
            .expect("home_screen.jpeg could not be decoded")
            .to_rgba8();
        let (mw, mh) = menu_img.dimensions();
        let menu_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("menu image"),
            size: wgpu::Extent3d { width: mw, height: mh, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let menu_view = menu_tex.create_view(&Default::default());
        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &menu_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &menu_img,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(mw * 4), rows_per_image: Some(mh) },
            wgpu::Extent3d { width: mw, height: mh, depth_or_array_layers: 1 },
        );
        let menu_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("menu bind"),
            layout: &hud_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&menu_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let menu_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("menu shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("menu.wgsl").into()),
        });
        let menu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menu pipeline"),
            layout: Some(&hud_layout),
            vertex: wgpu::VertexState { module: &menu_shader, entry_point: "vs_full", compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &menu_shader,
                entry_point: "fs_menu",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
        });

        let (width, height) = (config.width, config.height);
        let (depth, depth_view) = Self::make_depth(&device, width, height);

        Self {
            device, queue, surface, config, pipeline, sky_pipeline, hud_pipeline,
            scene_bind, hud_bind, menu_pipeline, menu_bind, _menu_tex: menu_tex,
            globals_buf, vertex_buf, index_buf, index_count,
            instance_buf, hud_tex, depth, depth_view, instances: Vec::with_capacity(4096),
        }
    }

    fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        (tex, view)
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        let (d, v) = Self::make_depth(&self.device, w, h);
        self.depth = d;
        self.depth_view = v;
    }

    pub fn size(&self) -> (u32, u32) { (self.config.width, self.config.height) }

    pub fn render(&mut self, globals: Globals, hud_pixels: &[u8], show_menu: bool) {
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        let n = self.instances.len().min(MAX_INSTANCES);
        if n > 0 {
            self.queue.write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&self.instances[..n]));
        }
        self.queue.write_texture(
            wgpu::ImageCopyTexture { texture: &self.hud_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            hud_pixels,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(HUD_W * 4), rows_per_image: Some(HUD_H) },
            wgpu::Extent3d { width: HUD_W, height: HUD_H, depth_or_array_layers: 1 },
        );

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_bind_group(0, &self.scene_bind, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);

            if show_menu {
                pass.set_pipeline(&self.menu_pipeline);
                pass.set_bind_group(0, &self.menu_bind, &[]);
                pass.draw(0..3, 0..1);
            }

            if n > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.scene_bind, &[]);
                pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..self.index_count, 0, 0..(n as u32));
            }

            pass.set_pipeline(&self.hud_pipeline);
            pass.set_bind_group(0, &self.hud_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
