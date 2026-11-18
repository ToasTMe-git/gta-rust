struct Globals {
    view_proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_pos: vec4f,
    sun_dir: vec4f,
    params: vec4f,
    screen: vec4f,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var menu_tex: texture_2d<f32>;
@group(0) @binding(2) var menu_samp: sampler;

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    var p = vec2f(-1.0, -1.0);
    if (vi == 1u) { p = vec2f(3.0, -1.0); }
    else if (vi == 2u) { p = vec2f(-1.0, 3.0); }
    return vec4f(p, 0.0, 1.0);
}

@fragment
fn fs_menu(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let uv = frag.xy / g.screen.xy;
    let c = textureSample(menu_tex, menu_samp, uv);
    return vec4f(c.rgb, 1.0);
}
