@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var<storage, read> cells: array<GpuCell>;
@group(0) @binding(3) var<uniform> grid_uniform: GridUniform;

struct GpuCell {
    glyph_index: u32,
    flags: u32,
    _pad2: u32,
    _pad3: u32,
    fg: vec4<f32>,
    bg: vec4<f32>,
}
struct GridUniform {
    cell_size: vec2<f32>,
    grid_size: vec2<u32>,
    atlas_size: vec2<f32>,
    slots_per_row: u32,
    _pad1: u32,
    cursor_pos: vec2<u32>,
    cursor_style: u32,
    _pad2: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let pos = array<vec2<f32>, 6>(
        vec2(-1.0,  1.0),
        vec2( 1.0,  1.0),
        vec2(-1.0, -1.0),
        vec2( 1.0,  1.0),
        vec2( 1.0, -1.0),
        vec2(-1.0, -1.0),
    );
    return vec4<f32>(pos[i], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let cell_size = grid_uniform.cell_size;
    let grid_size = grid_uniform.grid_size;

    let cell_pos = vec2<u32>(pos.xy / cell_size);
    if any(grid_size <= cell_pos) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0);
    }

    let slots_per_row = grid_uniform.slots_per_row;
    let atlas_size = grid_uniform.atlas_size;

    let index = cell_pos.y * grid_size.x + cell_pos.x;
    let cell = cells[index];
    let slot_pos = vec2<u32>(
        cell.glyph_index % slots_per_row,
        cell.glyph_index / slots_per_row,
    );
    let local_pos = vec2<u32>(pos.xy) % vec2<u32>(cell_size);

    let uv = vec2<f32>(slot_pos * vec2<u32>(cell_size) + local_pos) / atlas_size;
    let alpha = textureSample(atlas, s, uv).r;
    var color = mix(cell.bg, cell.fg, alpha);

    let cursor_pos = grid_uniform.cursor_pos;
    if all(cell_pos == cursor_pos) {
        let cursor_style = grid_uniform.cursor_style;
        switch cursor_style {
            // 非表示
            case 0: {}
            // ブロック
            case 1: {
                color = mix(cell.fg, cell.bg, alpha);
            }
            // 下線
            case 2: {
                if u32(cell_size.y) - 2 <= local_pos.y {
                    color = cell.fg;
                }
            }
            // 縦線
            case 3: {
                if local_pos.x < 2 {
                    color = cell.fg;
                }
            }
            // ブロック点滅
            case 4: {
                color = mix(cell.fg, cell.bg, alpha);
            }
            // 下線点滅
            case 5: {
                if u32(cell_size.y) - 2 <= local_pos.y {
                    color = cell.fg;
                }
            }
            // 縦線点滅
            case 6: {
                if local_pos.x < 2 {
                    color = cell.fg;
                }
            }
            // 不正なカーソルは紫
            default: {
                color = vec4<f32>(1.0, 0.0, 1.0, 1.0);
            }
        }
    } else {
        let flags = cell.flags;
        // 太字
        if (flags & 0x0001) != 0 {}
        // 減光
        if (flags & 0x0002) != 0 {}
        // 斜体
        if (flags & 0x0004) != 0 {
            let skew = 0.2;
            var local_pos = local_pos;
            local_pos.x     -= u32(skew * (cell_size.y - f32(local_pos.y)));
            let uv = vec2<f32>(slot_pos * vec2<u32>(cell_size) + local_pos) / atlas_size;
            let alpha = textureSample(atlas, s, uv).r;
            color = mix(cell.bg, cell.fg, alpha);
        }
        // 下線
        if (flags & 0x0008) != 0 {
            if u32(cell_size.y) - 2 <= local_pos.y {
                color = cell.fg;
            }
        }
        // 点滅 ( あまり使われない )
        if (flags & 0x0010) != 0 {}
        // 背景色の反転
        if (flags & 0x0020) != 0 {
            color = mix(cell.fg, cell.bg, alpha);
        }
        // 不可視
        if (flags & 0x0040) != 0 {}
        // 取り消し線
        if (flags & 0x0080) != 0 {}
    }
    return color;
}

fn mix(bg: vec4<f32>, fg: vec4<f32>, alpha: f32) -> vec4<f32> {
    return bg * (1.0 - alpha) + fg * alpha;
}
