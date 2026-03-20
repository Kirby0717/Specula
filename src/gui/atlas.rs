use super::GpuContext;

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphIndex {
    Wide(u32, u32),
    Narrow(u32),
}
pub struct GlyphAtlas {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    cache: HashMap<char, GlyphIndex>,
    next_slot: u32,
    font: fontdue::Font,
    px: f32,
    pub cell_width: u32,
    pub cell_height: u32,
    ascent: i32,
    pub slots_per_row: u32,
}
impl GlyphAtlas {
    pub const ATLAS_SIZE: u32 = 1 << 11;
    pub fn new(gpu: &GpuContext, px: f32) -> Self {
        // フォントの読み込み
        let font_data = std::fs::read("./BizinGothicCCNerdFont-Regular.ttf")
            .expect("フォントの読み込みに失敗しました");
        let font = fontdue::Font::from_bytes(
            font_data,
            fontdue::FontSettings::default(),
        )
        .expect("フォントの解析に失敗しました");
        let line_metrics = font.horizontal_line_metrics(px).unwrap();
        let cell_height =
            (line_metrics.ascent - line_metrics.descent).ceil() as u32;
        let ascent = line_metrics.ascent.ceil() as i32;
        let (metrics, _) = font.rasterize('A', px);
        let cell_width = metrics.advance_width.ceil() as u32;

        // アトラステクスチャの作成
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("アトラステクスチャ"),
            size: wgpu::Extent3d {
                width: Self::ATLAS_SIZE,
                height: Self::ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let slots_per_row = Self::ATLAS_SIZE / cell_width;

        GlyphAtlas {
            texture,
            view,
            cache: HashMap::new(),
            next_slot: 0,
            font,
            px,
            cell_width,
            cell_height,
            ascent,
            slots_per_row,
        }
    }
    pub fn get_or_insert(
        &mut self,
        gpu: &GpuContext,
        c: char,
        is_wide: bool,
    ) -> GlyphIndex {
        if let Some(index) = self.cache.get(&c) {
            return *index;
        }

        // フォントのラスタライズ
        let (metrics, bitmap) = self.font.rasterize(c, self.px);
        let dst_x = metrics.xmin.max(0) as u32;
        let dst_y =
            (self.ascent - metrics.ymin - metrics.height as i32).max(0) as u32;

        // アトラスへ書き込み
        // TODO: 上書き時の前のデータの消去
        if is_wide {
            let slot_left = self.next_slot;
            self.next_slot += 1;
            if dst_x <= self.cell_width {
                let left_width =
                    (self.cell_width - dst_x).min(metrics.width as u32);
                let (slot_lx, slot_ly) = self.slot_origin(slot_left);
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfoBase {
                        texture: &self.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: slot_lx + dst_x,
                            y: slot_ly + dst_y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &bitmap,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(metrics.width as u32),
                        rows_per_image: Some(metrics.height as u32),
                    },
                    wgpu::Extent3d {
                        width: left_width,
                        height: metrics.height as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }

            let slot_right = self.next_slot;
            self.next_slot += 1;
            if self.cell_width <= metrics.width as u32 + dst_x {
                let right_skip = self.cell_width.saturating_sub(dst_x);
                let right_width =
                    (metrics.width as u32).saturating_sub(right_skip);
                let (slot_rx, slot_ry) = self.slot_origin(slot_right);
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfoBase {
                        texture: &self.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: slot_rx,
                            y: slot_ry + dst_y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &bitmap,
                    wgpu::TexelCopyBufferLayout {
                        offset: right_skip as u64,
                        bytes_per_row: Some(metrics.width as u32),
                        rows_per_image: Some(metrics.height as u32),
                    },
                    wgpu::Extent3d {
                        width: right_width,
                        height: metrics.height as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }

            let index = GlyphIndex::Wide(slot_left, slot_right);
            self.cache.insert(c, index);
            index
        }
        else {
            let slot = self.next_slot;
            self.next_slot += 1;
            let (slot_x, slot_y) = self.slot_origin(slot);

            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfoBase {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: slot_x + dst_x,
                        y: slot_y + dst_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &bitmap,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(metrics.width as u32),
                    rows_per_image: Some(metrics.height as u32),
                },
                wgpu::Extent3d {
                    width: (metrics.width as u32).min(self.cell_width),
                    height: metrics.height as u32,
                    depth_or_array_layers: 1,
                },
            );

            let index = GlyphIndex::Narrow(slot);
            self.cache.insert(c, index);
            index
        }
    }
    fn slot_origin(&self, slot: u32) -> (u32, u32) {
        let slot_origin_x = (slot % self.slots_per_row) * self.cell_width;
        let slot_origin_y = (slot / self.slots_per_row) * self.cell_height;
        (slot_origin_x, slot_origin_y)
    }
}
