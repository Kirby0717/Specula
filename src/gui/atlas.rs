use super::GpuContext;
use crate::core::CellFlags;

use std::collections::HashMap;

use fontdb::{Database, Family, Query, Style, Weight};
use fontdue::Font;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GlyphInfo {
    pub uv_rect: [f32; 4],
    // 左上基準
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub style: FontStyle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct GlyphKey {
    pub c: char,
    pub style: FontStyle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}
impl FontStyle {
    fn fallback_chain(self) -> &'static [FontStyle] {
        match self {
            FontStyle::Regular => &[],
            FontStyle::Bold => &[FontStyle::Regular],
            FontStyle::Italic => &[FontStyle::Regular],
            FontStyle::BoldItalic => {
                &[FontStyle::Bold, FontStyle::Italic, FontStyle::Regular]
            }
        }
    }
    pub fn from_cell_flags(flags: CellFlags) -> Self {
        match (
            flags.contains(CellFlags::BOLD),
            flags.contains(CellFlags::ITALIC),
        ) {
            (true, true) => FontStyle::BoldItalic,
            (true, false) => FontStyle::Bold,
            (false, true) => FontStyle::Italic,
            (false, false) => FontStyle::Regular,
        }
    }
    pub fn is_bold(self) -> bool {
        matches!(self, FontStyle::Bold | FontStyle::BoldItalic)
    }
    pub fn is_italic(self) -> bool {
        matches!(self, FontStyle::Italic | FontStyle::BoldItalic)
    }
}
#[derive(Debug)]
pub(super) struct GlyphAtlas {
    // フォント
    font: Font,
    font_bold: Option<Font>,
    font_italic: Option<Font>,
    font_bold_italic: Option<Font>,
    ascent: i32,
    px: f32,
    // キャッシュ
    cache: HashMap<GlyphKey, GlyphInfo>,
    // テクスチャ
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    // 箱詰め管理
    cursor: [u32; 2],
    row_height: u32,
}
impl GlyphAtlas {
    pub const ATLAS_SIZE: u32 = 1 << 11;
    const GLYPH_PADDING: u32 = 1;
    pub fn new(
        gpu: &GpuContext,
        scale_factor: f64,
        config: &crate::config::FontConfig,
    ) -> Self {
        // フォントの読み込み
        let mut db = Database::new();
        db.load_system_fonts();

        let font = load_font(
            &db,
            config.family.as_deref(),
            Weight::NORMAL,
            Style::Normal,
        )
        .expect("モノスペースフォントが見つかりません");
        let font_name = font.name().unwrap_or_default();
        log::info!("フォント: {font_name}");

        let (f, w, s) = resolve_variant(
            &config.bold,
            font_name,
            Weight::BOLD,
            Style::Normal,
        );
        let font_bold = load_font(&db, Some(f), w, s);
        if let Some(font) = &font_bold {
            log::info!("太字フォント: {}", font.name().unwrap_or_default());
        }

        let (f, w, s) = resolve_variant(
            &config.italic,
            font_name,
            Weight::NORMAL,
            Style::Italic,
        );
        let font_italic = load_font(&db, Some(f), w, s);
        if let Some(font) = &font_italic {
            log::info!("斜体フォント: {}", font.name().unwrap_or_default());
        }

        let (f, w, s) = resolve_variant(
            &config.bold_italic,
            font_name,
            Weight::BOLD,
            Style::Italic,
        );
        let font_bold_italic = load_font(&db, Some(f), w, s);
        if let Some(font) = &font_bold_italic {
            log::info!("太字斜体フォント: {}", font.name().unwrap_or_default());
        }

        let px = config.size.to_px(scale_factor) as f32;
        let ascent =
            font.horizontal_line_metrics(px).unwrap().ascent.floor() as i32;

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

        GlyphAtlas {
            font,
            font_bold,
            font_italic,
            font_bold_italic,
            ascent,
            px,
            cache: HashMap::default(),
            texture,
            view,
            cursor: [Self::GLYPH_PADDING, Self::GLYPH_PADDING],
            row_height: 0,
        }
    }
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
    pub fn clear(&mut self, gpu: &GpuContext) {
        self.cache.clear();
        self.cursor = [Self::GLYPH_PADDING, Self::GLYPH_PADDING];
        self.row_height = 0;
        let mut encoder =
            gpu.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("アトラスクリア"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        });
        gpu.queue.submit(std::iter::once(encoder.finish()));
    }
    pub fn cell_size(&self) -> [u32; 2] {
        let lm = self.font.horizontal_line_metrics(self.px).unwrap();
        let h = (lm.ascent - lm.descent).floor() as u32;
        let (m, _) = self.font.rasterize(' ', self.px);
        let w = m.advance_width.floor() as u32;
        [w, h]
    }
    fn font_for_style(&self, style: FontStyle) -> Option<&Font> {
        match style {
            FontStyle::Regular => Some(&self.font),
            FontStyle::Bold => self.font_bold.as_ref(),
            FontStyle::Italic => self.font_italic.as_ref(),
            FontStyle::BoldItalic => self.font_bold_italic.as_ref(),
        }
    }
    fn resolve_font(&self, style: FontStyle) -> (&Font, FontStyle) {
        if let Some(font) = self.font_for_style(style) {
            return (font, style);
        }
        for &fallback in style.fallback_chain() {
            if let Some(font) = self.font_for_style(fallback) {
                return (font, fallback);
            }
        }
        (&self.font, FontStyle::Regular)
    }
    pub fn get_or_insert(
        &mut self,
        gpu: &GpuContext,
        key: GlyphKey,
    ) -> Option<GlyphInfo> {
        if let Some(&info) = self.cache.get(&key) {
            return Some(info);
        }

        // 実際に使うフォントの決定
        let (font, resolved_style) = self.resolve_font(key.style);

        // すでにあるならそれを返す
        if let Some(&info) = self.cache.get(&GlyphKey {
            c: key.c,
            style: resolved_style,
        }) {
            self.cache.insert(key, info);
            return Some(info);
        }

        // フォントのラスタライズ
        let (metrics, bitmap) = font.rasterize(key.c, self.px);

        let width = metrics.width as u32;
        let height = metrics.height as u32;

        if width == 0 || height == 0 {
            let info = GlyphInfo {
                uv_rect: [0.0, 0.0, 0.0, 0.0],
                offset: [0.0, 0.0],
                size: [0.0, 0.0],
                style: key.style,
            };
            self.cache.insert(key, info);
            return Some(info);
        }

        // 改行
        if Self::ATLAS_SIZE < self.cursor[0] + width + Self::GLYPH_PADDING {
            self.cursor[0] = 0;
            self.cursor[1] += self.row_height + Self::GLYPH_PADDING;
            self.row_height = 0;
        }

        let [x, y] = self.cursor;
        if Self::ATLAS_SIZE < x + width + Self::GLYPH_PADDING
            || Self::ATLAS_SIZE < y + height + Self::GLYPH_PADDING
        {
            log::warn!("グリフがアトラスに入りませんでした");
            return None;
        }

        // アトラスへ書き込み
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfoBase {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // カーソルを進める
        self.cursor[0] += width + Self::GLYPH_PADDING;
        self.row_height = self.row_height.max(height);

        // グリフ情報の作成
        let xf = x as f32;
        let yf = y as f32;
        let widthf = width as f32;
        let heightf = height as f32;
        let atlas_sizef = Self::ATLAS_SIZE as f32;
        let info = GlyphInfo {
            uv_rect: [
                xf / atlas_sizef,
                yf / atlas_sizef,
                (xf + widthf) / atlas_sizef,
                (yf + heightf) / atlas_sizef,
            ],
            offset: [
                metrics.xmin as f32,
                (self.ascent - metrics.ymin - height as i32) as f32,
            ],
            size: [widthf, heightf],
            style: resolved_style,
        };

        self.cache.insert(key, info);
        Some(info)
    }
}

