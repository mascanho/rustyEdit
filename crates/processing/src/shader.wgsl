// The whole develop pipeline as compute kernels. Each kernel mirrors a CPU
// module (named in its comment); `lib.rs` has the GPU/CPU parity tests.
//
// Images between stages are RGB in 16-bit floats (two u32 words per
// pixel, see `px_*` / `store_*`), on a 0..1 scale, so edits never
// quantise to 8 bits until the very end. Up to and including noise
// reduction, values may exceed 1 (highlight headroom from RAW files and
// lens corrections), which later exposure and tone edits can pull back.
//
// Every kernel shares one bind group layout; a pass binds a small dummy
// buffer to the slots it doesn't use.
//
// Positions: a pass may work on a region of the full image (the zoomed-in
// detail view). `off_x/off_y` is where this pass's output starts within
// its full image of `full_w x full_h`, and `src_off_x/src_off_y` the same
// for its input. Whole-image renders leave them at 0 / the image size.

struct Params {
    // Output size of this pass, and the size of the buffer it reads.
    width: u32, height: u32, src_width: u32, src_height: u32,
    // Load: orientation and lens corrections (lens.rs, geometry.rs).
    flip_h: u32, flip_v: u32, quarters: u32, lens_on: u32,
    lens_k: f32, lens_ca: f32, lens_vig: f32, lens_norm2: f32,
    lens_cx: f32, lens_cy: f32, dn_luma: f32, dn_color: f32,
    // Noise reduction (denoise.rs) and the finishing flags (F_*).
    dn_luma_sigma_s: f32, dn_luma_sigma_r: f32, dn_color_sigma_r: f32, flags: u32,
    // Tone and colour (tone.rs, color.rs).
    exposure_factor: f32, shadow_amount: f32, highlight_amount: f32, contrast_factor: f32,
    black_point: f32, range: f32, temp: f32, tint: f32,
    saturation_factor: f32, vibrance: f32, threshold: f32, blur_radius: i32,
    // Blur kernel weights (in `aux`) and sharpening (sharpen.rs).
    blur_offset: u32, sharpen_strength: f32, sharpen_threshold: f32, sharpen_masking: u32,
    // Finishing (finishing/*): offsets of each tool's tables in `aux`.
    curves_offset: u32, mixer_offset: u32, grading_offset: u32, gradient_offset: u32,
    gradient_count: u32, dehaze: f32, vignette: f32, vig_cx: f32,
    vig_cy: f32, vig_inner: f32, vig_outer: f32, grain_strength: f32,
    // Crop (crop.rs).
    grain_cell: u32, crop_x0: f32, crop_y0: f32, crop_sin: f32,
    crop_cos: f32, crop_rotate: u32, crop_left: u32, crop_top: u32,
    // Output and resizing.
    clip_shadows: u32, clip_highlights: u32, hist_stride: u32, resize_filter: u32,
    // Source: its format (SRC_*), whether it holds linear camera RGB from
    // a RAW file (developed with the table at `raw_offset`), and the 3D
    // colour lookup table applied on output (`lut_size` 0: none).
    source_format: u32, source_linear: u32, raw_offset: u32, lut_offset: u32,
    lut_size: u32, off_x: u32, off_y: u32, full_w: u32,
    // Spot removal: the spot's table in `aux` (see gpu.rs `plan_spots`).
    full_h: u32, src_off_x: u32, src_off_y: u32, spot_offset: u32,
    // Perspective correction (Crop::perspective_strength).
    persp_x: f32, persp_y: f32, vig_round: f32, lb_radius: f32,
    // Lens blur (lens_blur.rs).
    lb_boost: f32, lb_falloff: f32, lb_shape: u32, lb_invert: u32,
    lb_focus: u32, lb_table: u32, pad3: f32, pad4: f32,
}

const F_CURVES: u32 = 1u;
const F_MIXER: u32 = 2u;
const F_GRADING: u32 = 4u;
const F_DEHAZE: u32 = 8u;
const F_LOCAL: u32 = 16u;
const F_VIGNETTE: u32 = 32u;
const F_GRAIN: u32 = 64u;
const F_WIDE: u32 = 128u;
const F_NARROW: u32 = 256u;

// Source formats.
const SRC_RGB8: u32 = 0u;
const SRC_RGB16: u32 = 1u;
const SRC_HALF: u32 = 2u;

// Floats per gradient layer in `aux` (see gpu.rs `gradient_floats`).
const GRADIENT_STRIDE: u32 = 232u;
const DAB_FLOATS: u32 = 40u;
// Entries per channel of the tone curve tables.
const CURVE_STEPS: u32 = 1024u;

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;
@group(0) @binding(3) var<storage, read_write> tmp: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> tmp2: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> aux: array<f32>;
@group(0) @binding(6) var<storage, read> img_a: array<u32>;
@group(0) @binding(7) var<storage, read> img_b: array<u32>;
@group(0) @binding(8) var<storage, read_write> hist: array<atomic<u32>>;

// ---------------------------------------------------------------- helpers

fn half3(lo: u32, hi: u32) -> vec3<f32> {
    return vec3<f32>(unpack2x16float(lo), unpack2x16float(hi).x);
}

/// Pixel `i` of the half-float image in `src` / `img_a` / `img_b`.
fn px_src(i: u32) -> vec3<f32> {
    return half3(src[2u * i], src[2u * i + 1u]);
}

fn px_a(i: u32) -> vec3<f32> {
    return half3(img_a[2u * i], img_a[2u * i + 1u]);
}

fn px_b(i: u32) -> vec3<f32> {
    return half3(img_b[2u * i], img_b[2u * i + 1u]);
}

/// Stores pixel `i` of `dst`, kept non-negative but not capped at 1.
fn store_hdr(i: u32, c: vec3<f32>) {
    let v = clamp(c, vec3<f32>(0.0), vec3<f32>(60000.0));
    dst[2u * i] = pack2x16float(v.xy);
    dst[2u * i + 1u] = pack2x16float(vec2<f32>(v.z, 0.0));
}

/// Stores pixel `i` of `dst`, clamped to 0..1 as the 8-bit CPU stages
/// clamp.
fn store(i: u32, c: vec3<f32>) {
    store_hdr(i, clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)));
}

fn luma(c: vec3<f32>) -> f32 {
    return 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
}

/// `x^y` for `x >= 0`, with `0^y = 0` guaranteed.
fn spow(x: f32, y: f32) -> f32 {
    if (x <= 0.0) { return 0.0; }
    return pow(x, y);
}

