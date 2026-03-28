use super::atlas::{GlyphAtlas, GlyphInfo};
use crate::core::{CellFlags, Terminal, TerminalMode};

use std::sync::Arc;

use winit::window::Window;

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub surface: wgpu::Surface<'static>,
    pub surface_format: wgpu::TextureFormat,
}
impl GpuContext {
    pub fn new(window: &Arc<Window>) -> Self {
        // デバイスとキューの作成
        let instance =
            wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        use pollster::FutureExt as _;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .block_on()
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .block_on()
            .unwrap();

        // 描画先のウィンドウ情報の取得
        let size = window.inner_size();
        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = if let Some(f) =
            cap.formats.iter().find(|format| !format.is_srgb())
        {
            *f
        }
        else {
            cap.formats[0]
        };

        GpuContext {
            device,
            queue,
            size,
            surface,
            surface_format,
        }
    }
    pub fn configure_surface(&mut self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            view_formats: vec![self.surface_format],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width.max(1),
            height: self.size.height.max(1),
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoNoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuCell {
    cell_pos: [u32; 2],
    fg: [f32; 4],
    bg: [f32; 4],
    uv_rect: [f32; 4],
    offset: [f32; 2],
    size: [f32; 2],
    flags: u32,
    _padding1: u32,
}
impl GpuCell {
    const ATTRIBS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Uint32x2,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x2,
        5 => Float32x2,
        6 => Uint32,
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridUniform {
    cell_size: [f32; 2],
    grid_size: [u32; 2],
    atlas_size: [f32; 2],
    cursor_pos: [u32; 2],
    cursor_style: u32,
    _padding1: u32,
    viewport_size: [f32; 2],
    selection_range: [u32; 2],
}

pub struct Renderer {
    cell_render_pipeline: wgpu::RenderPipeline,
    glyph_render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    cell_buffer: wgpu::Buffer,
    uniform: GridUniform,
    uniform_buffer: wgpu::Buffer,
}
impl Renderer {
    pub fn new(
        gpu: &GpuContext,
        atlas: &GlyphAtlas,
        terminal: &Terminal,
    ) -> Self {
        use wgpu::util::DeviceExt as _;
        let device = &gpu.device;
        let grid = terminal.active_grid();

        let shader =
            device.create_shader_module(wgpu::include_wgsl!("./shader.wgsl"));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let cell_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("CellBuffer"),
            size: (grid.grid_rows() * grid.grid_cols() * size_of::<GpuCell>())
                as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let [cell_width, cell_height] = atlas.cell_size();
        let uniform = GridUniform {
            cell_size: [cell_width as f32, cell_height as f32],
            grid_size: [grid.grid_cols() as u32, grid.grid_rows() as u32],
            atlas_size: [
                GlyphAtlas::ATLAS_SIZE as f32,
                GlyphAtlas::ATLAS_SIZE as f32,
            ],
            cursor_pos: {
                let point = grid.cursor().point;
                [point.col as u32, point.row as u32]
            },
            cursor_style: terminal.cursor_style() as u32,
            _padding1: Default::default(),
            viewport_size: {
                let window_size = gpu.size;
                [window_size.width as f32, window_size.height as f32]
            },
            selection_range: [0, 0],
        };
        let uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("UniformBuffer"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("BindGroupLayout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(
                            wgpu::SamplerBindingType::Filtering,
                        ),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BindGroup"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(
                        uniform_buffer.as_entire_buffer_binding(),
                    ),
                },
            ],
        });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("PipelineLayout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });
        let cell_render_pipeline = gpu.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("CellRenderPipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_cell"),
                    buffers: &[GpuCell::desc()],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_cell"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Front),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            },
        );
        let glyph_render_pipeline = gpu.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("GlyphRenderPipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_glyph"),
                    buffers: &[GpuCell::desc()],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_glyph"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Front),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            },
        );

        Self {
            cell_render_pipeline,
            glyph_render_pipeline,
            bind_group_layout,
            bind_group,
            sampler,
            cell_buffer,
            uniform,
            uniform_buffer,
        }
    }
    pub fn resize(
        &mut self,
        gpu: &GpuContext,
        atlas: &GlyphAtlas,
        rows: usize,
        cols: usize,
    ) {
        let need_buffer_size = (rows * cols * size_of::<GpuCell>()) as u64;
        if self.cell_buffer.size() < need_buffer_size {
            self.cell_buffer =
                gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("CellBuffer"),
                    size: need_buffer_size,
                    usage: wgpu::BufferUsages::VERTEX
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            self.bind_group =
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("BindGroup"),
                    layout: &self.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                atlas.view(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(
                                &self.sampler,
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(
                                self.uniform_buffer.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
        }

        self.uniform.grid_size = [cols as u32, rows as u32];
        let window_size = gpu.size;
        self.uniform.viewport_size =
            [window_size.width as f32, window_size.height as f32];
        gpu.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.uniform),
        );
    }
    pub fn render(
        &mut self,
        window: &winit::window::Window,
        gpu: &GpuContext,
        atlas: &mut GlyphAtlas,
        terminal: &Terminal,
        selection_range: [u32; 2],
    ) {
        // スワップチェーンのバックバッファの取得
        let surface_texture = gpu
            .surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view =
            surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor {
                    // Without add_srgb_suffix() the image we will be working with
                    // might not be "gamma correct".
                    //format: Some(self.surface_format),
                    ..Default::default()
                });

        let mut encoder =
            gpu.device.create_command_encoder(&Default::default());

        // レンダーパスの設定
        let mut render_pass =
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        // グリッドからGpuCellへの変換
        let mut cell_buffer = vec![];
        let mut empty_cell_buffer = vec![];
        let grid = terminal.active_grid();
        let rows = grid.grid_rows();
        let cols = grid.grid_cols();
        for y in 0..rows {
            let row = grid.viewport_row(y);
            for (x, cell) in row.iter().take(cols).enumerate() {
                let GlyphInfo {
                    uv_rect,
                    offset,
                    size,
                } = atlas.get_or_insert(gpu, cell.c);
                let fg = cell.fg.color_to_rgba();
                let bg = cell.bg.color_to_rgba();
                let flags = cell.flags.bits() as u32;
                if size[0] <= 0.0
                    || size[1] <= 0.0
                    || cell.flags.contains(CellFlags::WIDE_CHAR_SPACER)
                {
                    empty_cell_buffer.push(GpuCell {
                        cell_pos: [x as u32, y as u32],
                        fg,
                        bg,
                        uv_rect,
                        offset,
                        size,
                        flags,
                        _padding1: Default::default(),
                    });
                    continue;
                }
                cell_buffer.push(GpuCell {
                    cell_pos: [x as u32, y as u32],
                    fg,
                    bg,
                    uv_rect,
                    offset,
                    size,
                    flags,
                    _padding1: Default::default(),
                });
            }
        }
        gpu.queue.write_buffer(
            &self.cell_buffer,
            0,
            bytemuck::cast_slice(&cell_buffer),
        );
        gpu.queue.write_buffer(
            &self.cell_buffer,
            (size_of::<GpuCell>() * cell_buffer.len()) as u64,
            bytemuck::cast_slice(&empty_cell_buffer),
        );

        // Uniformの更新
        let point = grid.cursor().point;
        self.uniform.cursor_pos = [
            point.col as u32,
            (point.row + grid.viewport_offset()) as u32,
        ];
        self.uniform.selection_range = selection_range;
        gpu.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&GridUniform {
                cursor_style: {
                    if terminal.mode().contains(TerminalMode::CURSOR_VISIBLE) {
                        self.uniform.cursor_style
                    }
                    else {
                        crate::core::CursorStyle::Hidden as u32
                    }
                },
                ..self.uniform
            }),
        );

        // グリッドの描画
        render_pass.set_pipeline(&self.cell_render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.cell_buffer.slice(..));
        render_pass.draw(
            0..6,
            0..(cell_buffer.len() + empty_cell_buffer.len()) as u32,
        );
        // 文字の描画
        render_pass.set_pipeline(&self.glyph_render_pipeline);
        render_pass.draw(0..6, 0..cell_buffer.len() as u32);

        drop(render_pass);

        gpu.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        surface_texture.present();
    }
}
