struct Globals {
    view_proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_pos: vec4f,
    sun_dir: vec4f,
    params: vec4f, // time, daylight, fog_density, _
    screen: vec4f, // width, height, _, _
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;

struct VOut {
    @builtin(position) clip: vec4f,
    @location(0) world: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
    @location(4) params: vec4f,
};

@vertex
fn vs_scene(
    @location(0) pos: vec3f,
    @location(1) nrm: vec3f,
    @location(2) uv: vec2f,
    @location(3) m0: vec4f,
    @location(4) m1: vec4f,
    @location(5) m2: vec4f,
    @location(6) m3: vec4f,
    @location(7) color: vec4f,
    @location(8) params: vec4f,
) -> VOut {
    let m = mat4x4f(m0, m1, m2, m3);
    let wp = m * vec4f(pos, 1.0);
    var o: VOut;
    o.clip = g.view_proj * wp;
    o.world = wp.xyz;
    o.normal = normalize((m * vec4f(nrm, 0.0)).xyz);
    o.uv = uv;
    o.color = color;
    o.params = params;
    return o;
}

@fragment
fn fs_scene(i: VOut) -> @location(0) vec4f {
    var base = i.color.rgb;
    if (i.params.z >= 0.0) {
        let t = textureSample(atlas, samp, i.uv * i.params.xy, i32(i.params.z));
        base = base * t.rgb;
    }
    let n = normalize(i.normal);
    let sun = max(dot(n, normalize(g.sun_dir.xyz)), 0.0);
    let day = g.params.y;
    var col = base * (0.22 + day * 0.26 + sun * (0.28 + day * 0.75));
    if (i.params.w > 0.5) { col = base; }
    let d = length(i.world - g.camera_pos.xyz);
    let fog = 1.0 - exp(-pow(d * g.params.z, 2.0));
    let skyc = mix(vec3f(0.02, 0.03, 0.08), vec3f(0.55, 0.72, 0.9), day);
    col = mix(col, skyc, clamp(fog, 0.0, 0.94));
    return vec4f(col, 1.0);
}

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    var p = vec2f(-1.0, -1.0);
    if (vi == 1u) { p = vec2f(3.0, -1.0); }
    else if (vi == 2u) { p = vec2f(-1.0, 3.0); }
    return vec4f(p, 0.0, 1.0);
}

@fragment
fn fs_sky(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let uv = frag.xy / g.screen.xy;
    let ndc = vec2f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let p = g.inv_view_proj * vec4f(ndc, 1.0, 1.0);
    let dir = normalize(p.xyz / p.w - g.camera_pos.xyz);
    let day = g.params.y;
    let h = dir.y;
    let top = mix(vec3f(0.01, 0.02, 0.06), vec3f(0.12, 0.44, 0.86), day);
    let hor = mix(vec3f(0.03, 0.04, 0.10), vec3f(0.72, 0.86, 0.96), day);
    var col = mix(hor, top, smoothstep(0.0, 0.55, h));
    if (h < 0.0) {
        col = mix(hor, vec3f(0.08, 0.08, 0.10), clamp(-h * 2.5, 0.0, 1.0));
    }
    let sd = max(dot(dir, normalize(g.sun_dir.xyz)), 0.0);
    col += vec3f(1.0, 0.82, 0.5) * pow(sd, 320.0) * 3.0 * day;
    col += vec3f(1.0, 0.6, 0.3) * pow(sd, 12.0) * 0.35 * day;
    let n = floor(dir.xz * 420.0 + dir.y * 420.0);
    let star = step(0.9975, fract(sin(dot(n, vec2f(12.9898, 78.233))) * 43758.5453));
    col += vec3f(star) * (1.0 - day) * clamp(h, 0.0, 1.0) * 0.9;
    return vec4f(col, 1.0);
}
