// Mirrors tone.rs / color.rs / detail.rs. Pixels are packed RGBA8 in u32 so
// every stage rounds to 8 bits exactly like the CPU path does.

struct Params {
    width: u32,
    height: u32,
    exposure_factor: f32,
    shadow_amount: f32,
    highlight_amount: f32,
    contrast_factor: f32,
    black_point: f32,
    range: f32,
    temp: f32,
    tint: f32,
    saturation_factor: f32,
    vibrance: f32,
    sigma: f32,
    radius: i32,
    threshold: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
@group(0) @binding(3) var<storage, read_write> tmp: array<vec4<f32>>;

fn unpack(v: u32) -> vec3<f32> {
    return vec3<f32>(f32(v & 255u), f32((v >> 8u) & 255u), f32((v >> 16u) & 255u)) / 255.0;
}

fn pack(c: vec3<f32>) -> u32 {
    let q = vec3<u32>(round(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0));
    return q.x | (q.y << 8u) | (q.z << 16u) | (255u << 24u);
}

fn luma(c: vec3<f32>) -> f32 {
    return 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
}

fn tone(pixel: vec3<f32>) -> vec3<f32> {
    var c = pixel * p.exposure_factor;
    let l = luma(c);
    let sw = pow(clamp(1.0 - l, 0.0, 1.0), 1.5);
    let hw = pow(clamp(l, 0.0, 1.0), 1.5);
    c = c + vec3<f32>(p.shadow_amount * sw * 0.5);
    c = c + vec3<f32>(p.highlight_amount * hw * 0.35);
    c = (c - vec3<f32>(0.5)) * p.contrast_factor + vec3<f32>(0.5);
    return clamp((c - vec3<f32>(p.black_point)) / p.range, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn rgb_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let mx = max(c.x, max(c.y, c.z));
    let mn = min(c.x, min(c.y, c.z));
    let l = (mx + mn) / 2.0;
    let delta = mx - mn;
    if (delta < 1.1920929e-7) {
        return vec3<f32>(0.0, 0.0, l);
    }
    var s: f32;
    if (l < 0.5) { s = delta / (mx + mn); } else { s = delta / (2.0 - mx - mn); }
    var h: f32;
    if (mx == c.x) {
        h = ((c.y - c.z) / delta + 6.0) % 6.0;
    } else if (mx == c.y) {
        h = (c.z - c.x) / delta + 2.0;
    } else {
        h = (c.x - c.y) / delta + 4.0;
    }
    return vec3<f32>(h * 60.0, s, l);
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x; let s = hsl.y; let l = hsl.z;
    if (s < 1.1920929e-7) { return vec3<f32>(l); }
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - abs(hp % 2.0 - 1.0));
    var rgb: vec3<f32>;
    switch (i32(hp)) {
        case 0: { rgb = vec3<f32>(c, x, 0.0); }
        case 1: { rgb = vec3<f32>(x, c, 0.0); }
        case 2: { rgb = vec3<f32>(0.0, c, x); }
        case 3: { rgb = vec3<f32>(0.0, x, c); }
        case 4: { rgb = vec3<f32>(x, 0.0, c); }
        default: { rgb = vec3<f32>(c, 0.0, x); }
    }
    return rgb + vec3<f32>(l - c / 2.0);
}

fn color(pixel: vec3<f32>) -> vec3<f32> {
    var c = pixel;
    c.x = c.x + p.temp * 0.12 - p.tint * 0.06;
    c.z = c.z - p.temp * 0.12 - p.tint * 0.06;
    c.y = c.y + p.tint * 0.12;
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    let hsl = rgb_to_hsl(c);
    let vib = 1.0 + p.vibrance * (1.0 - hsl.y);
    let ns = clamp(hsl.y * p.saturation_factor * vib, 0.0, 1.0);
    return hsl_to_rgb(vec3<f32>(hsl.x, ns, hsl.z));
}

@compute @workgroup_size(16, 16)
fn tone_color(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.width || id.y >= p.height) { return; }
    let i = id.y * p.width + id.x;
    // Round between stages, like the CPU path's u8 intermediate.
    let toned = unpack(pack(tone(unpack(src[i]))));
    dst[i] = pack(color(toned));
}

fn gauss(d: i32) -> f32 {
    let x = f32(d);
    return exp(-(x * x) / (2.0 * p.sigma * p.sigma));
}

@compute @workgroup_size(16, 16)
fn blur_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.width || id.y >= p.height) { return; }
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var d = -p.radius; d <= p.radius; d = d + 1) {
        let x = clamp(i32(id.x) + d, 0, i32(p.width) - 1);
        let w = gauss(d);
        acc = acc + unpack(src[id.y * p.width + u32(x)]) * w;
        wsum = wsum + w;
    }
    tmp[id.y * p.width + id.x] = vec4<f32>(acc / wsum, 1.0);
}

// Vertical blur of `tmp`, then the unsharp combine against `src`.
@compute @workgroup_size(16, 16)
fn blur_v_unsharp(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.width || id.y >= p.height) { return; }
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var d = -p.radius; d <= p.radius; d = d + 1) {
        let y = clamp(i32(id.y) + d, 0, i32(p.height) - 1);
        let w = gauss(d);
        acc = acc + tmp[u32(y) * p.width + id.x].xyz * w;
        wsum = wsum + w;
    }
    let blurred = round(acc / wsum * 255.0);
    let orig = round(unpack(src[id.y * p.width + id.x]) * 255.0);
    let diff = orig - blurred;
    // image::imageops::unsharpen: only pixels whose difference exceeds the
    // threshold are pushed away from the blur.
    let apply = vec3<f32>(abs(diff) > vec3<f32>(p.threshold));
    let out = orig + diff * apply;
    dst[id.y * p.width + id.x] = pack(out / 255.0);
}