/// finishing::smoothstep: also defined when edge0 > edge1.
fn smoothstep2(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn rem_euclid(x: f32, m: f32) -> f32 {
    let r = x % m;
    if (r < 0.0) { return r + m; }
    return r;
}

fn in_bounds(id: vec3<u32>) -> bool {
    return id.x < p.width && id.y < p.height;
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

/// sRGB's transfer curve (the working space uses it), continued above 1
/// so highlight headroom survives encoding.
fn encode_srgb(v: f32) -> f32 {
    if (v <= 0.0031308) { return max(v, 0.0) * 12.92; }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

// ------------------------------------------------- the source image

fn src_u16(i: u32) -> u32 {
    let word = src[i >> 1u];
    if ((i & 1u) == 1u) { return word >> 16u; }
    return word & 0xffffu;
}

/// Source pixel (`x`, `y`) on a 0..1 scale, whatever its format.
fn read_src(x: u32, y: u32) -> vec3<f32> {
    let i = y * p.src_width + x;
    switch (p.source_format) {
        case 0u: {
            let b = i * 3u;
            return vec3<f32>(
                f32((src[b >> 2u] >> ((b & 3u) * 8u)) & 255u),
                f32((src[(b + 1u) >> 2u] >> (((b + 1u) & 3u) * 8u)) & 255u),
                f32((src[(b + 2u) >> 2u] >> (((b + 2u) & 3u) * 8u)) & 255u),
            ) / 255.0;
        }
        case 1u: {
            let b = i * 3u;
            return vec3<f32>(f32(src_u16(b)), f32(src_u16(b + 1u)), f32(src_u16(b + 2u))) / 65535.0;
        }
        default: {
            return px_src(i);
        }
    }
}

// ------------------------------------------------- RAW: demosaic

// The demosaic table in `aux` at `raw_offset` (see gpu.rs `RawTable`).
const RAW_BLACK: u32 = 0u;     // 4: per 2x2 position
const RAW_WHITE: u32 = 4u;     // 4
const RAW_WB: u32 = 8u;        // 3: as-shot white balance, green = 1
const RAW_PW: u32 = 11u;       // CFA pattern width
const RAW_PH: u32 = 12u;       // and height
const RAW_CFA: u32 = 13u;      // 36: colour of each pattern cell (0 R, 1 G, 2 B)
const RAW_CROP: u32 = 49u;     // 4: x, y, width, height of the picture area
const RAW_ORIENT: u32 = 53u;   // 3: transpose, flip x, flip y
// Developing (load): camera RGB -> working space.
const RAW_MATRIX: u32 = 56u;   // 9: camera RGB -> linear working RGB, rows
const RAW_CLIP: u32 = 65u;     // 3: where each channel clips (= RAW_WB)
const RAW_GAIN: u32 = 68u;     // 3: the user's white balance and exposure

fn raw(k: u32) -> f32 {
    return aux[p.raw_offset + k];
}

fn mirror(v: i32, n: i32) -> i32 {
    var m = v;
    if (m < 0) { m = -m; }
    if (m >= n) { m = 2 * n - 2 - m; }
    return clamp(m, 0, n - 1);
}

fn cfa_color(x: i32, y: i32) -> u32 {
    let pw = u32(raw(RAW_PW));
    let ph = u32(raw(RAW_PH));
    return u32(raw(RAW_CFA + (u32(y) % ph) * pw + u32(x) % pw));
}

/// Mosaic sample at sensor position (`x`, `y`), mirrored at the edges (so
/// the CFA colour is preserved), black-subtracted, scaled so the sensor's
/// white is 1, and white balanced.
fn mosaic(x: i32, y: i32) -> f32 {
    let mx = mirror(x, i32(p.src_width));
    let my = mirror(y, i32(p.src_height));
    let v = f32(src_u16(u32(my) * p.src_width + u32(mx)));
    let pos = u32(my % 2) * 2u + u32(mx % 2);
    let black = raw(RAW_BLACK + pos);
    let white = raw(RAW_WHITE + pos);
    return max(v - black, 0.0) / max(white - black, 1.0) * raw(RAW_WB + cfa_color(mx, my));
}

/// Malvar-He-Cutler demosaicing for 2x2 RGB Bayer patterns: bilinear
/// interpolation corrected by the Laplacian of the known channel.
fn demosaic_bayer(x: i32, y: i32) -> vec3<f32> {
    let c0 = cfa_color(x, y);
    let p00 = mosaic(x, y);
    let n1 = mosaic(x, y - 1) + mosaic(x, y + 1);
    let w1 = mosaic(x - 1, y) + mosaic(x + 1, y);
    let n2 = mosaic(x, y - 2) + mosaic(x, y + 2);
    let w2 = mosaic(x - 2, y) + mosaic(x + 2, y);
    let diag = mosaic(x - 1, y - 1) + mosaic(x + 1, y - 1) + mosaic(x - 1, y + 1) + mosaic(x + 1, y + 1);
    var out = vec3<f32>(0.0);
    if (c0 == 1u) {
        // Green: the colour beside it in the row, and the one above/below.
        let row = (5.0 * p00 + 4.0 * w1 - w2 - diag + 0.5 * n2) / 8.0;
        let col = (5.0 * p00 + 4.0 * n1 - n2 - diag + 0.5 * w2) / 8.0;
        out.y = p00;
        let row_color = cfa_color(x + 1, y);
        if (row_color == 0u) { out.x = row; out.z = col; } else { out.z = row; out.x = col; }
    } else {
        let g = (4.0 * p00 + 2.0 * (n1 + w1) - (n2 + w2)) / 8.0;
        let other = (6.0 * p00 + 2.0 * diag - 1.5 * (n2 + w2)) / 8.0;
        out.y = g;
        if (c0 == 0u) { out.x = p00; out.z = other; } else { out.z = p00; out.x = other; }
    }
    return max(out, vec3<f32>(0.0));
}

/// Any other CFA layout (X-Trans, ...): each missing channel is a
/// distance-weighted average of its samples in the 5x5 around the pixel.
fn demosaic_generic(x: i32, y: i32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    var weight = vec3<f32>(0.0);
    for (var dy = -2; dy <= 2; dy++) {
        for (var dx = -2; dx <= 2; dx++) {
            let c = cfa_color(mirror(x + dx, i32(p.src_width)), mirror(y + dy, i32(p.src_height)));
            let w = 1.0 / f32(1 + dx * dx + dy * dy);
            let v = mosaic(x + dx, y + dy);
            if (c == 0u) { sum.x += v * w; weight.x += w; }
            else if (c == 1u) { sum.y += v * w; weight.y += w; }
            else { sum.z += v * w; weight.z += w; }
        }
    }
    var out = sum / max(weight, vec3<f32>(1e-6));
    let c0 = cfa_color(x, y);
    let own = mosaic(x, y);
    if (c0 == 0u) { out.x = own; } else if (c0 == 1u) { out.y = own; } else { out.z = own; }
    return out;
}

// Sensor mosaic (`src`, u16) -> linear, white-balanced camera RGB (half
// floats), cropped to the picture area and turned upright.
@compute @workgroup_size(16, 16)
fn demosaic(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    var u = id.x;
    var v = id.y;
    if (raw(RAW_ORIENT) > 0.5) { u = id.y; v = id.x; }
    let cw = u32(raw(RAW_CROP + 2u));
    let ch = u32(raw(RAW_CROP + 3u));
    if (raw(RAW_ORIENT + 1u) > 0.5) { u = cw - 1u - u; }
    if (raw(RAW_ORIENT + 2u) > 0.5) { v = ch - 1u - v; }
    let x = i32(u32(raw(RAW_CROP)) + u);
    let y = i32(u32(raw(RAW_CROP + 1u)) + v);
    var rgb: vec3<f32>;
    if (u32(raw(RAW_PW)) == 2u && u32(raw(RAW_PH)) == 2u) {
        rgb = demosaic_bayer(x, y);
    } else {
        rgb = demosaic_generic(x, y);
    }
    store_hdr(id.y * p.width + id.x, rgb);
}

/// Develops linear camera RGB into the working space: rebuilds clipped
/// highlights, applies the user's white balance and exposure as a camera
/// would (gains on the sensor data), converts with the camera's colour
/// matrix and encodes with the working space's curve. Values above 1
/// are kept for later recovery.
fn develop_raw(camera: vec3<f32>) -> vec3<f32> {
    var c = camera;
    // Highlight reconstruction: a channel at its clipping point only says
    // "at least this bright", so it's raised towards the brightest channel,
    // turning blown areas neutral instead of magenta and keeping their
    // brightness above 1.
    let clip = vec3<f32>(raw(RAW_CLIP), raw(RAW_CLIP + 1u), raw(RAW_CLIP + 2u));
    let near = smoothstep(vec3<f32>(0.92), vec3<f32>(1.0), c / clip);
    let brightest = max(c.x, max(c.y, c.z));
    c = mix(c, max(c, vec3<f32>(brightest)), near);
    c *= vec3<f32>(raw(RAW_GAIN), raw(RAW_GAIN + 1u), raw(RAW_GAIN + 2u));
    let m = p.raw_offset + RAW_MATRIX;
    let lin = vec3<f32>(
        aux[m] * c.x + aux[m + 1u] * c.y + aux[m + 2u] * c.z,
        aux[m + 3u] * c.x + aux[m + 4u] * c.y + aux[m + 5u] * c.z,
        aux[m + 6u] * c.x + aux[m + 7u] * c.y + aux[m + 8u] * c.z,
    );
    return vec3<f32>(encode_srgb(lin.x), encode_srgb(lin.y), encode_srgb(lin.z));
}

// ------------------------------------------------- spot removal (retouch.rs)

// The spot's table: target x, y and source x, y (pixels), radius
// (pixels), feather, opacity, and 1 for heal / 0 for clone.
fn spot(k: u32) -> f32 {
    return aux[p.spot_offset + k];
}

/// retouch.rs `sample`: the pixel at (`x`, `y`), clamped to the photo.
fn spot_sample(x: f32, y: f32) -> vec3<f32> {
    let px = min(u32(max(floor(x), 0.0)), p.src_width - 1u);
    let py = min(u32(max(floor(y), 0.0)), p.src_height - 1u);
    return px_src(py * p.src_width + px);
}

/// retouch.rs `average`: 3x3 around (`x`, `y`).
fn spot_average(x: f32, y: f32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            sum += spot_sample(x + f32(ox), y + f32(oy));
        }
    }
    return sum / 9.0;
}

const SPOT_RING: u32 = 32u;
const TAU: f32 = 6.28318530718;

// retouch.rs `EdgeDifference::measure`: how the target's surroundings
// differ from the source's, at 32 points round the circle, into `tmp`.
@compute @workgroup_size(32)
fn spot_ring(@builtin(local_invocation_index) i: u32) {
    let r = spot(4u) * 1.08 + 1.0;
    let angle = f32(i) / f32(SPOT_RING) * TAU;
    let dir = vec2<f32>(cos(angle), sin(angle)) * r;
    let t = spot_average(spot(0u) + dir.x, spot(1u) + dir.y);
    let s = spot_average(spot(2u) + dir.x, spot(3u) + dir.y);
    tmp[i] = vec4<f32>(t - s, 0.0);
}

// retouch.rs `apply_spot` over the spot's bounding box (`off_x/off_y`,
// `width x height`): `src` is the photo before this spot, `dst` a copy
// of it to patch.
@compute @workgroup_size(16, 16)
fn spot_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let x = p.off_x + id.x;
    let y = p.off_y + id.y;
    let i = y * p.src_width + x;
    let dx = f32(x) + 0.5 - spot(0u);
    let dy = f32(y) + 0.5 - spot(1u);
    let d = sqrt(dx * dx + dy * dy) / spot(4u);
    if (d >= 1.0) { return; }
    let inner = 1.0 - clamp(spot(5u), 0.0, 1.0);
    var mask = 1.0;
    if (d > inner) {
        let t = clamp((d - inner) / max(1.0 - inner, 1e-3), 0.0, 1.0);
        mask = 1.0 - t * t * (3.0 - 2.0 * t);
    }
    let weight = mask * clamp(spot(6u), 0.0, 1.0);
    if (weight <= 0.0) { return; }
    var patched = spot_sample(f32(x) + 0.5 + spot(2u) - spot(0u), f32(y) + 0.5 + spot(3u) - spot(1u));
    if (spot(7u) > 0.5) {
        var mean = vec3<f32>(0.0);
        for (var k = 0u; k < SPOT_RING; k++) {
            mean += tmp[k].xyz;
        }
        mean /= f32(SPOT_RING);
        let position = rem_euclid(atan2(dy, dx), TAU) / TAU * f32(SPOT_RING);
        let a = u32(floor(position)) % SPOT_RING;
        let b = (a + 1u) % SPOT_RING;
        let edge = mix(tmp[a].xyz, tmp[b].xyz, fract(position));
        patched += mean + (edge - mean) * d * d;
    }
    let orig = px_src(i);
    store_hdr(i, orig + (patched - orig) * weight);
}

