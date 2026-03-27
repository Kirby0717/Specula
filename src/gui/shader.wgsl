@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var<uniform> grid: GridUniform;

struct GpuCell {
    @location(0) cell_pos: vec2<u32>,
    @location(1) fg: vec4<f32>,
    @location(2) bg: vec4<f32>,
    @location(3) uv_rect: vec4<f32>,
    @location(4) offset: vec2<f32>,
    @location(5) size: vec2<f32>,
    @location(6) flags: u32,
}
struct GridUniform {
    cell_size: vec2<f32>,
    grid_size: vec2<u32>,
    atlas_size: vec2<f32>,
    cursor_pos: vec2<u32>,
    cursor_style: u32,
    _pad2: u32,
    viewport_size: vec2<f32>,
    selection_range: vec2<u32>,
}

// カーソルや装飾などは前景と背景の色を切り替える
// 複数重なっている時元に戻ることで反転の中の下線が見えるようになる

// 背景
@vertex
fn vs_cell(@builtin(vertex_index) i: u32, cell: GpuCell) -> CellOut {
    let rect = array<vec2<u32>, 6>(
        vec2(0, 0),
        vec2(1, 0),
        vec2(0, 1),
        vec2(1, 0),
        vec2(1, 1),
        vec2(0, 1),
    )[i];

    let cell_pos = cell.cell_pos;
    let pixel_pos = vec2<f32>(rect + cell_pos) * grid.cell_size;
    let ndc = pixel_pos / grid.viewport_size * 2.0 - 1.0;
    let pos = vec4(ndc.x, -ndc.y, 0.0, 1.0);

    var fg = cell.fg;
    var bg = cell.bg;
    let flags = cell.flags;

    return CellOut(
        pos,
        cell_pos,
        fg,
        bg,
        flags
    );
}
struct CellOut {
    @builtin(position)              pos: vec4<f32>,
    @location(0)                    cell_pos: vec2<u32>,
    @location(1)                    fg: vec4<f32>,
    @location(2)                    bg: vec4<f32>,
    @location(3) @interpolate(flat) flags: u32,
}
@fragment
fn fs_cell(cell: CellOut) -> @location(0) vec4<f32> {
    let flags = cell.flags;
    var fg = cell.fg;
    var bg = cell.bg;

    // 選択
    let cell_index = cell.cell_pos.y * grid.grid_size.x + cell.cell_pos.x;
    let selection_range = grid.selection_range;
    if selection_range.x <= cell_index && cell_index < selection_range.y {
        let tem = fg;
        fg = bg;
        bg = tem;
    }

    // 背景色の反転
    if (flags & 0x0020) != 0 {
        let tem = fg;
        fg = bg;
        bg = tem;
    }
    // 下線
    if (flags & 0x0008) != 0 {
        let local_pos = cell.pos.xy % grid.cell_size;
        if grid.cell_size.y - 2.5 < local_pos.y && local_pos.y < grid.cell_size.y - 0.5 {
            let tem = fg;
            fg = bg;
            bg = tem;
        }
    }
    // 取り消し線
    if (flags & 0x0080) != 0 {
        let local_pos = cell.pos.xy % grid.cell_size;
        let center = grid.cell_size.y / 2.0;
        if center - 1.5 < local_pos.y && local_pos.y < center + 1.5 {
            let tem = fg;
            fg = bg;
            bg = tem;
        }
    }

    // カーソル
    let local_pos = cell.pos.xy % grid.cell_size;
    if all(cell.cell_pos == grid.cursor_pos) {
        apply_cursor(&fg, &bg, local_pos);
    }

    return bg;
}

// 文字
@vertex
fn vs_glyph(@builtin(vertex_index) i: u32, cell: GpuCell) -> GlyphOut {
    let rect = array<vec2<f32>, 6>(
        vec2(0, 0),
        vec2(1, 0),
        vec2(0, 1),
        vec2(1, 0),
        vec2(1, 1),
        vec2(0, 1),
    )[i];

    let glyph_size = cell.size;
    var glyph_pos = rect * vec2<f32>(glyph_size);

    let flags = cell.flags;
    // 斜体
    if (flags & 0x0004) != 0 {
        if rect.y < 0.5 {
            let skew = 0.2;
            glyph_pos.x += skew * grid.cell_size.y;
        }
    }

    let origin = vec2<f32>(cell.cell_pos) * grid.cell_size + cell.offset;
    let pixel_pos = origin + glyph_pos;
    let ndc = pixel_pos / grid.viewport_size * 2.0 - 1.0;
    let pos = vec4(ndc.x, -ndc.y, 0.0, 1.0);

    let uv = mix(cell.uv_rect.xy, cell.uv_rect.zw, rect);
    var fg = cell.fg;
    var bg = cell.bg;

    return GlyphOut(pos, cell.cell_pos, uv, fg, bg, flags, glyph_pos);
}
struct GlyphOut {
    @builtin(position)              pos: vec4<f32>,
    @location(0)                    cell_pos: vec2<u32>,
    @location(1)                    uv: vec2<f32>,
    @location(2)                    fg: vec4<f32>,
    @location(3)                    bg: vec4<f32>,
    @location(4) @interpolate(flat) flags: u32,
    @location(5)                    glyph_pos: vec2<f32>,
}
@fragment
fn fs_glyph(glyph: GlyphOut) -> @location(0) vec4<f32> {
    let flags = glyph.flags;
    var fg = glyph.fg;
    var bg = glyph.bg;

    // 選択
    let cell_index = glyph.cell_pos.y * grid.grid_size.x + glyph.cell_pos.x;
    let selection_range = grid.selection_range;
    if selection_range.x <= cell_index && cell_index < selection_range.y {
        let tem = fg;
        fg = bg;
        bg = tem;
    }

    // 背景色の反転
    if (flags & 0x0020) != 0 {
        let tem = fg;
        fg = bg;
        bg = tem;
    }
    // 不可視
    if (flags & 0x0040) != 0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // カーソル
    let local_pos = glyph.pos.xy % grid.cell_size;
    if all(glyph.cell_pos == grid.cursor_pos) {
        apply_cursor(&fg, &bg, local_pos);
    }

    // 斜体
    if (flags & 0x0004) != 0 {
        let alpha = textureSample(atlas, s, glyph.uv).r;
        return vec4<f32>(fg.rgb, alpha);
    } else {
        let texel = vec2<i32>(glyph.uv * grid.atlas_size);
        let alpha = textureLoad(atlas, texel, 0).r;
        return vec4<f32>(fg.rgb, alpha);
    }
}

fn apply_cursor(fg: ptr<function, vec4<f32>>, bg: ptr<function, vec4<f32>>, local_pos: vec2<f32>) {
    let cursor_style = grid.cursor_style;
    switch cursor_style {
        case 0: {}
        case 1, 4: {
            let tmp = *fg;
            *fg = *bg;
            *bg = tmp;
        }
        case 2, 5: {
            if grid.cell_size.y - 2.5 < local_pos.y && local_pos.y < grid.cell_size.y - 0.5 {
                let tmp = *fg;
                *fg = *bg;
                *bg = tmp;
            }
        }
        case 3, 6: {
            if local_pos.x < 1.5 {
                let tmp = *fg;
                *fg = *bg;
                *bg = tmp;
            }
        }
        default: {}
    }
}
