@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var<storage, read> cells: array<GpuCell>;
@group(0) @binding(3) var<uniform> grid_uniform: GridUniform;

struct GpuCell {
    glyph_index: u32,
    _pad1: u32,
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

    let row = u32(pos.y / cell_size.y);
    let col = u32(pos.x / cell_size.x);
    if grid_size.y <= row || grid_size.x <= col {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let slots_per_row = grid_uniform.slots_per_row;
    let atlas_size = grid_uniform.atlas_size;

    let index = row * grid_size.x + col;
    let cell = cells[index];
    let slot_pos = vec2<u32>(
        cell.glyph_index % slots_per_row,
        cell.glyph_index / slots_per_row,
    );
    let local_pos = vec2<u32>(pos.xy) % vec2<u32>(cell_size);

    let uv = vec2<f32>(slot_pos * vec2<u32>(cell_size) + local_pos) / atlas_size;

    let alpha = textureSample(atlas, s, uv).r;

    return mix(cell.bg, cell.fg, alpha);
}

fn mix(bg: vec4<f32>, fg: vec4<f32>, alpha: f32) -> vec4<f32> {
    return bg * (1.0 - alpha) + fg * alpha;
}