// ------------------------------------------------- load (lens, geometry)

/// lens.rs `sample`: bilinear, clamped to the photo.
fn lens_sample(fx0: f32, fy0: f32) -> vec3<f32> {
    let w = p.src_width;
    let h = p.src_height;
    let fx = clamp(fx0, 0.0, f32(w - 1u));
    let fy = clamp(fy0, 0.0, f32(h - 1u));
    let x0 = u32(floor(fx));
    let y0 = u32(floor(fy));
    let x1 = min(x0 + 1u, w - 1u);
    let y1 = min(y0 + 1u, h - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let top = read_src(x0, y0) * (1.0 - tx) + read_src(x1, y0) * tx;
    let bottom = read_src(x0, y1) * (1.0 - tx) + read_src(x1, y1) * tx;
    return top * (1.0 - ty) + bottom * ty;
}

/// The source pixel shown at (`ox`, `oy`) of the flipped and rotated
/// photo (image's `fliph`/`flipv`, then `rotate90/180/270`).
fn oriented_source(ox: u32, oy: u32) -> vec2<u32> {
    let w = p.src_width;
    let h = p.src_height;
    var x: u32;
    var y: u32;
    switch (p.quarters) {
        case 1u: { x = oy; y = h - 1u - ox; }
        case 2u: { x = w - 1u - ox; y = h - 1u - oy; }
        case 3u: { x = w - 1u - oy; y = ox; }
        default: { x = ox; y = oy; }
    }
    if (p.flip_h != 0u) { x = w - 1u - x; }
    if (p.flip_v != 0u) { y = h - 1u - y; }
    return vec2<u32>(x, y);
}

// Source -> oriented, lens-corrected image (developed first if RAW).
@compute @workgroup_size(16, 16)
fn load(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let s = oriented_source(p.off_x + id.x, p.off_y + id.y);
    var c: vec3<f32>;
    if (p.lens_on != 0u) {
        let dx = f32(s.x) - p.lens_cx;
        let dy = f32(s.y) - p.lens_cy;
        let r2 = (dx * dx + dy * dy) / p.lens_norm2;
        let scale = 1.0 - p.lens_k * r2;
        let gain = max(1.0 + p.lens_vig * r2, 0.0);
        let f = vec3<f32>(scale * (1.0 + p.lens_ca), scale, scale * (1.0 - p.lens_ca));
        c = vec3<f32>(
            lens_sample(p.lens_cx + dx * f.x, p.lens_cy + dy * f.x).x,
            lens_sample(p.lens_cx + dx * f.y, p.lens_cy + dy * f.y).y,
            lens_sample(p.lens_cx + dx * f.z, p.lens_cy + dy * f.z).z,
        );
        if (p.source_linear != 0u) {
            c = develop_raw(c);
        }
        c *= gain;
    } else {
        c = read_src(s.x, s.y);
        if (p.source_linear != 0u) {
            c = develop_raw(c);
        }
    }
    store_hdr(id.y * p.width + id.x, c);
}

// ------------------------------------------------- defringe

// defringe.rs: purple / green desaturated next to strong edges. The two
// amounts arrive in `dn_luma` (purple) and `dn_color` (green).
fn defringe_band(hue: f32, lo: f32, hi: f32) -> f32 {
    return smoothstep2(lo - 20.0, lo, hue) * (1.0 - smoothstep2(hi, hi + 20.0, hue));
}

@compute @workgroup_size(16, 16)
fn defringe(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let w = i32(p.width);
    let h = i32(p.height);
    let i = id.y * p.width + id.x;
    let c = px_src(i);
    var lo = 1.0e9;
    var hi = -1.0e9;
    for (var j = -2; j <= 2; j++) {
        let sy = clamp(i32(id.y) + j, 0, h - 1);
        for (var k = -2; k <= 2; k++) {
            let sx = clamp(i32(id.x) + k, 0, w - 1);
            let t = px_src(u32(sy * w + sx));
            let l = 0.299 * t.x + 0.587 * t.y + 0.114 * t.z;
            lo = min(lo, l);
            hi = max(hi, l);
        }
    }
    let near_edge = smoothstep2(0.10, 0.30, hi - lo);
    let mx = max(c.x, max(c.y, c.z));
    let mn = min(c.x, min(c.y, c.z));
    let d = mx - mn;
    if (near_edge <= 0.0 || d <= 0.0 || mx <= 0.0) {
        store_hdr(i, c);
        return;
    }
    var hue: f32;
    if (mx == c.x) {
        hue = 60.0 * rem_euclid((c.y - c.z) / d, 6.0);
    } else if (mx == c.y) {
        hue = 60.0 * ((c.z - c.x) / d + 2.0);
    } else {
        hue = 60.0 * ((c.x - c.y) / d + 4.0);
    }
    let colour = p.dn_luma * defringe_band(hue, 255.0, 335.0)
        + p.dn_color * defringe_band(hue, 70.0, 160.0);
    let amount = min(colour * near_edge * smoothstep2(0.12, 0.35, d / mx), 1.0);
    let grey = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
    store_hdr(i, c + (vec3<f32>(grey) - c) * amount);
}

// ------------------------------------------------- noise reduction

// Image -> (Y, Cb, Cr) planes in `tmp`, on denoise.rs's 0..255 scale.
@compute @workgroup_size(16, 16)
fn denoise_prep(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    let c = px_src(i) * 255.0;
    let l = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
    tmp[i] = vec4<f32>(l, (c.z - l) * 0.564, (c.x - l) * 0.713, 0.0);
}

/// denoise.rs `range` table entry for guide difference `diff`.
fn range_weight(diff: f32, sigma_r: f32) -> f32 {
    let d = min(abs(diff), 255.0);
    return exp(-(d * d) / (2.0 * sigma_r * sigma_r));
}

// Bilateral filters of the planes in `tmp` -> `tmp2`: luminance guided
// by itself, chroma (spaced taps) guided by the luminance.
@compute @workgroup_size(16, 16)
fn denoise_filter(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let w = i32(p.width);
    let h = i32(p.height);
    let x = i32(id.x);
    let y = i32(id.y);
    let i = id.y * p.width + id.x;
    let centre = tmp[i];
    var out = centre;
    if (p.dn_luma > 0.0) {
        let ss = p.dn_luma_sigma_s;
        var sum = 0.0;
        var weight = 0.0;
        for (var j = -2; j <= 2; j++) {
            let sy = clamp(y + j, 0, h - 1);
            for (var k = -2; k <= 2; k++) {
                let sx = clamp(x + k, 0, w - 1);
                let g = tmp[u32(sy * w + sx)].x;
                let spatial = exp(-f32(j * j + k * k) / (2.0 * ss * ss));
                let wt = spatial * range_weight(g - centre.x, p.dn_luma_sigma_r);
                sum += g * wt;
                weight += wt;
            }
        }
        out.x = sum / max(weight, 1e-6);
    }
    if (p.dn_color > 0.0) {
        var sum = vec2<f32>(0.0);
        var weight = 0.0;
        for (var j = -2; j <= 2; j++) {
            let sy = clamp(y + j * 3, 0, h - 1);
            for (var k = -2; k <= 2; k++) {
                let sx = clamp(x + k * 3, 0, w - 1);
                let tap = tmp[u32(sy * w + sx)];
                let spatial = exp(-f32(j * j + k * k) / (2.0 * 4.0 * 4.0));
                let wt = spatial * range_weight(tap.x - centre.x, p.dn_color_sigma_r);
                sum += tap.yz * wt;
                weight += wt;
            }
        }
        let filtered = sum / max(weight, 1e-6);
        out.y = filtered.x;
        out.z = filtered.y;
    }
    tmp2[i] = out;
}

// Blends the filtered planes back in and converts to RGB.
@compute @workgroup_size(16, 16)
fn denoise_combine(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    let o = tmp[i];
    let f = tmp2[i];
    let la = sqrt(p.dn_luma);
    let ca = sqrt(p.dn_color);
    let l = o.x + (f.x - o.x) * la;
    let b_c = o.y + (f.y - o.y) * ca;
    let r_c = o.z + (f.z - o.z) * ca;
    let r = l + r_c / 0.713;
    let b = l + b_c / 0.564;
    let g = (l - 0.299 * r - 0.114 * b) / 0.587;
    store_hdr(i, vec3<f32>(r, g, b) / 255.0);
}

// ------------------------------------------------- tone and colour

fn tone(pixel: vec3<f32>) -> vec3<f32> {
    var c = pixel * p.exposure_factor;
    let l = luma(c);
    let sw = spow(clamp(1.0 - l, 0.0, 1.0), 1.5);
    let hw = spow(clamp(l, 0.0, 1.0), 1.5);
    c = c + vec3<f32>(p.shadow_amount * sw * 0.5);
    c = c + vec3<f32>(p.highlight_amount * hw * 0.35);
    c = (c - vec3<f32>(0.5)) * p.contrast_factor + vec3<f32>(0.5);
    return clamp((c - vec3<f32>(p.black_point)) / p.range, vec3<f32>(0.0), vec3<f32>(1.0));
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
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    store(i, color(tone(px_src(i))));
}

// ------------------------------------------------- blurs and sharpening

// image::imageops::blur, horizontal half: image -> `tmp`, with the
// kernel's weights in `aux` and edges clamped. Taps are summed into two
// accumulators, so consecutive multiply-adds don't wait on each other.
@compute @workgroup_size(16, 16)
fn blur_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let r = p.blur_radius;
    let last = i32(p.width) - 1;
    let row = id.y * p.width;
    let x0 = i32(id.x);
    var a = px_src(row + u32(clamp(x0 + r, 0, last))) * aux[p.blur_offset + u32(2 * r)];
    var b = vec3<f32>(0.0);
    for (var d = -r; d < r; d += 2) {
        a += px_src(row + u32(clamp(x0 + d, 0, last))) * aux[p.blur_offset + u32(d + r)];
        b += px_src(row + u32(clamp(x0 + d + 1, 0, last))) * aux[p.blur_offset + u32(d + r + 1)];
    }
    tmp[row + id.x] = vec4<f32>(a + b, 0.0);
}