fn resolve_variant<'a>(
    variant: &'a Option<crate::config::FontStyleConfig>,
    default_family: &'a str,
    default_weight: Weight,
    default_style: Style,
) -> (&'a str, Weight, Style) {
    match variant.as_ref() {
        Some(c) => c.resolve(default_family, default_weight, default_style),
        None => (default_family, default_weight, default_style),
    }
}
fn load_font(
    db: &Database,
    family: Option<&str>,
    weight: Weight,
    style: Style,
) -> Option<Font> {
    let mut families = vec![];
    if let Some(name) = family {
        families.push(Family::Name(name));
    }
    families.extend([
        Family::Name("DejaVu Sans Mono"),
        Family::Name("Liberation Mono"),
        Family::Name("Noto Sans Mono"),
        Family::Monospace,
    ]);

    let query = Query {
        families: &families,
        weight,
        style,
        ..Default::default()
    };
    let id = db.query(&query)?;
    let face = db.face(id).unwrap();
    // 太さ・スタイルが違うならNone
    if 100 < face.weight.0.abs_diff(weight.0) || face.style != style {
        return None;
    }
    db.with_face_data(id, |data, face_index| {
        let settings = fontdue::FontSettings {
            collection_index: face_index,
            ..Default::default()
        };
        fontdue::Font::from_bytes(data, settings)
            .expect("フォントのロードに失敗")
    })
}
