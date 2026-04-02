use super::atlas::{FontStyle, GlyphAtlas, GlyphInfo, GlyphKey};
use crate::core::{
    CellFlags, CursorStyle, Terminal, TerminalMode, rgb_to_rgba,
    rgb_to_rgba_f32,
};

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
    pub fn new(
        window: &Arc<Window>,
        display_handle: Box<dyn wgpu::wgt::instance::WgpuHasDisplayHandle>,
    ) -> Self {
        // デバイスとキューの作成
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle(display_handle),
        );
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
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuCell {
    cell_pos: [u32; 2],
    fg: [u8; 4],
    bg: [u8; 4],
    uv_rect: [f32; 4],
    offset: [f32; 2],
    size: [f32; 2],
    flags: u32,
    _padding1: u32,
}
impl GpuCell {
    const ATTRIBS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Uint32x2,
        1 => Unorm8x4,
        2 => Unorm8x4,
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
    cursor_range: [u32; 2],
    cursor_fg: [f32; 4],
    cursor_bg: [f32; 4],
    cursor_style: u32,
    _padding1: u32,
    viewport_size: [f32; 2],
    selection_range: [u32; 2],
    _padding2: [u32; 2],
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
    ime_colors: [[u8; 3]; 2],
    palette: [[u8; 3]; 18],
    preedit_text: String,
    preedit_cursor: Option<(usize, usize)>,
    padding_color: wgpu::Color,
    padding: [f32; 2],
}
impl Renderer {
    const IME_BUFFER_CELLS: usize = 256;
    pub fn new(
        gpu: &GpuContext,
        atlas: &GlyphAtlas,
        terminal: &Terminal,
        config: &crate::config::Config,
    ) -> Self {
        let color_config = &config.colors;

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
            size: ((grid.grid_rows() * grid.grid_cols()
                + Self::IME_BUFFER_CELLS)
                * size_of::<GpuCell>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let [cell_width, cell_height] = atlas.cell_size();
        let cursor_colors = color_config.to_cursor_colors();
        let ime_colors = color_config.to_ime_colors();
        let palette = color_config.to_palette();
        let padding_color = {
            let [r, g, b] = config.window.padding_color.unwrap_or(
                palette[crate::core::NamedColor::Background as usize],
            );
            wgpu::Color {
                r: r as f64 / 255.0,
                g: g as f64 / 255.0,
                b: b as f64 / 255.0,
                a: 1.0,
            }
        };
        let uniform = GridUniform {
            cell_size: [cell_width as f32, cell_height as f32],
            grid_size: [grid.grid_cols() as u32, grid.grid_rows() as u32],
            atlas_size: [
                GlyphAtlas::ATLAS_SIZE as f32,
                GlyphAtlas::ATLAS_SIZE as f32,
            ],
            cursor_range: {
                let point = grid.cursor().point;
                [point.col as u32, point.row as u32]
            },
            cursor_fg: rgb_to_rgba_f32(cursor_colors[0]),
            cursor_bg: rgb_to_rgba_f32(cursor_colors[1]),
            cursor_style: terminal.cursor_style() as u32,
            _padding1: Default::default(),
            viewport_size: {
                let window_size = gpu.size;
                [window_size.width as f32, window_size.height as f32]
            },
            selection_range: [0, 0],
            _padding2: Default::default(),
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
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
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
                multiview_mask: None,
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
                multiview_mask: None,
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
            ime_colors,
            palette,
            preedit_text: String::new(),
            preedit_cursor: None,
            padding_color,
            padding: Default::default(),
        }
    }
    pub fn resize(
        &mut self,
        gpu: &GpuContext,
        atlas: &GlyphAtlas,
        rows: usize,
        cols: usize,
        padding: [f32; 2],
    ) {
        self.padding = padding;
        let need_buffer_size = ((rows * cols + Self::IME_BUFFER_CELLS)
            * size_of::<GpuCell>()) as u64;
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
        let [cell_w, cell_h] = atlas.cell_size();
        self.uniform.viewport_size =
            [cols as f32 * cell_w as f32, rows as f32 * cell_h as f32];
        gpu.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.uniform),
        );
    }
    pub fn render(
        &mut self,
        window: &winit::window::Window,
        gpu: &mut GpuContext,
        atlas: &mut GlyphAtlas,
        terminal: &Terminal,
        selection_range: [u32; 2],
    ) {
        // スワップチェーンのバックバッファの取得
        let surface_texture = {
            use wgpu::CurrentSurfaceTexture::*;
            match gpu.surface.get_current_texture() {
                Success(frame) => frame,
                Suboptimal(frame) => {
                    gpu.configure_surface();
                    frame
                }
                Timeout | Occluded => {
                    return;
                }
                Outdated => {
                    gpu.configure_surface();
                    return;
                }
                Lost => {
                    // 再作成の方が最適
                    gpu.configure_surface();
                    return;
                }
                Validation => {
                    log::error!("surface validation error");
                    return;
                }
            }
        };
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
                        load: wgpu::LoadOp::Clear(self.padding_color),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

        let grid = terminal.active_grid();
        let rows = grid.grid_rows();
        let cols = grid.grid_cols();

        // グリフの確保
        'retry: for attempt in 0..2 {
            for y in 0..rows {
                let row = grid.viewport_row(y);
                for cell in row.iter().take(cols) {
                    let key = GlyphKey {
                        c: cell.c,
                        style: FontStyle::from_cell_flags(cell.flags),
                    };
                    if atlas.get_or_insert(gpu, key).is_none() {
                        if attempt == 0 {
                            log::warn!(
                                "グリフを入れるためアトラスを再構築します"
                            );
                            atlas.clear(gpu);
                            continue 'retry;
                        }
                        panic!(
                            "画面を描画するのに必要なグリフをアトラスに入れることに失敗しました"
                        );
                    }
                }
            }
            for c in self.preedit_text.chars() {
                let key = GlyphKey {
                    c,
                    style: FontStyle::Regular,
                };
                if atlas.get_or_insert(gpu, key).is_none() {
                    if attempt == 0 {
                        log::warn!("グリフを入れるためアトラスを再構築します");
                        atlas.clear(gpu);
                        continue 'retry;
                    }
                    panic!(
                        "画面を描画するのに必要なグリフをアトラスに入れることに失敗しました"
                    );
                }
            }
            break;
        }

        // グリッドを変換
        let mut cell_buffer = vec![];
        let mut empty_cell_buffer = vec![];
        for y in 0..rows {
            let row = grid.viewport_row(y);
            for (x, cell) in row.iter().take(cols).enumerate() {
                let GlyphInfo {
                    uv_rect,
                    offset,
                    size,
                    style,
                } = atlas
                    .get_or_insert(
                        gpu,
                        GlyphKey {
                            c: cell.c,
                            style: FontStyle::from_cell_flags(cell.flags),
                        },
                    )
                    .expect("事前に確保済み");
                let mut flags = cell.flags;
                if style.is_bold() {
                    flags.remove(CellFlags::BOLD);
                }
                if style.is_italic() {
                    flags.remove(CellFlags::ITALIC);
                }
                let flags = flags.bits() as u32;
                let fg = cell.fg.color_to_rgba(&self.palette);
                let bg = cell.bg.color_to_rgba(&self.palette);
                if size[0] <= 0.0
                    || size[1] <= 0.0
                    || cell.flags.contains(CellFlags::WIDE_CHAR_SPACER)
                {
                    empty_cell_buffer.push(GpuCell {
                        cell_pos: [x as u32, y as u32],
                        fg,
                        bg,
                        uv_rect: [0.0, 0.0, 0.0, 0.0],
                        offset: [0.0, 0.0],
                        size: [0.0, 0.0],
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
        let grid_size = (cell_buffer.len() + empty_cell_buffer.len()) as u32;
        let grid_bg_range = 0..grid_size;
        let grid_fg_range = 0..cell_buffer.len() as u32;

        // IMEプレビューを変換
        let mut preedit_buffer = vec![];
        let mut empty_preedit_buffer = vec![];
        let mut ime_position = grid.cursor().point;
        let mut ime_fg = rgb_to_rgba(self.ime_colors[0]);
        let mut ime_bg = rgb_to_rgba(self.ime_colors[1]);
        let preedit_text = if self.preedit_text.is_empty() {
            vec![]
        }
        else if let Some((begin, end)) = self.preedit_cursor {
            vec![
                &self.preedit_text[..begin],
                &self.preedit_text[begin..end],
                &self.preedit_text[end..],
            ]
        }
        else {
            vec![self.preedit_text.as_str()]
        };
        let mut bar_cursor = None;
        for text in preedit_text {
            if text.is_empty() {
                bar_cursor = Some(ime_position);
            }
            for c in text.chars() {
                use unicode_width::UnicodeWidthChar;
                let width = c.width().unwrap_or(0);
                match width {
                    0 => continue,
                    1 | 2 => {
                        if cols < ime_position.col + width {
                            if ime_position.row == rows - 1 {
                                break;
                            }
                            ime_position.row += 1;
                            ime_position.col = 0;
                        }
                        let GlyphInfo {
                            uv_rect,
                            offset,
                            size,
                            ..
                        } = atlas
                            .get_or_insert(
                                gpu,
                                GlyphKey {
                                    c,
                                    style: FontStyle::Regular,
                                },
                            )
                            .expect("事前に確保済み");
                        let flags = CellFlags::UNDERLINE.bits() as u32;
                        if size[0] <= 0.0 || size[1] <= 0.0 {
                            empty_preedit_buffer.push(GpuCell {
                                cell_pos: [
                                    ime_position.col as u32,
                                    ime_position.row as u32,
                                ],
                                fg: ime_fg,
                                bg: ime_bg,
                                uv_rect: [0.0, 0.0, 0.0, 0.0],
                                offset: [0.0, 0.0],
                                size: [0.0, 0.0],
                                flags,
                                _padding1: Default::default(),
                            });
                        }
                        else {
                            preedit_buffer.push(GpuCell {
                                cell_pos: [
                                    ime_position.col as u32,
                                    ime_position.row as u32,
                                ],
                                fg: ime_fg,
                                bg: ime_bg,
                                uv_rect,
                                offset,
                                size,
                                flags,
                                _padding1: Default::default(),
                            });
                        }
                        if width == 2 {
                            empty_preedit_buffer.push(GpuCell {
                                cell_pos: [
                                    ime_position.col as u32 + 1,
                                    ime_position.row as u32,
                                ],
                                fg: ime_fg,
                                bg: ime_bg,
                                uv_rect: [0.0, 0.0, 0.0, 0.0],
                                offset: [0.0, 0.0],
                                size: [0.0, 0.0],
                                flags,
                                _padding1: Default::default(),
                            });
                        }
                        ime_position.col += width;
                    }
                    _ => unreachable!("Unicodeの幅は2以下"),
                }
                if Self::IME_BUFFER_CELLS
                    < preedit_buffer.len() + empty_preedit_buffer.len()
                {
                    log::warn!("IMEプレビューの文字数が上限を超えました");
                    preedit_buffer.pop();
                    if width == 2 {
                        empty_preedit_buffer.pop();
                    }
                    break;
                }
            }
            std::mem::swap(&mut ime_fg, &mut ime_bg);
        }
        let preedit_size =
            (preedit_buffer.len() + empty_preedit_buffer.len()) as u32;
        let preedit_bg_range = grid_size..grid_size + preedit_size;
        let preedit_fg_range =
            grid_size..grid_size + preedit_buffer.len() as u32;

        // バッファーを送信
        gpu.queue.write_buffer(
            &self.cell_buffer,
            0,
            bytemuck::cast_slice(&cell_buffer),
        );
        gpu.queue.write_buffer(
            &self.cell_buffer,
            (cell_buffer.len() * size_of::<GpuCell>()) as u64,
            bytemuck::cast_slice(&empty_cell_buffer),
        );
        if preedit_size != 0 {
            gpu.queue.write_buffer(
                &self.cell_buffer,
                ((cell_buffer.len() + empty_cell_buffer.len())
                    * size_of::<GpuCell>()) as u64,
                bytemuck::cast_slice(&preedit_buffer),
            );
            gpu.queue.write_buffer(
                &self.cell_buffer,
                ((cell_buffer.len()
                    + empty_cell_buffer.len()
                    + preedit_buffer.len())
                    * size_of::<GpuCell>()) as u64,
                bytemuck::cast_slice(&empty_preedit_buffer),
            );
        }

        // Uniformの更新
        if preedit_size == 0 {
            let width =
                if grid.cell_at_cursor().flags.contains(CellFlags::WIDE_CHAR) {
                    2
                }
                else {
                    1
                };
            let point = grid.cursor().point;
            let begin_index = (point.row * cols + point.col) as u32;
            self.uniform.cursor_range = [begin_index, begin_index + width];
        }
        else {
            if let Some(point) = bar_cursor {
                let begin_index = (point.row * cols + point.col) as u32;
                self.uniform.cursor_range = [begin_index, begin_index + 1];
            }
            else {
                self.uniform.cursor_range = [0, 0];
            }
        }
        self.uniform.cursor_style = terminal.cursor_style() as u32;
        if bar_cursor.is_some() {
            self.uniform.cursor_style = CursorStyle::Bar as u32;
        }
        if !terminal.mode().contains(TerminalMode::CURSOR_VISIBLE) {
            self.uniform.cursor_style = CursorStyle::Hidden as u32;
        }
        if self.uniform.cursor_style == CursorStyle::Bar as u32 {
            self.uniform.cursor_range[1] = self.uniform.cursor_range[0] + 1;
        }
        self.uniform.selection_range = selection_range;
        gpu.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.uniform),
        );

        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.cell_buffer.slice(..));
        render_pass.set_viewport(
            self.padding[0],
            self.padding[1],
            self.uniform.viewport_size[0],
            self.uniform.viewport_size[1],
            0.0,
            1.0,
        );
        // グリッドの描画
        render_pass.set_pipeline(&self.cell_render_pipeline);
        render_pass.draw(0..6, grid_bg_range);
        render_pass.set_pipeline(&self.glyph_render_pipeline);
        render_pass.draw(0..6, grid_fg_range);
        // IMEプレビューの描画
        if preedit_size != 0 {
            render_pass.set_pipeline(&self.cell_render_pipeline);
            render_pass.draw(0..6, preedit_bg_range);
            render_pass.set_pipeline(&self.glyph_render_pipeline);
            render_pass.draw(0..6, preedit_fg_range);
        }

        drop(render_pass);

        gpu.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        surface_texture.present();

        // IMEの範囲設定
        if preedit_size != 0 {
            let ime_start_position = grid.cursor().point;
            let ime_end_position = ime_position;
            let [cell_w, cell_h] = atlas.cell_size();
            let x = ime_start_position.col as u32 * cell_w;
            let y = ime_start_position.row as u32 * cell_h;
            window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(
                    x + self.padding[0] as u32,
                    y + self.padding[1] as u32,
                ),
                winit::dpi::PhysicalSize::new(
                    cell_w,
                    cell_h
                        * (ime_end_position.row - ime_start_position.row + 1)
                            as u32,
                ),
            );
        }
    }
    pub fn set_preedit(
        &mut self,
        text: String,
        cursor: Option<(usize, usize)>,
    ) {
        self.preedit_text = text;
        self.preedit_cursor = cursor;
    }
}