// Vertical half: `tmp` -> image.
@compute @workgroup_size(16, 16)
fn blur_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let r = p.blur_radius;
    let last = i32(p.height) - 1;
    let y0 = i32(id.y);
    var a = tmp[u32(clamp(y0 + r, 0, last)) * p.width + id.x].xyz * aux[p.blur_offset + u32(2 * r)];
    var b = vec3<f32>(0.0);
    for (var d = -r; d < r; d += 2) {
        a += tmp[u32(clamp(y0 + d, 0, last)) * p.width + id.x].xyz * aux[p.blur_offset + u32(d + r)];
        b += tmp[u32(clamp(y0 + d + 1, 0, last)) * p.width + id.x].xyz * aux[p.blur_offset + u32(d + r + 1)];
    }
    store_hdr(id.y * p.width + id.x, a + b);
}

// image::imageops::unsharpen's combine: `src` against its blur `img_a`,
// with the threshold in 8-bit levels.
@compute @workgroup_size(16, 16)
fn unsharp(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    let orig = px_src(i);
    let diff = orig - px_a(i);
    let apply = vec3<f32>(abs(diff) * 255.0 > vec3<f32>(p.threshold));
    store(i, orig + diff * apply);
}

fn luma601_at(x: i32, y: i32) -> f32 {
    let cx = clamp(x, 0, i32(p.width) - 1);
    let cy = clamp(y, 0, i32(p.height) - 1);
    let c = px_src(u32(cy) * p.width + u32(cx)) * 255.0;
    return 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
}

// sharpen.rs: `src` pushed away from its blur `img_a`, held back from
// flat areas by an edge mask when masking is on.
@compute @workgroup_size(16, 16)
fn sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    var m = p.sharpen_strength;
    if (p.sharpen_masking != 0u) {
        let x = i32(id.x);
        let y = i32(id.y);
        let gx = luma601_at(x + 1, y) - luma601_at(x - 1, y);
        let gy = luma601_at(x, y + 1) - luma601_at(x, y - 1);
        let magnitude = sqrt(gx * gx + gy * gy);
        let thr = p.sharpen_threshold;
        let t = clamp((magnitude - thr * 0.5) / (thr * 1.5 + 4.0), 0.0, 1.0);
        m = t * t * (3.0 - 2.0 * t) * p.sharpen_strength;
    }
    let v = px_src(i);
    // Negative (Clarity's softening) pulls towards the blur.
    store(i, v + (v - px_a(i)) * m);
}

// ------------------------------------------------- finishing

/// The composed tone curve for channel `ch`, linearly interpolated.
fn curve(ch: u32, v: f32) -> f32 {
    let x = clamp(v, 0.0, 1.0) * f32(CURVE_STEPS - 1u);
    let i0 = u32(floor(x));
    let i1 = min(i0 + 1u, CURVE_STEPS - 1u);
    let base = p.curves_offset + ch * CURVE_STEPS;
    return mix(aux[base + i0], aux[base + i1], x - f32(i0));
}

