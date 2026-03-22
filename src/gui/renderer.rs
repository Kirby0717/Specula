use super::atlas::{GlyphAtlas, GlyphIndex};
use crate::core::{CellFlags, Terminal};

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
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuCell {
    glyph_index: u32,
    // vec4のアライメント用パディング
    _pad: [u32; 3],
    fg: [f32; 4],
    bg: [f32; 4],
}

#[repr(u32)]
#[allow(unused)]
pub enum CursorShape {
    Hidden = 0,
    Block = 1,
    Underline = 2,
    Bar = 3,
}
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridUniform {
    cell_size: [f32; 2],
    grid_size: [u32; 2],
    atlas_size: [f32; 2],
    slots_per_row: u32,
    // vec2のアライメント用パディング
    _padding1: u32,
    cursor_pos: [u32; 2],
    cursor_style: u32,
    _padding2: u32,
}

pub struct Renderer {
    render_pipeline: wgpu::RenderPipeline,
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

        let shader =
            device.create_shader_module(wgpu::include_wgsl!("./shader.wgsl"));
        let sampler =
            device.create_sampler(&wgpu::SamplerDescriptor::default());
        let cell_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("CellBuffer"),
            size: (terminal.grid_rows()
                * terminal.grid_cols()
                * size_of::<GpuCell>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = GridUniform {
            cell_size: [atlas.cell_width as f32, atlas.cell_height as f32],
            grid_size: [
                terminal.grid_cols() as u32,
                terminal.grid_rows() as u32,
            ],
            atlas_size: [
                GlyphAtlas::ATLAS_SIZE as f32,
                GlyphAtlas::ATLAS_SIZE as f32,
            ],
            slots_per_row: atlas.slots_per_row,
            _padding1: Default::default(),
            cursor_pos: {
                let point = terminal.cursor().point;
                [point.col as u32, point.row as u32]
            },
            cursor_style: CursorShape::Block as u32,
            _padding2: Default::default(),
        };
        let uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("CellBuffer"),
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
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage {
                                read_only: true,
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
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
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(
                        cell_buffer.as_entire_buffer_binding(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
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
        let render_pipeline = gpu.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("RenderPipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
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

        Self {
            render_pipeline,
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
                    usage: wgpu::BufferUsages::STORAGE
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
                                &atlas.view,
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
                                self.cell_buffer.as_entire_buffer_binding(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                self.uniform_buffer.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                });
        }

        self.uniform.grid_size = [cols as u32, rows as u32];
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
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
        let grid = terminal.active_grid();
        for y in 0..grid.grid_rows() {
            let row = grid.viewport_row(y);
            let mut wide_right = None;
            for x in 0..grid.grid_cols() {
                // 範囲内
                if x < row.len() {
                    let cell = &row[x];
                    let fg = cell.fg.color_to_rgba();
                    let bg = cell.bg.color_to_rgba();

                    // ワイドの左側
                    if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER)
                        && let Some(index) = wide_right
                    {
                        cell_buffer.push(GpuCell {
                            glyph_index: index,
                            _pad: Default::default(),
                            fg,
                            bg,
                        });
                        continue;
                    }
                    wide_right = None;
                    let glyph_index = atlas.get_or_insert(
                        gpu,
                        cell.c,
                        cell.flags.contains(CellFlags::WIDE_CHAR),
                    );
                    let index = match glyph_index {
                        GlyphIndex::Wide(l, r) => {
                            wide_right = Some(r);
                            l
                        }
                        GlyphIndex::Narrow(i) => i,
                    };
                    cell_buffer.push(GpuCell {
                        glyph_index: index,
                        _pad: Default::default(),
                        fg,
                        bg,
                    });
                }
                // 範囲外
                else {
                    let cell = crate::core::Cell::default();
                    let fg = cell.fg.color_to_rgba();
                    let bg = cell.bg.color_to_rgba();
                    let GlyphIndex::Narrow(glyph_index) =
                        atlas.get_or_insert(gpu, ' ', false)
                    else {
                        unreachable!()
                    };
                    cell_buffer.push(GpuCell {
                        glyph_index,
                        _pad: Default::default(),
                        fg,
                        bg,
                    });
                }
            }
        }
        gpu.queue.write_buffer(
            &self.cell_buffer,
            0,
            bytemuck::cast_slice(&cell_buffer),
        );

        // カーソルの更新
        let point = terminal.cursor().point;
        self.uniform.cursor_pos = [point.col as u32, point.row as u32];
        gpu.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&GridUniform {
                cursor_style: if grid.is_scrollback() {
                    CursorShape::Hidden as u32
                }
                else {
                    self.uniform.cursor_style
                },
                ..self.uniform
            }),
        );

        // デバッグテクスチャの描画
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..6, 0..1);

        drop(render_pass);

        gpu.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        surface_texture.present();
    }
}