/// finishing/mixer.rs. Per band in `aux`: centre hue, span above, span
/// below, hue, saturation and luminance adjustments.
fn mixer(c: vec3<f32>) -> vec3<f32> {
    let hsl = rgb_to_hsl(c);
    if (hsl.y < 0.02) { return c; }
    var dh = 0.0;
    var ds = 0.0;
    var dl = 0.0;
    for (var band = 0u; band < 8u; band++) {
        let b = p.mixer_offset + band * 6u;
        let d = ((hsl.x - aux[b] + 540.0) % 360.0) - 180.0;
        var span: f32;
        if (d >= 0.0) { span = aux[b + 1u]; } else { span = aux[b + 2u]; }
        let w = clamp(1.0 - abs(d) / max(span, 1.0), 0.0, 1.0);
        if (w <= 0.0) { continue; }
        dh += w * aux[b + 3u] / 100.0 * 30.0;
        ds += w * aux[b + 4u] / 100.0;
        dl += w * aux[b + 5u] / 100.0;
    }
    let h = rem_euclid(hsl.x + dh, 360.0);
    let s = clamp(hsl.y * (1.0 + ds), 0.0, 1.0);
    let l = clamp(hsl.z + dl * 0.25 * hsl.y, 0.0, 1.0);
    return hsl_to_rgb(vec3<f32>(h, s, l));
}

/// finishing/grading.rs, with its tints and pivot at `aux[at..at + 10]`.
fn grade(c: vec3<f32>, at: u32) -> vec3<f32> {
    let l = luma(c);
    let pivot = aux[at + 9u];
    let sw = smoothstep2(pivot, 0.0, l);
    let hw = smoothstep2(pivot, 1.0, l);
    let mw = max(1.0 - sw - hw, 0.0);
    let shadows = vec3<f32>(aux[at], aux[at + 1u], aux[at + 2u]);
    let mids = vec3<f32>(aux[at + 3u], aux[at + 4u], aux[at + 5u]);
    let highs = vec3<f32>(aux[at + 6u], aux[at + 7u], aux[at + 8u]);
    return c + sw * shadows + mw * mids + hw * highs;
}

/// finishing/effects.rs Dehaze, `a` already divided by 100.
fn dehaze(c: vec3<f32>, a: f32) -> vec3<f32> {
    if (a > 0.0) {
        let dark = min(c.x, min(c.y, c.z));
        let transmission = max(1.0 - a * 0.7 * dark, 0.35);
        return (c - vec3<f32>(1.0)) / transmission + vec3<f32>(1.0);
    }
    return c + (vec3<f32>(0.9) - c) * -a * 0.5;
}

/// Gradient::mask_at for the layer at `g` in `aux`, at a position of the
/// full image.
fn mask_at(g: u32, x: f32, y: f32) -> f32 {
    let width = f32(p.full_w);
    let height = f32(p.full_h);
    var strength: f32;
    if (aux[g] > 2.5) {
        // Range: everywhere (the ranges narrow it).
        strength = 1.0;
    } else if (aux[g] > 1.5) {
        // Brush: the union of the dabs, erasers taking it away.
        let long = max(width, height);
        let inner = 1.0 - clamp(aux[g + 7u], 0.0, 1.0);
        let count = u32(aux[g + 31u]);
        strength = 0.0;
        for (var n = 0u; n < count; n++) {
            let at = g + DAB_FLOATS + n * 4u;
            let r = max(aux[at + 2u] * long, 1.0);
            let ex = x - aux[at] * width;
            let ey = y - aux[at + 1u] * height;
            let d = sqrt(ex * ex + ey * ey) / r;
            var f: f32;
            if (d <= inner) {
                f = 1.0;
            } else if (d >= 1.0) {
                f = 0.0;
            } else {
                let t = (d - inner) / max(1.0 - inner, 1e-3);
                f = 1.0 - t * t * (3.0 - 2.0 * t);
            }
            if (aux[at + 3u] > 0.5) {
                strength = strength * (1.0 - f);
            } else {
                strength = strength + f - strength * f;
            }
        }
    } else if (aux[g] < 0.5) {
        let ax = aux[g + 1u] * width;
        let ay = aux[g + 2u] * height;
        let dx = aux[g + 3u] * width - ax;
        let dy = aux[g + 4u] * height - ay;
        let length = max(dx * dx + dy * dy, 1.0);
        let t = clamp(((x - ax) * dx + (y - ay) * dy) / length, 0.0, 1.0);
        strength = 1.0 - t * t * (3.0 - 2.0 * t);
    } else {
        let long = max(width, height);
        let cx = aux[g + 1u] * width;
        let cy = aux[g + 2u] * height;
        let rx = max(aux[g + 5u] * long, 1.0);
        let ry = max(aux[g + 6u] * long, 1.0);
        let ex = (x - cx) / rx;
        let ey = (y - cy) / ry;
        let d = sqrt(ex * ex + ey * ey);
        let inner = 1.0 - clamp(aux[g + 7u], 0.0, 1.0);
        if (d <= inner) {
            strength = 1.0;
        } else if (d >= 1.0) {
            strength = 0.0;
        } else {
            let t = (d - inner) / max(1.0 - inner, 1e-3);
            strength = 1.0 - t * t * (3.0 - 2.0 * t);
        }
    }
    if (aux[g + 8u] > 0.5) {
        strength = 1.0 - strength;
    }
    return strength * clamp(aux[g + 9u] / 100.0, 0.0, 1.0);
}

/// Gradient::range_at: how much the layer's brightness and colour ranges
/// let through for colour `c`.
fn range_at(g: u32, c: vec3<f32>) -> f32 {
    var weight = 1.0;
    let lo = aux[g + 32u];
    let hi = aux[g + 33u];
    if (lo > 0.0 || hi < 1.0) {
        let l = luma(c);
        let f = max(aux[g + 34u] * 0.5, 0.002);
        var rise = 1.0;
        var fall = 1.0;
        if (lo > 0.0) {
            let t = clamp((l - (lo - f)) / (f * 2.0), 0.0, 1.0);
            rise = t * t * (3.0 - 2.0 * t);
        }
        if (hi < 1.0) {
            let t = clamp((l - (hi - f)) / (f * 2.0), 0.0, 1.0);
            fall = 1.0 - t * t * (3.0 - 2.0 * t);
        }
        weight = weight * rise * fall;
    }
    if (aux[g + 35u] > 0.5) {
        let mx = max(c.x, max(c.y, c.z));
        let mn = min(c.x, min(c.y, c.z));
        let d = mx - mn;
        if (d < 1e-5 || mx <= 0.0) {
            return 0.0;
        }
        var h: f32;
        if (mx == c.x) {
            h = (c.y - c.z) / d;
            h = h - 6.0 * floor(h / 6.0);
        } else if (mx == c.y) {
            h = (c.z - c.x) / d + 2.0;
        } else {
            h = (c.x - c.y) / d + 4.0;
        }
        h = h * 60.0;
        var delta = h - aux[g + 36u];
        delta = delta - 360.0 * floor(delta / 360.0);
        if (delta > 180.0) {
            delta = 360.0 - delta;
        }
        let width = max(aux[g + 37u], 1.0);
        let inner = width * (1.0 - clamp(aux[g + 38u], 0.0, 1.0));
        var w_hue: f32;
        if (delta <= inner) {
            w_hue = 1.0;
        } else if (delta >= width) {
            w_hue = 0.0;
        } else {
            let t = (delta - inner) / max(width - inner, 1e-3);
            w_hue = 1.0 - t * t * (3.0 - 2.0 * t);
        }
        let w_sat = clamp((d / mx) / 0.12, 0.0, 1.0);
        weight = weight * w_hue * w_sat;
    }
    return weight;
}

/// finishing/local.rs `adjust` for the layer at `g`, at mask `strength`.
fn local_adjust(rgb: vec3<f32>, g: u32, strength: f32, i: u32) -> vec3<f32> {
    var c = rgb;
    let exposure = aux[g + 10u];
    let contrast = aux[g + 11u];
    let highlights = aux[g + 12u];
    let shadows = aux[g + 13u];
    let temperature = aux[g + 14u];
    let tint = aux[g + 15u];
    let saturation = aux[g + 16u];
    let haze = aux[g + 17u];
    let clarity = aux[g + 18u];
    let sharpening = aux[g + 19u];
    let lens_blur = aux[g + 39u];
    let k = strength / 100.0;

    if (exposure != 0.0) {
        c = c * pow(2.0, exposure * k * 2.0);
    }
    if (shadows != 0.0 || highlights != 0.0) {
        let l = clamp(luma(c), 0.0, 1.0);
        let lift = shadows * k * spow(1.0 - l, 1.5) * 0.5 + highlights * k * spow(l, 1.5) * 0.35;
        c = c + vec3<f32>(lift);
    }
    if (contrast != 0.0) {
        c = (c - vec3<f32>(0.5)) * (1.0 + contrast * k) + vec3<f32>(0.5);
    }
    if (temperature != 0.0 || tint != 0.0) {
        let te = temperature * k;
        let ti = tint * k;
        c = c + vec3<f32>(te * 0.12 - ti * 0.06, ti * 0.12, -te * 0.12 - ti * 0.06);
    }
    if (saturation != 0.0) {
        let l = luma(c);
        c = vec3<f32>(l) + (c - vec3<f32>(l)) * (1.0 + saturation * k);
    }
    if (abs(haze * strength) > 0.5) {
        c = dehaze(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), haze * strength / 100.0);
    }
    if (lens_blur != 0.0 && (p.flags & F_WIDE) != 0u) {
        c = c + (px_a(i) - c) * (lens_blur * k);
    }
    if (clarity != 0.0 && (p.flags & F_WIDE) != 0u) {
        c = c + (c - px_a(i)) * (clarity * k * 0.9);
    }
    if (sharpening != 0.0 && (p.flags & F_NARROW) != 0u) {
        c = c + (c - px_b(i)) * (sharpening * k * 1.6);
    }
    if (aux[g + 20u] > 0.5) {
        let graded = grade(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), g + 21u);
        c = c + (graded - c) * strength;
    }
    return c;
}

// Curves, Color Mixer, Color Grading, Dehaze and gradient masks, before
// the crop (finishing/mod.rs `apply_row`). `img_a` and `img_b` hold the
// wide and narrow blurs local clarity and sharpening push away from.
@compute @workgroup_size(16, 16)
fn finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    var c = clamp(px_src(i), vec3<f32>(0.0), vec3<f32>(1.0));
    if ((p.flags & F_CURVES) != 0u) {
        c = vec3<f32>(curve(0u, c.x), curve(1u, c.y), curve(2u, c.z));
    }
    if ((p.flags & F_MIXER) != 0u) {
        c = mixer(c);
    }
    if ((p.flags & F_GRADING) != 0u) {
        c = grade(c, p.grading_offset);
    }
    if ((p.flags & F_DEHAZE) != 0u) {
        c = dehaze(c, p.dehaze);
    }
    if ((p.flags & F_LOCAL) != 0u) {
        let x = f32(p.off_x + id.x) + 0.5;
        let y = f32(p.off_y + id.y) + 0.5;
        for (var n = 0u; n < p.gradient_count; n++) {
            let g = p.gradient_offset + n * GRADIENT_STRIDE;
            var m = mask_at(g, x, y);
            if (m > 0.001) {
                m = m * range_at(g, c);
            }
            if (m > 0.001) {
                c = local_adjust(c, g, m, i);
            }
        }
    }
    store(i, c);
}

// ------------------------------------------------- crop, vignette, grain

/// Pixel at (`x`, `y`) of the full image `src` is a region of.
fn src_at(x: i32, y: i32) -> vec3<f32> {
    let lx = clamp(x - i32(p.src_off_x), 0, i32(p.src_width) - 1);
    let ly = clamp(y - i32(p.src_off_y), 0, i32(p.src_height) - 1);
    return px_src(u32(ly) * p.src_width + u32(lx));
}

/// crop.rs `bilinear`: between pixel centres of the full image, black off
/// it.
fn crop_bilinear(x: f32, y: f32) -> vec3<f32> {
    let w = i32(p.full_w);
    let h = i32(p.full_h);
    if (x < -1.0 || y < -1.0 || x > f32(w) || y > f32(h)) {
        return vec3<f32>(0.0);
    }
    let fx = floor(x);
    let fy = floor(y);
    let tx = x - fx;
    let ty = y - fy;
    let ix = i32(fx);
    let iy = i32(fy);
    let x0 = clamp(ix, 0, w - 1);
    let x1 = clamp(ix + 1, 0, w - 1);
    let y0 = clamp(iy, 0, h - 1);
    let y1 = clamp(iy + 1, 0, h - 1);
    let a = src_at(x0, y0);
    let b = src_at(x1, y0);
    let c = src_at(x0, y1);
    let d = src_at(x1, y1);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    return top + (bottom - top) * ty;
}

/// finishing/effects.rs `hash`: a position hash in -1..=1.
fn grain_hash(x: u32, y: u32) -> f32 {
    var h = (x * 0x8da6b343u) ^ (y * 0xd8163841u);
    h ^= h >> 15u;
    h *= 0x2c1b3c6du;
    h ^= h >> 12u;
    h *= 0x297a2d39u;
    h ^= h >> 15u;
    return (f32(h) / 4294967296.0) * 2.0 - 1.0;
}

// Straighten and crop `src` into this pass's output, then Vignette and
// Grain, which frame the crop.
@compute @workgroup_size(16, 16)
fn crop_finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let ox = p.off_x + id.x;
    let oy = p.off_y + id.y;
    var c: vec3<f32>;
    if (p.crop_rotate != 0u) {
        let cx = f32(p.full_w) / 2.0;
        let cy = f32(p.full_h) / 2.0;
        let dx = p.crop_x0 + f32(ox) + 0.5 - cx;
        let dy = p.crop_y0 + f32(oy) + 0.5 - cy;
        let rx = dx * p.crop_cos - dy * p.crop_sin;
        let ry = dx * p.crop_sin + dy * p.crop_cos;
        let w = max(1.0 + p.persp_x * rx / cx + p.persp_y * ry / cy, 0.05);
        let sx = cx + rx / w;
        let sy = cy + ry / w;
        c = crop_bilinear(sx - 0.5, sy - 0.5);
    } else {
        c = src_at(i32(p.crop_left + ox), i32(p.crop_top + oy));
    }
    if ((p.flags & F_VIGNETTE) != 0u) {
        let longer = max(p.vig_cx, p.vig_cy);
        let reach_x = max(p.vig_cx + (longer - p.vig_cx) * p.vig_round, 1.0);
        let reach_y = max(p.vig_cy + (longer - p.vig_cy) * p.vig_round, 1.0);
        let dx = (f32(ox) + 0.5 - p.vig_cx) / reach_x;
        let dy = (f32(oy) + 0.5 - p.vig_cy) / reach_y;
        let r = sqrt(dx * dx + dy * dy) / 1.4142135;
        let v = smoothstep2(p.vig_inner, p.vig_outer, r);
        if (p.vignette < 0.0) {
            c = c * (1.0 + p.vignette * v);
        } else {
            c = c + (vec3<f32>(1.0) - c) * p.vignette * v;
        }
    }
    if ((p.flags & F_GRAIN) != 0u) {
        let noise = 0.7 * grain_hash(ox / p.grain_cell, oy / p.grain_cell)
            + 0.3 * grain_hash(ox ^ 0x9e37u, oy ^ 0x79b9u);
        let l = luma(c);
        let weight = 0.4 + 2.4 * l * (1.0 - l);
        c = c + vec3<f32>(noise * p.grain_strength * weight);
    }
    store(id.y * p.width + id.x, c);
}

// ------------------------------------------------- resizing

fn sinc(x: f32) -> f32 {
    if (x == 0.0) { return 1.0; }
    let a = x * 3.14159265358979;
    return sin(a) / a;
}

/// image's filter kernels: Triangle (`resize_filter` 0) or Lanczos3 (1).
fn resize_weight(x: f32) -> f32 {
    if (p.resize_filter == 1u) {
        if (abs(x) >= 3.0) { return 0.0; }
        return sinc(x) * sinc(x / 3.0);
    }
    return max(1.0 - abs(x), 0.0);
}

fn resize_support() -> f32 {
    if (p.resize_filter == 1u) { return 3.0; }
    return 1.0;
}

// image::imageops::resize, vertical half: `src` (any source format,
// src_width x src_height) -> `tmp` (src_width x height).
@compute @workgroup_size(16, 16)
fn resize_v(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.src_width || id.y >= p.height) { return; }
    let ratio = f32(p.src_height) / f32(p.height);
    let sratio = max(ratio, 1.0);
    let support = resize_support() * sratio;
    let centre = (f32(id.y) + 0.5) * ratio;
    let left = u32(clamp(i32(floor(centre - support)), 0, i32(p.src_height) - 1));
    let right = u32(clamp(i32(ceil(centre + support)), i32(left) + 1, i32(p.src_height)));
    var acc = vec3<f32>(0.0);
    var sum = 0.0;
    for (var y = left; y < right; y++) {
        let w = resize_weight((f32(y) - (centre - 0.5)) / sratio);
        acc += read_src(id.x, y) * w;
        sum += w;
    }
    tmp[id.y * p.src_width + id.x] = vec4<f32>(acc / sum, 0.0);
}

// Horizontal half: `tmp` -> image (width x height).
@compute @workgroup_size(16, 16)
fn resize_h(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let ratio = f32(p.src_width) / f32(p.width);
    let sratio = max(ratio, 1.0);
    let support = resize_support() * sratio;
    let centre = (f32(id.x) + 0.5) * ratio;
    let left = u32(clamp(i32(floor(centre - support)), 0, i32(p.src_width) - 1));
    let right = u32(clamp(i32(ceil(centre + support)), i32(left) + 1, i32(p.src_width)));
    var acc = vec3<f32>(0.0);
    var sum = 0.0;
    for (var x = left; x < right; x++) {
        let w = resize_weight((f32(x) - (centre - 0.5)) / sratio);
        acc += tmp[id.y * p.src_width + x].xyz * w;
        sum += w;
    }
    store_hdr(id.y * p.width + id.x, acc / sum);
}

// ------------------------------------------------- output

/// The output colour conversion (cms.rs `Lut3d`) at `lut_offset`:
/// `lut_size` 1 is a matrix conversion (3x3 matrix, then a 4096-entry
/// curve per channel), larger sizes a `lut_size`^3 grid (red fastest).
const OUT_CURVE: u32 = 4096u;

fn lut_at(r: u32, g: u32, b: u32) -> vec3<f32> {
    let n = p.lut_size;
    let i = p.lut_offset + ((b * n + g) * n + r) * 3u;
    return vec3<f32>(aux[i], aux[i + 1u], aux[i + 2u]);
}

fn decode_srgb(v: f32) -> f32 {
    if (v <= 0.04045) { return v / 12.92; }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn out_curve(channel: u32, v: f32) -> f32 {
    let x = v * f32(OUT_CURVE - 1u);
    let i0 = u32(floor(x));
    let i1 = min(i0 + 1u, OUT_CURVE - 1u);
    let base = p.lut_offset + 9u + channel * OUT_CURVE;
    return mix(aux[base + i0], aux[base + i1], x - f32(i0));
}

fn apply_lut(c0: vec3<f32>) -> vec3<f32> {
    let c = clamp(c0, vec3<f32>(0.0), vec3<f32>(1.0));
    if (p.lut_size == 0u) { return c; }
    if (p.lut_size == 1u) {
        let lin = vec3<f32>(decode_srgb(c.x), decode_srgb(c.y), decode_srgb(c.z));
        let m = p.lut_offset;
        let out = clamp(vec3<f32>(
            aux[m] * lin.x + aux[m + 1u] * lin.y + aux[m + 2u] * lin.z,
            aux[m + 3u] * lin.x + aux[m + 4u] * lin.y + aux[m + 5u] * lin.z,
            aux[m + 6u] * lin.x + aux[m + 7u] * lin.y + aux[m + 8u] * lin.z,
        ), vec3<f32>(0.0), vec3<f32>(1.0));
        return clamp(vec3<f32>(out_curve(0u, out.x), out_curve(1u, out.y), out_curve(2u, out.z)), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let n = f32(p.lut_size - 1u);
    let x = c * n;
    let i0 = vec3<u32>(floor(x));
    let i1 = min(i0 + vec3<u32>(1u), vec3<u32>(p.lut_size - 1u));
    let t = x - floor(x);
    let c00 = mix(lut_at(i0.x, i0.y, i0.z), lut_at(i1.x, i0.y, i0.z), t.x);
    let c10 = mix(lut_at(i0.x, i1.y, i0.z), lut_at(i1.x, i1.y, i0.z), t.x);
    let c01 = mix(lut_at(i0.x, i0.y, i1.z), lut_at(i1.x, i0.y, i1.z), t.x);
    let c11 = mix(lut_at(i0.x, i1.y, i1.z), lut_at(i1.x, i1.y, i1.z), t.x);
    return clamp(mix(mix(c00, c10, t.y), mix(c01, c11, t.y), t.z), vec3<f32>(0.0), vec3<f32>(1.0));
}

/// 8-bit levels of the working-space pixel `i` (what the histogram and
/// clipping warnings measure).
fn levels8(i: u32) -> vec3<u32> {
    return vec3<u32>(floor(clamp(px_src(i), vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5));
}

/// 8-bit output levels of pixel `i`, converted by the output table.
fn output8(i: u32) -> vec3<u32> {
    return vec3<u32>(floor(apply_lut(px_src(i)) * 255.0 + 0.5));
}

fn flat_index(id: vec3<u32>, groups: vec3<u32>) -> u32 {
    return id.y * groups.x * 256u + id.x;
}

// Image -> tightly packed 8-bit RGB bytes in the output colour space,
// one output word per thread.
@compute @workgroup_size(256)
fn out_rgb(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let word = flat_index(id, groups);
    let total = p.width * p.height * 3u;
    if (word * 4u >= total) { return; }
    var out = 0u;
    var cached_pixel = 0xffffffffu;
    var cached = vec3<u32>(0u);
    for (var k = 0u; k < 4u; k++) {
        let b = word * 4u + k;
        if (b < total) {
            let pixel = b / 3u;
            if (pixel != cached_pixel) {
                cached = output8(pixel);
                cached_pixel = pixel;
            }
            out |= cached[b % 3u] << (k * 8u);
        }
    }
    dst[word] = out;
}

// Image -> tightly packed 16-bit RGB samples in the output colour space,
// two samples per output word (the first in the low half).
@compute @workgroup_size(256)
fn out_rgb16(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let word = flat_index(id, groups);
    let total = p.width * p.height * 3u;
    if (word * 2u >= total) { return; }
    var out = 0u;
    for (var k = 0u; k < 2u; k++) {
        let b = word * 2u + k;
        if (b < total) {
            let c = clamp(apply_lut(px_src(b / 3u)), vec3<f32>(0.0), vec3<f32>(1.0));
            let v = u32(floor(c[b % 3u] * 65535.0 + 0.5));
            out |= v << (k * 16u);
        }
    }
    dst[word] = out;
}

// Image -> the BGRA frame the viewer shows, in the display's colour
// space, with clipped pixels painted when the warnings are on (blue:
// crushed shadows, red: blown highlights).
@compute @workgroup_size(256)
fn out_bgra(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = flat_index(id, groups);
    if (i >= p.width * p.height) { return; }
    let level = levels8(i);
    let mx = max(level.x, max(level.y, level.z));
    var c = output8(i);
    if (p.clip_highlights != 0u && mx >= 254u) {
        c = vec3<u32>(255u, 40u, 40u);
    } else if (p.clip_shadows != 0u && mx <= 2u) {
        c = vec3<u32>(40u, 110u, 255u);
    }
    dst[i] = c.z | (c.y << 8u) | (c.x << 16u) | (255u << 24u);
}

// Histogram layout in `hist` (u32 counters), matching
// rustyedit_core::Histogram::from_rgb8_sized.
const H_LEVELS: u32 = 1024u;           // red, green, blue, luma: 256 each
const WAVE_COLS: u32 = 128u;
const WAVE_ROWS: u32 = 64u;
const WAVE_CELLS: u32 = 8192u;         // WAVE_COLS * WAVE_ROWS
const H_WAVES: u32 = 1024u;            // red, green, blue, luma grids follow
const H_VECTOR: u32 = 33792u;          // H_WAVES + 4 * WAVE_CELLS
const VECTOR_SIZE: u32 = 64u;

var<workgroup> levels: array<atomic<u32>, 1024>;

fn level_row(v: u32) -> u32 {
    return (v * (WAVE_ROWS - 1u) + 127u) / 255u;
}

// Level histograms (gathered per workgroup first, to keep the global
// atomics cool), waveforms and vectorscope of an image. Dispatched
// one-dimensionally; each thread walks the image in strides.
@compute @workgroup_size(256)
fn histogram(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
    @builtin(num_workgroups) groups: vec3<u32>,
) {
    for (var k = li; k < H_LEVELS; k += 256u) {
        atomicStore(&levels[k], 0u);
    }
    workgroupBarrier();
    let total = p.width * p.height;
    let threads = groups.x * 256u;
    for (var i = gid.x; i < total; i += threads) {
        let v = levels8(i);
        let r = v.x;
        let g = v.y;
        let b = v.z;
        let l = (54u * r + 183u * g + 19u * b) >> 8u;
        atomicAdd(&levels[r], 1u);
        atomicAdd(&levels[256u + g], 1u);
        atomicAdd(&levels[512u + b], 1u);
        atomicAdd(&levels[768u + l], 1u);
        if ((i / p.width) % p.hist_stride != 0u) {
            continue;
        }
        let col = (i % p.width) * WAVE_COLS / p.width;
        atomicAdd(&hist[H_WAVES + level_row(r) * WAVE_COLS + col], 1u);
        atomicAdd(&hist[H_WAVES + WAVE_CELLS + level_row(g) * WAVE_COLS + col], 1u);
        atomicAdd(&hist[H_WAVES + 2u * WAVE_CELLS + level_row(b) * WAVE_COLS + col], 1u);
        atomicAdd(&hist[H_WAVES + 3u * WAVE_CELLS + level_row(l) * WAVE_COLS + col], 1u);
        let fr = f32(r) / 255.0;
        let fg = f32(g) / 255.0;
        let fb = f32(b) / 255.0;
        let y = 0.2126 * fr + 0.7152 * fg + 0.0722 * fb;
        let cb = (fb - y) / 1.8556;
        let cr = (fr - y) / 1.5748;
        let vx = min(u32(max((cb + 0.5) * f32(VECTOR_SIZE) + 0.01, 0.0)), VECTOR_SIZE - 1u);
        let vy = min(u32(max((0.5 - cr) * f32(VECTOR_SIZE) + 0.01, 0.0)), VECTOR_SIZE - 1u);
        atomicAdd(&hist[H_VECTOR + vy * VECTOR_SIZE + vx], 1u);
    }
    workgroupBarrier();
    for (var k = li; k < H_LEVELS; k += 256u) {
        let n = atomicLoad(&levels[k]);
        if (n > 0u) {
            atomicAdd(&hist[k], n);
        }
    }
}

// ------------------------------------------------- lens blur (lens_blur.rs)

/// LensBlur::blur_at: how blurred (0..1) a pixel `focus` in focus is.
fn lb_blur_at(focus: f32) -> f32 {
    var away = 1.0 - focus;
    if (p.lb_invert != 0u) { away = focus; }
    away = clamp(away, 0.0, 1.0);
    let t = clamp((away - 0.15) / 0.35, 0.0, 1.0);
    let steep = t * t * (3.0 - 2.0 * t);
    return steep + (away - steep) * clamp(p.lb_falloff, 0.0, 1.0);
}

/// Each pixel's colour and circle of confusion, into `tmp`.
@compute @workgroup_size(16, 16)
fn lens_coc(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    let c = clamp(px_src(i), vec3<f32>(0.0), vec3<f32>(1.0));
    let x = f32(p.off_x + id.x) + 0.5;
    let y = f32(p.off_y + id.y) + 0.5;
    var m = mask_at(p.lb_focus, x, y);
    if (m > 0.001) {
        m = m * range_at(p.lb_focus, c);
    }
    tmp[i] = vec4<f32>(c, p.lb_radius * lb_blur_at(m));
}

/// lens_blur.rs `highlight`: how much a pixel is a point of light.
fn lb_highlight(l: f32, around: f32) -> f32 {
    let t = clamp((l - 0.55) / 0.35, 0.0, 1.0);
    let bright = t * t * (3.0 - 2.0 * t);
    return bright * clamp((l - around - 0.3) / 0.25, 0.0, 1.0);
}

/// The points of light (`src` against its wide blur in `img_a`).
@compute @workgroup_size(16, 16)
fn lens_bright(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    let c = clamp(px_src(i), vec3<f32>(0.0), vec3<f32>(1.0));
    let soft = clamp(px_a(i), vec3<f32>(0.0), vec3<f32>(1.0));
    store(i, c * lb_highlight(luma(c), luma(soft)));
}


/// `tmp` at (`x`, `y`) (pixel centres at .5), bilinear, inside the pass.
fn lb_bilinear(x: f32, y: f32) -> vec3<f32> {
    let w = f32(p.width);
    let h = f32(p.height);
    let fx = clamp(x - 0.5, 0.0, w - 1.0);
    let fy = clamp(y - 0.5, 0.0, h - 1.0);
    let x0 = u32(floor(fx));
    let y0 = u32(floor(fy));
    let x1 = min(x0 + 1u, p.width - 1u);
    let y1 = min(y0 + 1u, p.height - 1u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let top = mix(tmp[y0 * p.width + x0].xyz, tmp[y0 * p.width + x1].xyz, tx);
    let bottom = mix(tmp[y1 * p.width + x0].xyz, tmp[y1 * p.width + x1].xyz, tx);
    return mix(top, bottom, ty);
}

/// The pixel nearest (`x`, `y`): the softened copies are smooth enough
/// to read without blending neighbours (lens_blur.rs `index`).
fn lb_index(x: f32, y: f32) -> u32 {
    let xi = min(u32(max(floor(x), 0.0)), p.width - 1u);
    let yi = min(u32(max(floor(y), 0.0)), p.height - 1u);
    return yi * p.width + xi;
}

/// Gathers each pixel's bokeh from `tmp` (see lens_blur.rs `apply`).
@compute @workgroup_size(16, 16)
fn lens_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!in_bounds(id)) { return; }
    let i = id.y * p.width + id.x;
    let here = tmp[i];
    let r = here.w;
    if (r < 0.5) {
        store(i, here.xyz);
        return;
    }
    let cx = f32(id.x) + 0.5;
    let cy = f32(id.y) + 0.5;
    let gx = f32(p.off_x) + cx;
    let gy = f32(p.off_y) + cy;
    let edge = vec2<f32>(gx / f32(p.full_w) * 2.0 - 1.0, gy / f32(p.full_h) * 2.0 - 1.0);
    var sum = here.xyz;
    var total = 1.0;
    var glow = vec3<f32>(0.0);
    var glow_n = 0.0;
    // lens_blur.rs `level`, `level_start` and `pre_blur_mix`.
    let wanted = r * 2.0;
    var count = 64u;
    var start = 48u;
    if (wanted <= 16.0) {
        count = 16u;
        start = 0u;
    } else if (wanted <= 32.0) {
        count = 32u;
        start = 16u;
    }
    let soft_mix = clamp(r / max(p.lb_radius, 1.0) * 1.2, 0.0, 1.0);
    let lit = p.lb_boost > 0.0;
    for (var n = 0u; n < count; n++) {
        let at = p.lb_table + (start + n) * 3u;
        let o = vec2<f32>(aux[at], aux[at + 1u]);
        let shape_w = aux[at + 2u];
        if (p.lb_shape == 2u) {
            let k = o + edge * 0.6;
            if (dot(k, k) > 1.0) { continue; }
        }
        let qx = cx + o.x * r;
        let qy = cy + o.y * r;
        let distance = length(o) * r;
        glow_n += shape_w;
        let q = lb_index(qx, qy);
        let reach = clamp(tmp[q].w - distance + 1.0, 0.0, 1.0);
        if (reach <= 0.0) { continue; }
        if (lit) {
            glow += px_b(q) * (reach * shape_w);
        }
        var c: vec3<f32>;
        if (soft_mix >= 1.0) {
            c = px_a(q);
        } else {
            c = mix(lb_bilinear(qx, qy), px_a(q), soft_mix);
        }
        let weight = reach * shape_w;
        sum += c * weight;
        total += weight;
    }
    // lens_blur.rs `bloom`: isolated lights bloom, crowded ones don't.
    let lights = glow / max(glow_n, 1.0);
    let gain = p.lb_boost * 40.0;
    let strength = gain / (1.0 + luma(lights) * gain / 0.6);
    store(i, sum / total + lights * strength);
}
