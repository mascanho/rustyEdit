//! wgpu compute backend for the whole develop pipeline (`shader.wgsl`).
//!
//! A render prepares the photo once (upload; for RAW files, demosaic; then
//! spot removal), runs every stage as a compute pass in one command
//! buffer, and reads back once: the RGB result (exports), the BGRA frame
//! the viewer shows plus the histogram and scopes (live preview), or a
//! sharp region of the full-size photo (the zoomed-in detail view).
//!
//! Between renders the GPU keeps, per kind of render (preview, detail):
//! - its working buffers, so a slider step allocates nothing big;
//! - the prepared photo, so a slider step uploads nothing;
//! - the "early" stage (lens corrections, orientation, noise reduction;
//!   for RAW files also white balance and exposure), re-run only when one
//!   of its own settings changes.
//!
//! Images between stages are half floats, so nothing is quantised to 8
//! bits until the output.
//!
//! Everything here is best-effort: any failure (no adapter, an image too
//! big for the device, a validation or out-of-memory error) yields `None`
//! and the caller falls back to the CPU path.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bytemuck::{Pod, Zeroable};
use image::{DynamicImage, RgbImage, RgbaImage};
use rustyedit_core::{
    BandAdjust, ColorBand, DevelopSettings, Gradient, GradientKind, MAX_DABS, Histogram, Scopes, SpotKind,
    Spots, VECTOR_SIZE, WAVE_COLS, WAVE_ROWS,
};
use wgpu::util::DeviceExt;

use crate::cms::{self, OutputSpace};
use crate::develop_source::{DevelopSource, Pixels, RawColor, RawMosaic};
use crate::finishing::effects::{Dehaze, Grain, Vignette};
use crate::finishing::grading::Grading;

/// Uniforms shared by every kernel; must match `Params` in the shader
/// field for field.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct Params {
    width: u32,
    height: u32,
    src_width: u32,
    src_height: u32,
    flip_h: u32,
    flip_v: u32,
    quarters: u32,
    lens_on: u32,
    lens_k: f32,
    lens_ca: f32,
    lens_vig: f32,
    lens_norm2: f32,
    lens_cx: f32,
    lens_cy: f32,
    dn_luma: f32,
    dn_color: f32,
    dn_luma_sigma_s: f32,
    dn_luma_sigma_r: f32,
    dn_color_sigma_r: f32,
    flags: u32,
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
    threshold: f32,
    blur_radius: i32,
    blur_offset: u32,
    sharpen_strength: f32,
    sharpen_threshold: f32,
    sharpen_masking: u32,
    curves_offset: u32,
    mixer_offset: u32,
    grading_offset: u32,
    gradient_offset: u32,
    gradient_count: u32,
    dehaze: f32,
    vignette: f32,
    vig_cx: f32,
    vig_cy: f32,
    vig_inner: f32,
    vig_outer: f32,
    grain_strength: f32,
    grain_cell: u32,
    crop_x0: f32,
    crop_y0: f32,
    crop_sin: f32,
    crop_cos: f32,
    crop_rotate: u32,
    crop_left: u32,
    crop_top: u32,
    clip_shadows: u32,
    clip_highlights: u32,
    hist_stride: u32,
    resize_filter: u32,
    source_format: u32,
    source_linear: u32,
    raw_offset: u32,
    lut_offset: u32,
    lut_size: u32,
    off_x: u32,
    off_y: u32,
    full_w: u32,
    full_h: u32,
    src_off_x: u32,
    src_off_y: u32,
    spot_offset: u32,
    /// Perspective correction strengths (Crop::perspective_strength).
    persp_x: f32,
    persp_y: f32,
    /// Vignette roundness, 0..=1.
    vig_round: f32,
    /// Lens blur (lens_blur.rs): the largest radius in pixels, Boost and
    /// Falloff (0..=1), the bokeh shape, inverted, and the focus mask's
    /// table in `aux`.
    lb_radius: f32,
    lb_boost: f32,
    lb_falloff: f32,
    lb_shape: u32,
    lb_invert: u32,
    lb_focus: u32,
    /// The sample table (lens_blur.rs `sample_table`) in `aux`.
    lb_table: u32,
    _pad3: f32,
    _pad4: f32,
}

const _: () = assert!(std::mem::size_of::<Params>() == 336);

/// `resize_filter` values.
const TRIANGLE: u32 = 0;
const LANCZOS3: u32 = 1;

/// `source_format` values: 8-bit RGB, 16-bit RGB, half-float image.
const SRC_RGB8: u32 = 0;
const SRC_RGB16: u32 = 1;
const SRC_HALF: u32 = 2;

// Finishing flags, as in the shader.
const F_CURVES: u32 = 1;
const F_MIXER: u32 = 2;
const F_GRADING: u32 = 4;
const F_DEHAZE: u32 = 8;
const F_LOCAL: u32 = 16;
const F_VIGNETTE: u32 = 32;
const F_GRAIN: u32 = 64;
const F_WIDE: u32 = 128;
const F_NARROW: u32 = 256;

/// Floats per gradient layer in the aux buffer.
const GRADIENT_STRIDE: usize = 40 + 4 * MAX_DABS;
/// Entries per channel of the tone curve tables (the shader's
/// `CURVE_STEPS`).
const CURVE_STEPS: usize = 1024;
/// Bytes per pixel of a half-float image.
const HALF_PX: u64 = 8;

/// Histogram counters: four level histograms, four waveform grids and the
/// vectorscope, laid out as the shader's `H_*` constants say.
const WAVE_CELLS: usize = WAVE_COLS * WAVE_ROWS;
const H_WAVES: usize = 1024;
const H_VECTOR: usize = H_WAVES + 4 * WAVE_CELLS;
const HIST_WORDS: usize = H_VECTOR + VECTOR_SIZE * VECTOR_SIZE;
const HIST_BYTES: u64 = HIST_WORDS as u64 * 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kernel {
    Load,
    Demosaic,
    SpotRing,
    SpotApply,
    Defringe,
    DenoisePrep,
    DenoiseFilter,
    DenoiseCombine,
    ToneColor,
    BlurH,
    BlurV,
    Unsharp,
    Sharpen,
    Finish,
    CropFinish,
    ResizeV,
    ResizeH,
    OutRgb,
    OutRgb16,
    OutBgra,
    Histogram,
    LensCoc,
    LensBlur,
    LensBright,
    /// Not a kernel: copies `src` to `dst` (`Pass::dst_bytes` of them).
    Copy,
}

const ENTRY_POINTS: [&str; 24] = [
    "load",
    "demosaic",
    "spot_ring",
    "spot_apply",
    "defringe",
    "denoise_prep",
    "denoise_filter",
    "denoise_combine",
    "tone_color",
    "blur_h",
    "blur_v",
    "unsharp",
    "sharpen",
    "finish",
    "crop_finish",
    "resize_v",
    "resize_h",
    "out_rgb",
    "out_rgb16",
    "out_bgra",
    "histogram",
    "lens_coc",
    "lens_blur",
    "lens_bright",
];

impl Kernel {
    /// Whether it uses the `tmp`, `tmp2` and `hist` slots.
    fn scratch(self) -> (bool, bool, bool) {
        match self {
            Kernel::DenoiseFilter | Kernel::DenoiseCombine => (true, true, false),
            Kernel::DenoisePrep
            | Kernel::BlurH
            | Kernel::BlurV
            | Kernel::ResizeV
            | Kernel::ResizeH
            | Kernel::SpotRing
            | Kernel::SpotApply
            | Kernel::LensCoc
            | Kernel::LensBlur => (true, false, false),
            Kernel::Histogram => (false, false, true),
            _ => (false, false, false),
        }
    }
}

/// The buffers a pass can bind as its image input (`src`, `img_a`,
/// `img_b`) or output (`dst`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum Buf {
    /// The photo as uploaded (or demosaiced).
    Source,
    /// A RAW file's sensor data.
    Mosaic,
    /// The photo with its spots removed (two, to alternate between).
    SpotA,
    SpotB,
    /// Output of the early stage.
    Early,
    Ping,
    Pong,
    BlurA,
    BlurB,
    /// Stands in for an input the kernel doesn't read.
    NoInput,
    /// Stands in for an output the kernel doesn't write.
    NoOutput,
}

/// Every buffer a render can own, each made the first time a plan needs
/// it and grown when a plan needs more.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum Slot {
    Image(Buf),
    Tmp,
    Tmp2,
    Hist,
    Readback,
}

impl Slot {
    fn of(buf: Buf) -> Option<Slot> {
        match buf {
            Buf::NoInput | Buf::NoOutput => None,
            other => Some(Slot::Image(other)),
        }
    }

    fn usage(self) -> wgpu::BufferUsages {
        use wgpu::BufferUsages as U;
        match self {
            Slot::Image(_) => U::STORAGE | U::COPY_SRC | U::COPY_DST,
            Slot::Tmp | Slot::Tmp2 => U::STORAGE,
            Slot::Hist => U::STORAGE | U::COPY_SRC | U::COPY_DST,
            Slot::Readback => U::MAP_READ | U::COPY_DST,
        }
    }
}

struct Pass {
    kernel: Kernel,
    params: Params,
    src: Buf,
    dst: Buf,
    img_a: Buf,
    img_b: Buf,
    groups: [u32; 3],
    /// How big `dst` must be.
    dst_bytes: u64,
    /// How big the float scratch must be.
    tmp_bytes: u64,
}

/// What a render produces.
#[derive(Clone)]
pub(crate) enum Output {
    /// 8-bit RGB in `space`, optionally fitted within a long edge
    /// (Lanczos3) and then sharpened (`imageops::unsharpen` sigma and
    /// threshold), as exports want them.
    Rgb {
        fit: Option<u32>,
        sharpen: Option<(f32, i32)>,
        space: OutputSpace,
        /// 16 bits per channel instead of 8.
        deep: bool,
    },
    /// The live preview: a BGRA frame in the display's colour space, plus
    /// the histogram and scopes (of the working-space render).
    Preview {
        clip_shadows: bool,
        clip_highlights: bool,
    },
    /// A region of the photo at full resolution (fractions of the cropped
    /// photo: left, top, width, height), scaled to `size`, as BGRA in the
    /// display's colour space.
    Detail {
        region: (f32, f32, f32, f32),
        size: (u32, u32),
    },
}

pub(crate) enum Rendered {
    Rgb(RgbImage),
    Rgb16(image::ImageBuffer<image::Rgb<u16>, Vec<u16>>),
    Preview { bgra: RgbaImage, histogram: Histogram },
    Detail(RgbaImage),
}

/// The settings the prepared photo depends on.
#[derive(Clone, PartialEq)]
struct SourceKey {
    id: u64,
    spots: Spots,
}

/// The settings the early stage depends on, as bits so it can be `Eq`.
#[derive(Clone, Copy, PartialEq, Eq)]
struct EarlyKey {
    lens: [u32; 3],
    noise: [u32; 2],
    /// RAW only: exposure, temperature and tint.
    raw: [u32; 3],
    flips: (bool, bool),
    quarters: u32,
    /// The region of the oriented photo it covers.
    window: [u32; 4],
}

/// The photo as the load stage reads it.
#[derive(Clone, Copy)]
struct Prepared {
    buf: Buf,
    format: u32,
    raw: Option<RawColor>,
}

/// Buffers (and what's cached in them) for one kind of render.
#[derive(Default)]
struct Resources {
    buffers: HashMap<Slot, (wgpu::Buffer, u64)>,
    source_key: Option<SourceKey>,
    prepared: Option<Prepared>,
    early_key: Option<EarlyKey>,
}

impl Resources {
    fn get(&self, slot: Slot) -> Option<&wgpu::Buffer> {
        self.buffers.get(&slot).map(|(buffer, _)| buffer)
    }
}

/// A copy from a result buffer into the readback buffer.
struct ReadCopy {
    from: Slot,
    to: u64,
    bytes: u64,
}

/// Which cached set of buffers a render uses.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Channel {
    Preview,
    Detail,
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::ComputePipeline>,
    /// Bound to slots a pass doesn't use. Inputs can share one buffer;
    /// each writable slot needs its own.
    no_input: wgpu::Buffer,
    no_output: wgpu::Buffer,
    no_tmp: wgpu::Buffer,
    no_tmp2: wgpu::Buffer,
    no_hist: wgpu::Buffer,
    max_buffer: u64,
    name: String,
    /// The live preview's and the detail view's buffers and caches.
    pools: Mutex<HashMap<Channel, Resources>>,
    /// Held by one-off full-size work (exports, resizes): one at a time
    /// keeps GPU memory bounded however many threads ask, while their
    /// decoding and encoding still run in parallel on the CPU.
    exclusive: Mutex<()>,
}

static GPU: OnceLock<Option<Gpu>> = OnceLock::new();

fn gpu() -> Option<&'static Gpu> {
    GPU.get_or_init(Gpu::new).as_ref()
}

/// Name of the GPU the develop pipeline runs on, if one is available.
/// Initialises the device on first call, so run it off the UI thread.
pub fn adapter_name() -> Option<String> {
    gpu().map(|g| g.name.clone())
}

/// Runs the develop pipeline on the GPU. `settings` must already be
/// [`DevelopSettings::effective`]. `None` means "use the CPU path".
pub(crate) fn render(
    source: &DevelopSource,
    settings: &DevelopSettings,
    output: Output,
) -> Option<Rendered> {
    gpu()?.render(source, settings, output)
}

/// Whether `source` fits the GPU, so a render of it (a detail view, say)
/// is quick rather than a full CPU pass.
pub(crate) fn fits(source: &DevelopSource) -> bool {
    gpu().is_some_and(|g| {
        let px = source.width() as u64 * source.height() as u64;
        px > 0 && px * 16 <= g.max_buffer
    })
}

/// Resizes an 8- or 16-bit image to `width` x `height` with a triangle
/// filter on the GPU (like `image`'s `FilterType::Triangle`), to 8 bits.
pub(crate) fn resize(source: &DynamicImage, width: u32, height: u32) -> Option<RgbImage> {
    gpu()?.resize(source, width, height)
}

/// A preview-sized copy of `source` (long edge `max_edge` at most) in
/// GPU format: demosaiced for RAW files, otherwise just resized.
pub(crate) fn preview_source(source: &DevelopSource, max_edge: u32) -> Option<DevelopSource> {
    gpu()?.preview_source(source, max_edge)
}

impl Gpu {
    fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .ok()?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        if limits.max_storage_buffers_per_shader_stage < 8 {
            return None;
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rustyedit-develop"),
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .ok()?;

        // Surface any shader/pipeline validation problem as "no GPU" rather
        // than a panic on first render.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("develop"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let storage = |read_only| wgpu::BufferBindingType::Storage { read_only };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("develop-layout"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, storage(true)),
                entry(2, storage(false)),
                entry(3, storage(false)),
                entry(4, storage(false)),
                entry(5, storage(true)),
                entry(6, storage(true)),
                entry(7, storage(true)),
                entry(8, storage(false)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("develop-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipelines = ENTRY_POINTS
            .iter()
            .map(|entry| {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&pipeline_layout),
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            })
            .collect();
        if let Some(error) = pollster::block_on(scope.pop()) {
            eprintln!("GPU develop pipeline unavailable: {error}");
            return None;
        }

        let max_buffer = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size) as u64;
        let small = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 16,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            no_input: small("no-input"),
            no_output: small("no-output"),
            no_tmp: small("no-tmp"),
            no_tmp2: small("no-tmp2"),
            no_hist: small("no-hist"),
            device,
            queue,
            layout,
            pipelines,
            max_buffer,
            name: info.name,
            pools: Mutex::new(HashMap::new()),
            exclusive: Mutex::new(()),
        })
    }

    fn buffer(&self, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size.max(16).next_multiple_of(4),
            usage,
            mapped_at_creation: false,
        })
    }

    /// Makes sure `slot` holds at least `bytes`. Returns whether it had to
    /// be (re)made, losing what it held.
    fn ensure(&self, res: &mut Resources, slot: Slot, bytes: u64) -> bool {
        if res.buffers.get(&slot).is_some_and(|(_, size)| *size >= bytes) {
            return false;
        }
        let size = bytes.max(16).next_multiple_of(4);
        let buffer = self.buffer(&format!("{slot:?}"), size, slot.usage());
        res.buffers.insert(slot, (buffer, size));
        true
    }

    /// `queue.write_buffer` wants whole words; pads the tail.
    fn write_padded(&self, buffer: &wgpu::Buffer, bytes: &[u8]) {
        let aligned = bytes.len() / 4 * 4;
        if aligned > 0 {
            self.queue.write_buffer(buffer, 0, &bytes[..aligned]);
        }
        if aligned < bytes.len() {
            let mut tail = [0u8; 4];
            tail[..bytes.len() - aligned].copy_from_slice(&bytes[aligned..]);
            self.queue.write_buffer(buffer, aligned as u64, &tail);
        }
    }

    /// Uploads `source` and plans what turns it into the image the load
    /// stage reads (demosaic, spot removal), unless `res` already holds it
    /// for these spots. Returns how to read it.
    fn prepare_source(
        &self,
        res: &mut Resources,
        plan: &mut Planner,
        source: &DevelopSource,
        spots: &Spots,
    ) -> Option<Prepared> {
        let key = SourceKey {
            id: source.id,
            spots: *spots,
        };
        if res.source_key.as_ref() == Some(&key) {
            if let Some(prepared) = res.prepared {
                return Some(prepared);
            }
        }
        res.source_key = None;
        res.prepared = None;
        res.early_key = None;

        let (w, h) = (source.width(), source.height());
        let px = w as u64 * h as u64;
        let source_slot = Slot::Image(Buf::Source);
        let mut prepared = match &*source.pixels {
            Pixels::Image(image) => {
                let (format, bytes): (u32, std::borrow::Cow<[u8]>) = match image {
                    DynamicImage::ImageRgb8(rgb) => (SRC_RGB8, rgb.as_raw().into()),
                    DynamicImage::ImageRgb16(rgb) => {
                        (SRC_RGB16, bytemuck::cast_slice(rgb.as_raw()).to_vec().into())
                    }
                    other => (
                        SRC_RGB16,
                        bytemuck::cast_slice(other.to_rgb16().as_raw()).to_vec().into(),
                    ),
                };
                self.ensure(res, source_slot, bytes.len() as u64);
                self.write_padded(res.get(source_slot)?, &bytes);
                Prepared {
                    buf: Buf::Source,
                    format,
                    raw: None,
                }
            }
            Pixels::Half { data, raw } => {
                self.ensure(res, source_slot, px * HALF_PX);
                self.write_padded(res.get(source_slot)?, bytemuck::cast_slice(data));
                Prepared {
                    buf: Buf::Source,
                    format: SRC_HALF,
                    raw: *raw,
                }
            }
            Pixels::Raw(mosaic) => {
                let data = mosaic.data();
                let (sw, sh) = mosaic.sensor_size();
                if data.len() < sw as usize * sh as usize {
                    return None;
                }
                let mosaic_slot = Slot::Image(Buf::Mosaic);
                self.ensure(res, mosaic_slot, data.len() as u64 * 2);
                self.write_padded(res.get(mosaic_slot)?, bytemuck::cast_slice(data));
                let table = plan.aux(&raw_table(mosaic, [1.0; 3]));
                let params = Params {
                    width: w,
                    height: h,
                    src_width: sw,
                    src_height: sh,
                    raw_offset: table,
                    ..Default::default()
                };
                plan.push(Pass {
                    kernel: Kernel::Demosaic,
                    params,
                    src: Buf::Mosaic,
                    dst: Buf::Source,
                    img_a: Buf::NoInput,
                    img_b: Buf::NoInput,
                    groups: [w.div_ceil(16), h.div_ceil(16), 1],
                    dst_bytes: px * HALF_PX,
                    tmp_bytes: 0,
                });
                Prepared {
                    buf: Buf::Source,
                    format: SRC_HALF,
                    raw: Some(mosaic.color),
                }
            }
        };

        if !spots.is_empty() {
            prepared = plan_spots(plan, prepared, spots, w, h);
        }
        res.source_key = Some(key);
        res.prepared = Some(prepared);
        Some(prepared)
    }

    fn render(
        &self,
        source: &DevelopSource,
        s: &DevelopSettings,
        output: Output,
    ) -> Option<Rendered> {
        let (w, h) = (source.width(), source.height());
        let px = w as u64 * h as u64;
        if px == 0 || px * 16 > self.max_buffer {
            return None;
        }

        // Live previews and the detail view reuse their pooled buffers and
        // whatever is cached in them. Exports get their own, one at a
        // time, so they never evict a preview's photo.
        let channel = match output {
            Output::Preview { .. } => Some(Channel::Preview),
            Output::Detail { .. } => Some(Channel::Detail),
            Output::Rgb { .. } => None,
        };
        let mut pools = match channel {
            Some(_) => Some(self.pools.lock().ok()?),
            None => None,
        };
        let _exclusive = match channel {
            Some(_) => None,
            None => Some(self.exclusive.lock().ok()?),
        };
        let mut fresh = Resources::default();
        let res: &mut Resources = match (pools.as_deref_mut(), channel) {
            (Some(pools), Some(channel)) => pools.entry(channel).or_default(),
            _ => &mut fresh,
        };

        let scopes = ErrorScopes::push(&self.device);
        let quarters = ((s.rotation_degrees / 90.0).round() as i32).rem_euclid(4) as u32;
        let (ow, oh) = if quarters % 2 == 1 { (h, w) } else { (w, h) };
        let mut plan = Planner::new(ow, oh);
        let prepared = self.prepare_source(res, &mut plan, source, &s.spots)?;

        // Which part of the oriented photo to develop: all of it, or for
        // the detail view the part the visible region needs.
        let crop = CropGeometry::new(&s.crop, ow, oh);
        let (window, out_region) = match &output {
            Output::Detail { region, .. } => {
                let out = crop.region_pixels(*region);
                let window = crop.window_for(out, margin(s, prepared.raw.is_some(), (ow, oh)), ow, oh);
                (window, Some(out))
            }
            _ => ([0, 0, ow, oh], None),
        };
        plan.set_window(window);

        let raw = prepared.raw.is_some();
        let early_key = EarlyKey {
            lens: [s.lens_distortion, s.lens_vignetting, s.lens_ca].map(f32::to_bits),
            noise: [s.noise_luma, s.noise_color].map(f32::to_bits),
            raw: if raw {
                [s.exposure, s.temperature, s.tint].map(f32::to_bits)
            } else {
                [0; 3]
            },
            flips: (s.flip_horizontal, s.flip_vertical),
            quarters,
            window,
        };
        let early_bytes = window[2] as u64 * window[3] as u64 * HALF_PX;
        if self.ensure(res, Slot::Image(Buf::Early), early_bytes) {
            res.early_key = None;
        }
        if res.early_key != Some(early_key) || !plan.passes.is_empty() {
            plan_early(&mut plan, s, &prepared, source, w, h, quarters);
        }
        plan.cur = Buf::Early;
        plan_develop(&mut plan, s, raw);
        plan_crop(&mut plan, s, &crop, out_region);

        let mut copies = Vec::new();
        let display = cms::display_space();
        match &output {
            Output::Rgb {
                fit,
                sharpen,
                space,
                deep,
            } => {
                if let Some(edge) = fit {
                    if plan.width.max(plan.height) > *edge {
                        let (nw, nh) = crate::fit_within(plan.width, plan.height, *edge);
                        plan.resize(nw, nh, LANCZOS3, plan.cur, SRC_HALF);
                    }
                }
                if let Some((sigma, threshold)) = sharpen {
                    plan.unsharpen(*sigma, *threshold);
                }
                plan.set_lut(space);
                let words = if *deep {
                    (plan.pixels() * 3).div_ceil(2)
                } else {
                    (plan.pixels() * 3).div_ceil(4)
                };
                if *deep {
                    plan.out_rgb16();
                } else {
                    plan.out_rgb();
                }
                copies.push(ReadCopy {
                    from: Slot::of(plan.cur)?,
                    to: 0,
                    bytes: words * 4,
                });
            }
            Output::Preview {
                clip_shadows,
                clip_highlights,
            } => {
                plan.set_lut(&display);
                let pixels = plan.pixels();
                let mut params = plan.params();
                params.clip_shadows = *clip_shadows as u32;
                params.clip_highlights = *clip_highlights as u32;
                params.hist_stride = if pixels > 1_500_000 { 2 } else { 1 };
                plan.push(Pass {
                    kernel: Kernel::Histogram,
                    params,
                    src: plan.cur,
                    dst: Buf::NoOutput,
                    img_a: Buf::NoInput,
                    img_b: Buf::NoInput,
                    groups: [pixels.div_ceil(256).clamp(1, 1024) as u32, 1, 1],
                    dst_bytes: 0,
                    tmp_bytes: 0,
                });
                plan.out_bgra(params);
                copies.push(ReadCopy {
                    from: Slot::of(plan.cur)?,
                    to: 0,
                    bytes: pixels * 4,
                });
                copies.push(ReadCopy {
                    from: Slot::Hist,
                    to: (pixels * 4).next_multiple_of(16),
                    bytes: HIST_BYTES,
                });
            }
            Output::Detail { size, .. } => {
                let (tw, th) = (size.0.clamp(1, plan.width), size.1.clamp(1, plan.height));
                if (tw, th) != (plan.width, plan.height) {
                    plan.resize(tw, th, LANCZOS3, plan.cur, SRC_HALF);
                }
                plan.set_lut(&display);
                let params = plan.params();
                plan.out_bgra(params);
                copies.push(ReadCopy {
                    from: Slot::of(plan.cur)?,
                    to: 0,
                    bytes: plan.pixels() * 4,
                });
            }
        }

        let (out_w, out_h) = (plan.width, plan.height);
        let pixels = out_w as usize * out_h as usize;
        // Until this render succeeds, nothing cached is trusted.
        res.early_key = None;
        let prepared_key = res.source_key.take();
        let rendered = self.execute(res, &plan, &copies, |data| match &output {
            Output::Rgb { deep: true, .. } => {
                let samples: Vec<u16> = data[..pixels * 6]
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect();
                image::ImageBuffer::from_raw(out_w, out_h, samples).map(Rendered::Rgb16)
            }
            Output::Rgb { .. } => {
                RgbImage::from_raw(out_w, out_h, data[..pixels * 3].to_vec()).map(Rendered::Rgb)
            }
            Output::Preview { .. } => {
                let bgra = RgbaImage::from_raw(out_w, out_h, data[..pixels * 4].to_vec())?;
                let at = (pixels * 4).next_multiple_of(16);
                let words: &[u32] = bytemuck::cast_slice(&data[at..at + HIST_BYTES as usize]);
                Some(Rendered::Preview {
                    bgra,
                    histogram: histogram_from(words),
                })
            }
            Output::Detail { .. } => RgbaImage::from_raw(out_w, out_h, data[..pixels * 4].to_vec())
                .map(Rendered::Detail),
        });
        if !scopes.pop() {
            res.prepared = None;
            return None;
        }
        let rendered = rendered?;
        res.source_key = prepared_key;
        res.early_key = Some(early_key);
        Some(rendered)
    }

    fn resize(&self, source: &DynamicImage, nw: u32, nh: u32) -> Option<RgbImage> {
        let (w, h) = (source.width(), source.height());
        if w == 0 || h == 0 || nw == 0 || nh == 0 || w as u64 * nh as u64 * 16 > self.max_buffer {
            return None;
        }
        let _exclusive = self.exclusive.lock().ok()?;
        let scopes = ErrorScopes::push(&self.device);
        let mut res = Resources::default();
        let (format, bytes): (u32, std::borrow::Cow<[u8]>) = match source {
            DynamicImage::ImageRgb8(rgb) => (SRC_RGB8, rgb.as_raw().into()),
            DynamicImage::ImageRgb16(rgb) => {
                (SRC_RGB16, bytemuck::cast_slice(rgb.as_raw()).to_vec().into())
            }
            other => (SRC_RGB8, other.to_rgb8().into_raw().into()),
        };
        let slot = Slot::Image(Buf::Source);
        self.ensure(&mut res, slot, bytes.len() as u64);
        self.write_padded(res.get(slot)?, &bytes);

        let mut plan = Planner::new(w, h);
        plan.resize(nw, nh, TRIANGLE, Buf::Source, format);
        let words = (plan.pixels() * 3).div_ceil(4);
        plan.out_rgb();
        let copies = [ReadCopy {
            from: Slot::of(plan.cur)?,
            to: 0,
            bytes: words * 4,
        }];
        let pixels = nw as usize * nh as usize;
        let image = self.execute(&mut res, &plan, &copies, |data| {
            RgbImage::from_raw(nw, nh, data[..pixels * 3].to_vec())
        });
        if !scopes.pop() {
            return None;
        }
        image
    }

    fn preview_source(&self, source: &DevelopSource, max_edge: u32) -> Option<DevelopSource> {
        let (w, h) = (source.width(), source.height());
        if w == 0 || h == 0 || w as u64 * h as u64 * 16 > self.max_buffer {
            return None;
        }
        let (nw, nh) = if w.max(h) > max_edge {
            crate::fit_within(w, h, max_edge)
        } else {
            (w, h)
        };
        let _exclusive = self.exclusive.lock().ok()?;
        let scopes = ErrorScopes::push(&self.device);
        let mut res = Resources::default();
        let mut plan = Planner::new(w, h);
        let prepared = self.prepare_source(&mut res, &mut plan, source, &Spots::default())?;
        if (nw, nh) != (w, h) {
            plan.resize(nw, nh, TRIANGLE, prepared.buf, prepared.format);
        } else if prepared.format == SRC_HALF {
            plan.cur = prepared.buf;
        } else {
            // Already small and not a RAW file: nothing to do.
            return None;
        }
        let bytes = nw as u64 * nh as u64 * HALF_PX;
        let copies = [ReadCopy {
            from: Slot::of(plan.cur)?,
            to: 0,
            bytes,
        }];
        let data = self.execute(&mut res, &plan, &copies, |data| {
            Some(bytemuck::cast_slice::<u8, u32>(&data[..bytes as usize]).to_vec())
        });
        if !scopes.pop() {
            return None;
        }
        Some(DevelopSource::from_half(nw, nh, data?, prepared.raw))
    }

    /// Makes the buffers `plan` needs, records it, copies its results into
    /// the readback buffer, then waits and hands the readback to `read`.
    fn execute<T>(
        &self,
        res: &mut Resources,
        plan: &Planner,
        copies: &[ReadCopy],
        read: impl FnOnce(&[u8]) -> Option<T>,
    ) -> Option<T> {
        let mut needs: HashMap<Slot, u64> = HashMap::new();
        let mut need = |slot: Slot, bytes: u64| {
            let entry = needs.entry(slot).or_insert(0);
            *entry = (*entry).max(bytes);
        };
        for pass in &plan.passes {
            for buf in [pass.src, pass.img_a, pass.img_b] {
                if let Some(slot) = Slot::of(buf) {
                    need(slot, 16);
                }
            }
            if let Some(slot) = Slot::of(pass.dst) {
                need(slot, pass.dst_bytes);
            }
            let (tmp, tmp2, hist) = pass.kernel.scratch();
            if tmp {
                need(Slot::Tmp, pass.tmp_bytes);
            }
            if tmp2 {
                need(Slot::Tmp2, pass.tmp_bytes);
            }
            if hist {
                need(Slot::Hist, HIST_BYTES);
            }
        }
        for copy in copies {
            need(copy.from, copy.bytes);
            need(Slot::Readback, copy.to + copy.bytes);
        }
        for (slot, bytes) in needs {
            if bytes > self.max_buffer {
                return None;
            }
            self.ensure(res, slot, bytes);
        }

        let res: &Resources = res;
        let lookup = |b: Buf| -> &wgpu::Buffer {
            match Slot::of(b) {
                Some(slot) => res.get(slot).expect("made above"),
                None if b == Buf::NoOutput => &self.no_output,
                None => &self.no_input,
            }
        };
        let scratch = Scratch {
            tmp: res.get(Slot::Tmp).unwrap_or(&self.no_tmp),
            tmp2: res.get(Slot::Tmp2).unwrap_or(&self.no_tmp2),
            hist: res.get(Slot::Hist).unwrap_or(&self.no_hist),
        };
        let readback = res.get(Slot::Readback)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if plan.passes.iter().any(|p| p.kernel == Kernel::Histogram) {
            encoder.clear_buffer(scratch.hist, 0, None);
        }
        self.encode(&mut encoder, plan, &lookup, &scratch);
        for copy in copies {
            encoder.copy_buffer_to_buffer(res.get(copy.from)?, 0, readback, copy.to, copy.bytes);
        }
        self.queue.submit([encoder.finish()]);
        self.read(readback, read)
    }

    /// Records every planned pass into `encoder`.
    fn encode<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        plan: &Planner,
        lookup: &dyn Fn(Buf) -> &'a wgpu::Buffer,
        scratch: &Scratch<'a>,
    ) {
        let aux = if plan.aux.is_empty() {
            vec![0.0f32; 4]
        } else {
            plan.aux.clone()
        };
        let aux = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("aux"),
                contents: bytemuck::cast_slice(&aux),
                usage: wgpu::BufferUsages::STORAGE,
            });
        for pass in &plan.passes {
            if pass.kernel == Kernel::Copy {
                encoder.copy_buffer_to_buffer(
                    lookup(pass.src),
                    0,
                    lookup(pass.dst),
                    0,
                    pass.dst_bytes.next_multiple_of(4),
                );
                continue;
            }
            debug_assert!(
                pass.dst == Buf::NoOutput
                    || (pass.dst != pass.src && pass.dst != pass.img_a && pass.dst != pass.img_b),
                "a pass can't read and write {:?}",
                pass.dst
            );
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("params"),
                    contents: bytemuck::bytes_of(&pass.params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let resources = [
                &uniform,
                lookup(pass.src),
                lookup(pass.dst),
                scratch.tmp,
                scratch.tmp2,
                &aux,
                lookup(pass.img_a),
                lookup(pass.img_b),
                scratch.hist,
            ];
            let entries: Vec<wgpu::BindGroupEntry> = resources
                .iter()
                .enumerate()
                .map(|(binding, buffer)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: buffer.as_entire_binding(),
                })
                .collect();
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &entries,
            });
            let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            compute.set_pipeline(&self.pipelines[pass.kernel as usize]);
            compute.set_bind_group(0, &group, &[]);
            let [x, y, z] = pass.groups;
            compute.dispatch_workgroups(x, y, z);
        }
    }

    /// Waits for the GPU, then hands the mapped contents of `readback` to
    /// `f`.
    fn read<T>(&self, readback: &wgpu::Buffer, f: impl FnOnce(&[u8]) -> Option<T>) -> Option<T> {
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        rx.recv().ok()?.ok()?;
        let result = {
            let data = slice.get_mapped_range().ok()?;
            f(&data)
        };
        readback.unmap();
        result
    }
}

/// The scratch buffers bound to the fixed slots.
struct Scratch<'a> {
    tmp: &'a wgpu::Buffer,
    tmp2: &'a wgpu::Buffer,
    hist: &'a wgpu::Buffer,
}

/// Validation and out-of-memory error scopes around a piece of GPU work:
/// either kind of error means the work failed (and the caller falls back
/// to the CPU) rather than a panic.
struct ErrorScopes {
    validation: wgpu::ErrorScopeGuard,
    memory: wgpu::ErrorScopeGuard,
}

impl ErrorScopes {
    fn push(device: &wgpu::Device) -> Self {
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        Self { validation, memory }
    }

    /// Whether no error happened.
    fn pop(self) -> bool {
        let validation = pollster::block_on(self.validation.pop());
        let memory = pollster::block_on(self.memory.pop());
        if let Some(error) = validation.as_ref().or(memory.as_ref()) {
            eprintln!("GPU develop pipeline error, falling back to the CPU: {error}");
        }
        validation.is_none() && memory.is_none()
    }
}

/// Workgroups for a one-dimensional kernel over `items` (256 per group),
/// folded into two dimensions past the per-dimension limit.
fn flat_groups(items: u64) -> [u32; 3] {
    let groups = items.div_ceil(256).max(1);
    let x = groups.min(32_768);
    [x as u32, groups.div_ceil(x) as u32, 1]
}

/// Builds the list of passes, tracking which buffer holds the current
/// image, how big it is, and where it sits in its full image.
struct Planner {
    passes: Vec<Pass>,
    aux: Vec<f32>,
    cur: Buf,
    width: u32,
    height: u32,
    off_x: u32,
    off_y: u32,
    full_w: u32,
    full_h: u32,
    /// The output colour table, once chosen.
    lut: Option<(u32, u32)>,
}

impl Planner {
    fn new(width: u32, height: u32) -> Self {
        Self {
            passes: Vec::new(),
            aux: Vec::new(),
            cur: Buf::Early,
            width,
            height,
            off_x: 0,
            off_y: 0,
            full_w: width,
            full_h: height,
            lut: None,
        }
    }

    /// Works on `[x, y, width, height]` of the current full image.
    fn set_window(&mut self, [x, y, w, h]: [u32; 4]) {
        self.off_x = x;
        self.off_y = y;
        self.width = w;
        self.height = h;
    }

    fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// The ping-pong buffer the current image isn't in.
    fn next(&self) -> Buf {
        if self.cur == Buf::Ping {
            Buf::Pong
        } else {
            Buf::Ping
        }
    }

    /// Params sized and placed for the current image.
    fn params(&self) -> Params {
        let (lut_offset, lut_size) = self.lut.unwrap_or((0, 0));
        Params {
            width: self.width,
            height: self.height,
            src_width: self.width,
            src_height: self.height,
            off_x: self.off_x,
            off_y: self.off_y,
            full_w: self.full_w,
            full_h: self.full_h,
            lut_offset,
            lut_size,
            ..Default::default()
        }
    }

    fn groups(&self) -> [u32; 3] {
        [self.width.div_ceil(16), self.height.div_ceil(16), 1]
    }

    fn push(&mut self, pass: Pass) {
        self.passes.push(pass);
    }

    /// Appends `values` to the aux buffer, returning their offset.
    fn aux(&mut self, values: &[f32]) -> u32 {
        let at = self.aux.len() as u32;
        self.aux.extend_from_slice(values);
        at
    }

    /// Chooses the output colour conversion into `space`.
    fn set_lut(&mut self, space: &OutputSpace) {
        self.lut = cms::lut(space).map(|lut| (self.aux(&lut.data), lut.size));
    }

    /// A per-pixel pass from the current image into the next buffer.
    fn step(&mut self, kernel: Kernel, params: Params, img_a: Buf, img_b: Buf) {
        let dst = self.next();
        self.push(Pass {
            kernel,
            params,
            src: self.cur,
            dst,
            img_a,
            img_b,
            groups: self.groups(),
            dst_bytes: self.pixels() * HALF_PX,
            tmp_bytes: 0,
        });
        self.cur = dst;
    }

    /// image::imageops::blur of the current image into `into`.
    fn blur(&mut self, sigma: f32, into: Buf) {
        let weights = gaussian_kernel(sigma);
        let mut params = self.params();
        params.blur_radius = (weights.len() / 2) as i32;
        params.blur_offset = self.aux(&weights);
        let groups = self.groups();
        let tmp_bytes = self.pixels() * 16;
        self.push(Pass {
            kernel: Kernel::BlurH,
            params,
            src: self.cur,
            dst: Buf::NoOutput,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups,
            dst_bytes: 0,
            tmp_bytes,
        });
        self.push(Pass {
            kernel: Kernel::BlurV,
            params,
            src: Buf::NoInput,
            dst: into,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups,
            dst_bytes: self.pixels() * HALF_PX,
            tmp_bytes,
        });
    }

    /// image::imageops::unsharpen of the current image.
    fn unsharpen(&mut self, sigma: f32, threshold: i32) {
        self.blur(sigma, Buf::BlurA);
        let mut params = self.params();
        params.threshold = threshold as f32;
        self.step(Kernel::Unsharp, params, Buf::BlurA, Buf::NoInput);
    }

    /// Resamples `from` (in `format`) to `nw` x `nh` with `filter`, like
    /// `image::imageops::resize`. The result is a whole image of its own.
    fn resize(&mut self, nw: u32, nh: u32, filter: u32, from: Buf, format: u32) {
        let (w, h) = (self.width, self.height);
        let params = Params {
            width: nw,
            height: nh,
            src_width: w,
            src_height: h,
            resize_filter: filter,
            source_format: format,
            ..Default::default()
        };
        let tmp_bytes = w as u64 * nh as u64 * 16;
        self.push(Pass {
            kernel: Kernel::ResizeV,
            params,
            src: from,
            dst: Buf::NoOutput,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: [w.div_ceil(16), nh.div_ceil(16), 1],
            dst_bytes: 0,
            tmp_bytes,
        });
        let dst = if from == Buf::Ping { Buf::Pong } else { Buf::Ping };
        self.push(Pass {
            kernel: Kernel::ResizeH,
            params,
            src: Buf::NoInput,
            dst,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: [nw.div_ceil(16), nh.div_ceil(16), 1],
            dst_bytes: nw as u64 * nh as u64 * HALF_PX,
            tmp_bytes,
        });
        self.cur = dst;
        self.width = nw;
        self.height = nh;
        self.off_x = 0;
        self.off_y = 0;
        self.full_w = nw;
        self.full_h = nh;
    }

    /// Packs the current image as tightly packed 8-bit RGB bytes.
    fn out_rgb(&mut self) {
        let words = (self.pixels() * 3).div_ceil(4);
        let params = self.params();
        let dst = self.next();
        self.push(Pass {
            kernel: Kernel::OutRgb,
            params,
            src: self.cur,
            dst,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: flat_groups(words),
            dst_bytes: words * 4,
            tmp_bytes: 0,
        });
        self.cur = dst;
    }

    /// Packs the current image as tightly packed 16-bit RGB samples.
    fn out_rgb16(&mut self) {
        let words = (self.pixels() * 3).div_ceil(2);
        let params = self.params();
        let dst = self.next();
        self.push(Pass {
            kernel: Kernel::OutRgb16,
            params,
            src: self.cur,
            dst,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: flat_groups(words),
            dst_bytes: words * 4,
            tmp_bytes: 0,
        });
        self.cur = dst;
    }

    /// Converts the current image into the BGRA frame the viewer shows.
    fn out_bgra(&mut self, params: Params) {
        let dst = self.next();
        self.push(Pass {
            kernel: Kernel::OutBgra,
            params,
            src: self.cur,
            dst,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: flat_groups(self.pixels()),
            dst_bytes: self.pixels() * 4,
            tmp_bytes: 0,
        });
        self.cur = dst;
    }
}

/// The 1-D kernel `image::imageops::blur` uses for `sigma`.
fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let sigma = if sigma == 0.0 { 0.8 } else { sigma };
    let mut size = ((((sigma - 0.8) / 0.3) + 1.0) * 2.0 + 1.0).max(3.0) as usize;
    if size % 2 == 0 {
        size += 1;
    }
    let scale = 1.0 / ((2.0 * std::f32::consts::PI).sqrt() * sigma);
    let mean = (size / 2) as f32;
    let mut kernel: Vec<f32> = (0..size)
        .map(|x| (-0.5 * ((x as f32 - mean) / sigma).powf(2.0)).exp() * scale)
        .collect();
    let sum: f32 = kernel.iter().sum();
    if sum != 0.0 {
        let inv = 1.0 / sum;
        for weight in &mut kernel {
            *weight *= inv;
        }
    }
    kernel
}

fn blur_radius(sigma: f32) -> u32 {
    (gaussian_kernel(sigma).len() / 2) as u32
}

/// The demosaic and develop table the shader reads at `raw_offset`
/// (its `RAW_*` layout), with `gain` the user's white balance and
/// exposure.
fn raw_table(m: &RawMosaic, gain: [f32; 3]) -> Vec<f32> {
    let mut t = Vec::with_capacity(71);
    t.extend_from_slice(&m.black);
    t.extend_from_slice(&m.white);
    t.extend_from_slice(&m.wb);
    t.push(m.cfa.0 as f32);
    t.push(m.cfa.1 as f32);
    t.extend_from_slice(&m.cfa.2);
    t.extend(m.crop.map(|v| v as f32));
    t.extend(m.orient.map(|v| v as u8 as f32));
    t.extend_from_slice(&m.color.matrix);
    t.extend_from_slice(&m.color.clip);
    t.extend_from_slice(&gain);
    debug_assert_eq!(t.len(), 71);
    t
}

/// The develop half of `raw_table` for a demosaiced source (the demosaic
/// fields are unused then).
fn develop_table(color: &RawColor, gain: [f32; 3]) -> Vec<f32> {
    let mut t = vec![0.0; 56];
    t.extend_from_slice(&color.matrix);
    t.extend_from_slice(&color.clip);
    t.extend_from_slice(&gain);
    t
}

/// White balance and exposure for a RAW file, as gains on the camera's
/// white-balanced channels: temperature trades red against blue, tint
/// green against both, overall brightness kept; exposure in stops, with
/// the same ±2-stop range as the slider has on other photos.
fn raw_gains(s: &DevelopSettings) -> [f32; 3] {
    let t = (s.temperature / 100.0).clamp(-1.0, 1.0);
    let ti = (s.tint / 100.0).clamp(-1.0, 1.0);
    let gains = [(0.6 * t).exp2(), (0.45 * ti).exp2(), (-0.6 * t).exp2()];
    let norm = 0.25 * gains[0] + 0.5 * gains[1] + 0.25 * gains[2];
    let exposure = 2f32.powf(s.exposure / 50.0);
    gains.map(|g| g / norm * exposure)
}

/// Spot removal on the prepared photo (retouch.rs): converted to half
/// floats, then each spot in order, alternating between two buffers.
fn plan_spots(plan: &mut Planner, prepared: Prepared, spots: &Spots, w: u32, h: u32) -> Prepared {
    let px = w as u64 * h as u64;
    let full = Params {
        width: w,
        height: h,
        src_width: w,
        src_height: h,
        full_w: w,
        full_h: h,
        source_format: prepared.format,
        ..Default::default()
    };
    // A straight copy into half floats (the load kernel with nothing on).
    plan.push(Pass {
        kernel: Kernel::Load,
        params: full,
        src: prepared.buf,
        dst: Buf::SpotA,
        img_a: Buf::NoInput,
        img_b: Buf::NoInput,
        groups: [w.div_ceil(16), h.div_ceil(16), 1],
        dst_bytes: px * HALF_PX,
        tmp_bytes: 0,
    });
    let (fw, fh) = (w as f32, h as f32);
    let mut cur = Buf::SpotA;
    // Strokes are applied as circles along them, as on the CPU.
    let circles: Vec<_> = spots.as_slice().iter().flat_map(|s| s.circles(fw, fh)).collect();
    for spot in &circles {
        let target = (spot.target.0 * fw, spot.target.1 * fh);
        let source = (spot.source.0 * fw, spot.source.1 * fh);
        let r = (spot.radius * fw.max(fh)).max(1.0);
        let x0 = (target.0 - r).floor().max(0.0) as u32;
        let y0 = (target.1 - r).floor().max(0.0) as u32;
        let x1 = ((target.0 + r).ceil() as u32).min(w.saturating_sub(1));
        let y1 = ((target.1 + r).ceil() as u32).min(h.saturating_sub(1));
        if x0 > x1 || y0 > y1 {
            continue;
        }
        let next = if cur == Buf::SpotA { Buf::SpotB } else { Buf::SpotA };
        let table = plan.aux(&[
            target.0,
            target.1,
            source.0,
            source.1,
            r,
            spot.feather,
            spot.opacity,
            (spot.kind == SpotKind::Heal) as u8 as f32,
        ]);
        let params = Params {
            spot_offset: table,
            source_format: SRC_HALF,
            ..full
        };
        plan.push(Pass {
            kernel: Kernel::Copy,
            params,
            src: cur,
            dst: next,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: [0; 3],
            dst_bytes: px * HALF_PX,
            tmp_bytes: 0,
        });
        plan.push(Pass {
            kernel: Kernel::SpotRing,
            params,
            src: cur,
            dst: Buf::NoOutput,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: [1, 1, 1],
            dst_bytes: 0,
            tmp_bytes: 32 * 16,
        });
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        plan.push(Pass {
            kernel: Kernel::SpotApply,
            params: Params {
                width: bw,
                height: bh,
                off_x: x0,
                off_y: y0,
                ..params
            },
            src: cur,
            dst: next,
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: [bw.div_ceil(16), bh.div_ceil(16), 1],
            dst_bytes: px * HALF_PX,
            tmp_bytes: 32 * 16,
        });
        cur = next;
    }
    Prepared {
        buf: cur,
        format: SRC_HALF,
        raw: prepared.raw,
    }
}

/// Load (developing RAW files, with lens corrections and orientation) and
/// noise reduction, into the early buffer, over the planner's window.
fn plan_early(
    plan: &mut Planner,
    s: &DevelopSettings,
    prepared: &Prepared,
    source: &DevelopSource,
    w: u32,
    h: u32,
    quarters: u32,
) {
    let denoise = plan.full_w >= 3
        && plan.full_h >= 3
        && !(s.noise_luma < 0.5 && s.noise_color < 0.5);
    let defringe = plan.full_w >= 3
        && plan.full_h >= 3
        && crate::defringe::is_active(s.defringe_purple, s.defringe_green);
    let lens_on = crate::lens::is_active(s) && w >= 2 && h >= 2;
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let norm = (cx * cx + cy * cy).sqrt().max(1.0);
    let raw_offset = match (&*source.pixels, &prepared.raw) {
        (Pixels::Raw(mosaic), Some(_)) => plan.aux(&raw_table(mosaic, raw_gains(s))),
        (_, Some(color)) => plan.aux(&develop_table(color, raw_gains(s))),
        _ => 0,
    };
    let load = Params {
        src_width: w,
        src_height: h,
        flip_h: s.flip_horizontal as u32,
        flip_v: s.flip_vertical as u32,
        quarters,
        lens_on: lens_on as u32,
        lens_k: s.lens_distortion / 100.0 * 0.25,
        lens_ca: s.lens_ca / 100.0 * 0.01,
        lens_vig: s.lens_vignetting / 100.0 * 0.9,
        lens_norm2: norm * norm,
        lens_cx: cx,
        lens_cy: cy,
        source_format: prepared.format,
        source_linear: prepared.raw.is_some() as u32,
        raw_offset,
        ..plan.params()
    };
    // Defringe reads one buffer and writes another, and noise reduction
    // starts from `Ping` and ends in `Early`: with both, the load goes to
    // `Early` and defringe moves it to `Ping`.
    let loaded = match (defringe, denoise) {
        (true, true) => Buf::Early,
        (false, false) => Buf::Early,
        _ => Buf::Ping,
    };
    let bytes = plan.pixels() * HALF_PX;
    plan.push(Pass {
        kernel: Kernel::Load,
        params: load,
        src: prepared.buf,
        dst: loaded,
        img_a: Buf::NoInput,
        img_b: Buf::NoInput,
        groups: plan.groups(),
        dst_bytes: bytes,
        tmp_bytes: 0,
    });
    if defringe {
        // The two amounts ride in the noise slots, unused by this pass.
        let params = Params {
            dn_luma: (s.defringe_purple / 100.0).clamp(0.0, 1.0),
            dn_color: (s.defringe_green / 100.0).clamp(0.0, 1.0),
            ..plan.params()
        };
        plan.push(Pass {
            kernel: Kernel::Defringe,
            params,
            src: loaded,
            dst: if denoise { Buf::Ping } else { Buf::Early },
            img_a: Buf::NoInput,
            img_b: Buf::NoInput,
            groups: plan.groups(),
            dst_bytes: bytes,
            tmp_bytes: 0,
        });
    }
    if denoise {
        let la = (s.noise_luma / 100.0).clamp(0.0, 1.0);
        let ca = (s.noise_color / 100.0).clamp(0.0, 1.0);
        let params = Params {
            dn_luma: la,
            dn_color: ca,
            dn_luma_sigma_s: 2.0 + la,
            dn_luma_sigma_r: 3.0 + 40.0 * la,
            dn_color_sigma_r: 6.0 + 30.0 * ca,
            ..plan.params()
        };
        let tmp_bytes = plan.pixels() * 16;
        for (kernel, src, dst) in [
            (Kernel::DenoisePrep, Buf::Ping, Buf::NoOutput),
            (Kernel::DenoiseFilter, Buf::NoInput, Buf::NoOutput),
            (Kernel::DenoiseCombine, Buf::NoInput, Buf::Early),
        ] {
            plan.push(Pass {
                kernel,
                params,
                src,
                dst,
                img_a: Buf::NoInput,
                img_b: Buf::NoInput,
                groups: plan.groups(),
                dst_bytes: if dst == Buf::Early { bytes } else { 0 },
                tmp_bytes,
            });
        }
    }
}

/// The base settings after a RAW file's exposure, temperature and tint
/// moved into development.
fn base_settings(s: &DevelopSettings, raw: bool) -> (DevelopSettings, bool) {
    let (mut base, custom) = crate::split_sharpening(s);
    if raw {
        base.exposure = 0.0;
        base.temperature = 0.0;
        base.tint = 0.0;
    }
    (base, custom)
}

/// Everything after the early stage and before the crop: tone, colour,
/// detail, sharpening and finishing. Mirrors `lib::apply_cpu`.
fn plan_develop(plan: &mut Planner, s: &DevelopSettings, raw: bool) {
    let (base, custom_sharpen) = base_settings(s, raw);

    if crate::has_base_edits(&base) {
        let black_point = (base.blacks / 100.0).clamp(-1.0, 1.0) * 0.25;
        let white_point = 1.0 + (base.whites / 100.0).clamp(-1.0, 1.0) * 0.25;
        let params = Params {
            exposure_factor: 2f32.powf(base.exposure / 50.0),
            shadow_amount: base.shadows / 100.0,
            highlight_amount: base.highlights / 100.0,
            contrast_factor: 1.0 + base.contrast / 100.0,
            black_point,
            range: (white_point - black_point).max(0.05),
            temp: base.temperature / 100.0,
            tint: base.tint / 100.0,
            saturation_factor: 1.0 + base.saturation / 100.0,
            vibrance: base.vibrance / 100.0,
            ..plan.params()
        };
        plan.step(Kernel::ToneColor, params, Buf::NoInput, Buf::NoInput);
        // detail.rs
        if base.clarity.abs() > 0.5 && plan.full_w >= 3 && plan.full_h >= 3 {
            // Clarity: pushed away from (negative: towards) a wide blur.
            plan.blur(crate::detail::clarity_sigma(plan.full_w, plan.full_h), Buf::BlurA);
            let params = Params {
                sharpen_strength: crate::detail::clarity_amount(base.clarity),
                sharpen_masking: 0,
                ..plan.params()
            };
            plan.step(Kernel::Sharpen, params, Buf::BlurA, Buf::NoInput);
        }
    }

    // sharpen.rs
    if custom_sharpen && plan.full_w >= 3 && plan.full_h >= 3 && s.sharpening >= 0.5 {
        plan.blur(custom_sharpen_sigma(s), Buf::BlurA);
        let params = Params {
            sharpen_strength: s.sharpening / 100.0 * 1.6,
            sharpen_masking: (s.sharpen_masking >= 0.5) as u32,
            sharpen_threshold: s.sharpen_masking / 100.0 * 40.0,
            ..plan.params()
        };
        plan.step(Kernel::Sharpen, params, Buf::BlurA, Buf::NoInput);
    }

    plan_finishing(plan, s);
    plan_lens_blur(plan, s);
}

fn custom_sharpen_sigma(s: &DevelopSettings) -> f32 {
    crate::sharpen::sharpen_sigma(s.sharpen_radius)
}

/// Blur radii of the local adjustments (finishing/local.rs): they follow
/// the photo's size, so a layer looks the same on a preview as on the
/// full-size export.
fn local_sigmas(full_w: u32, full_h: u32, lens_blur: bool) -> (f32, f32) {
    let scale = full_w.max(full_h) as f32 / 2048.0;
    (
        ((if lens_blur { 16.0 } else { 10.0 }) * scale).max(4.0),
        (1.2 * scale).max(0.8),
    )
}

/// How far around a region the stages before the crop reach: the most a
/// detail render must develop beyond what it shows to get it exactly
/// right.
fn margin(s: &DevelopSettings, raw: bool, full: (u32, u32)) -> u32 {
    let (base, custom) = base_settings(s, raw);
    // Noise reduction reaches 6 pixels, defringe 2; the crop's sampling 2.
    let mut m = 6 + 2 + 2 + 2;
    if crate::has_base_edits(&base) {
        if base.clarity.abs() > 0.5 {
            m += blur_radius(crate::detail::clarity_sigma(full.0, full.1));
        }
    }
    if custom {
        m += blur_radius(custom_sharpen_sigma(s)) + 1;
    }
    if s.gradients.is_active() {
        let (wide, narrow) = local_sigmas(full.0, full.1, s.gradients.any_blur());
        m += blur_radius(wide) + blur_radius(narrow);
    }
    // Lens blur gathers from up to its largest radius away.
    let lens = s.lens_blur.max_radius(full.0, full.1);
    if lens >= 0.5 {
        // The gather, and the two blurs before it (colours, then lights
        // picked out of the blurred colours).
        m += lens.ceil() as u32
            + blur_radius(crate::lens_blur::surround_sigma(lens))
            + blur_radius(crate::lens_blur::pre_blur_sigma(lens))
            + 2;
    }
    m
}

/// lens_blur.rs: the CoC of each pixel into `tmp`, then the gather.
fn plan_lens_blur(plan: &mut Planner, s: &DevelopSettings) {
    let lens = s.lens_blur;
    let radius = lens.max_radius(plan.full_w, plan.full_h);
    if radius < 0.5 || plan.full_w < 2 || plan.full_h < 2 {
        return;
    }
    let focus = crate::lens_blur::focus_layer(s);
    let mut params = plan.params();
    params.lb_radius = radius;
    params.lb_boost = (lens.boost / 100.0).clamp(0.0, 1.0);
    params.lb_falloff = (lens.falloff / 100.0).clamp(0.0, 1.0);
    params.lb_shape = lens.shape.index();
    params.lb_invert = lens.invert as u32;
    params.lb_focus = plan.aux(&gradient_floats(&focus));
    let table: Vec<f32> = crate::lens_blur::sample_table(lens.shape).concat();
    params.lb_table = plan.aux(&table);
    // The softened copy samples are drawn from, and the points of light
    // picked out of it and softened the same (lens_blur.rs).
    let sigma = crate::lens_blur::pre_blur_sigma(radius);
    let groups = plan.groups();
    let image = plan.cur;
    let lights = plan.next();
    // Points of light: what stands out from a wide blur (in BlurB for
    // now). Without Boost there are none to look for.
    if params.lb_boost > 0.0 {
        plan.blur(crate::lens_blur::surround_sigma(radius), Buf::BlurB);
        plan.push(Pass {
            kernel: Kernel::LensBright,
            params,
            src: image,
            dst: lights,
            img_a: Buf::BlurB,
            img_b: Buf::NoInput,
            groups,
            dst_bytes: plan.pixels() * HALF_PX,
            tmp_bytes: 0,
        });
        plan.cur = lights;
        plan.blur(sigma, Buf::BlurB);
        plan.cur = image;
    }
    plan.blur(sigma, Buf::BlurA);
    let tmp_bytes = plan.pixels() * 16;
    plan.push(Pass {
        kernel: Kernel::LensCoc,
        params,
        src: plan.cur,
        dst: Buf::NoOutput,
        img_a: Buf::NoInput,
        img_b: Buf::NoInput,
        groups,
        dst_bytes: 0,
        tmp_bytes,
    });
    let dst = plan.next();
    plan.push(Pass {
        kernel: Kernel::LensBlur,
        params,
        src: Buf::NoInput,
        dst,
        img_a: Buf::BlurA,
        img_b: if params.lb_boost > 0.0 { Buf::BlurB } else { Buf::NoInput },
        groups,
        dst_bytes: plan.pixels() * HALF_PX,
        tmp_bytes,
    });
    plan.cur = dst;
}

/// finishing/mod.rs `before_crop`.
fn plan_finishing(plan: &mut Planner, s: &DevelopSettings) {
    let mut params = plan.params();
    if !(s.curves.is_identity() && s.parametric.is_identity()) {
        params.curves_offset = plan.aux(&curve_table(s));
        params.flags |= F_CURVES;
    }
    if !s.mixer.iter().all(|band| *band == BandAdjust::default()) {
        params.mixer_offset = plan.aux(&mixer_table(&s.mixer));
        params.flags |= F_MIXER;
    }
    if let Some(grading) = Grading::new(&s.grading) {
        params.grading_offset = plan.aux(&grading.packed());
        params.flags |= F_GRADING;
    }
    if let Some(dehaze) = Dehaze::new(s.dehaze) {
        params.dehaze = dehaze.amount();
        params.flags |= F_DEHAZE;
    }
    if s.gradients.is_active() {
        let layers: Vec<Gradient> = s
            .gradients
            .as_slice()
            .iter()
            .filter(|g| g.is_visible())
            .map(|g| g.on_display(s))
            .collect();
        let table: Vec<f32> = layers.iter().flat_map(gradient_floats).collect();
        params.gradient_offset = plan.aux(&table);
        params.gradient_count = layers.len() as u32;
        params.flags |= F_LOCAL;
        let (wide, narrow) = local_sigmas(plan.full_w, plan.full_h, s.gradients.any_blur());
        if layers.iter().any(|g| g.adjust.clarity != 0.0 || g.adjust.blur != 0.0) {
            plan.blur(wide, Buf::BlurA);
            params.flags |= F_WIDE;
        }
        if layers.iter().any(|g| g.adjust.sharpening != 0.0) {
            plan.blur(narrow, Buf::BlurB);
            params.flags |= F_NARROW;
        }
    }
    if params.flags != 0 {
        let wide = if params.flags & F_WIDE != 0 { Buf::BlurA } else { Buf::NoInput };
        let narrow = if params.flags & F_NARROW != 0 { Buf::BlurB } else { Buf::NoInput };
        plan.step(Kernel::Finish, params, wide, narrow);
    }
}

/// The composed tone curves, per channel, as `CURVE_STEPS` floats: the
/// parametric sliders, then the master point curve, then the channel's
/// own (finishing/curves.rs, without its 8-bit tables).
fn curve_table(s: &DevelopSettings) -> Vec<f32> {
    let n = CURVE_STEPS;
    let mut shaped = vec![0.0f32; n];
    let mut last = 0.0f32;
    for (i, out) in shaped.iter_mut().enumerate() {
        // Forced to rise, like ParametricCurve::lut.
        last = last.max(s.parametric.evaluate(i as f32 / (n - 1) as f32));
        *out = last;
    }
    let curves = &s.curves;
    let mut table = Vec::with_capacity(n * 3);
    for channel in [&curves.red, &curves.green, &curves.blue] {
        table.extend(
            shaped
                .iter()
                .map(|x| channel.evaluate(curves.master.evaluate(*x))),
        );
    }
    table
}

/// The crop (crop.rs) worked out for an oriented photo of `width` x
/// `height`.
struct CropGeometry {
    out_w: u32,
    out_h: u32,
    rotated: bool,
    /// Plain crops: the top-left corner kept.
    left: u32,
    top: u32,
    /// Straightened crops: the crop rectangle's corner on the canvas, and
    /// the turn back onto the photo.
    x0: f32,
    y0: f32,
    sin: f32,
    cos: f32,
    canvas: (f32, f32),
    identity: bool,
    persp: (f32, f32),
    crop: rustyedit_core::Crop,
}

impl CropGeometry {
    fn new(crop: &rustyedit_core::Crop, width: u32, height: u32) -> Self {
        let (fw, fh) = (width as f32, height as f32);
        let mut geometry = Self {
            out_w: width,
            out_h: height,
            rotated: false,
            left: 0,
            top: 0,
            x0: 0.0,
            y0: 0.0,
            sin: 0.0,
            cos: 1.0,
            canvas: (fw, fh),
            identity: crop.is_identity(),
            persp: crop.perspective_strength(),
            crop: *crop,
        };
        if geometry.identity {
            return geometry;
        }
        let (x, y, w, h) = crop.rect;
        let (x0, y0) = (x * fw, y * fh);
        geometry.out_w = ((w * fw).round() as u32).max(1);
        geometry.out_h = ((h * fh).round() as u32).max(1);
        if crop.angle == 0.0 && !crop.has_perspective() {
            geometry.left = (x0.round() as u32).min(width - 1);
            geometry.top = (y0.round() as u32).min(height - 1);
            geometry.out_w = geometry.out_w.min(width - geometry.left);
            geometry.out_h = geometry.out_h.min(height - geometry.top);
        } else {
            let (sin, cos) = (-crop.angle).to_radians().sin_cos();
            geometry.rotated = true;
            geometry.x0 = x0;
            geometry.y0 = y0;
            geometry.sin = sin;
            geometry.cos = cos;
        }
        geometry
    }

    /// `region` (fractions of the cropped photo) in output pixels,
    /// `[x, y, width, height]`.
    fn region_pixels(&self, (l, t, w, h): (f32, f32, f32, f32)) -> [u32; 4] {
        let (ow, oh) = (self.out_w as f32, self.out_h as f32);
        let x0 = ((l.clamp(0.0, 1.0) * ow).floor() as u32).min(self.out_w - 1);
        let y0 = ((t.clamp(0.0, 1.0) * oh).floor() as u32).min(self.out_h - 1);
        let x1 = (((l + w).clamp(0.0, 1.0) * ow).ceil() as u32).clamp(x0 + 1, self.out_w);
        let y1 = (((t + h).clamp(0.0, 1.0) * oh).ceil() as u32).clamp(y0 + 1, self.out_h);
        [x0, y0, x1 - x0, y1 - y0]
    }

    /// The part of the oriented photo the output region `[x, y, w, h]`
    /// reads, grown by `margin` and kept on the photo.
    fn window_for(&self, [x, y, w, h]: [u32; 4], margin: u32, width: u32, height: u32) -> [u32; 4] {
        let (min, max) = if self.rotated {
            let corners = [
                (x as f32, y as f32),
                ((x + w) as f32, y as f32),
                (x as f32, (y + h) as f32),
                ((x + w) as f32, (y + h) as f32),
            ];
            let mut min = (f32::MAX, f32::MAX);
            let mut max = (f32::MIN, f32::MIN);
            for (ox, oy) in corners {
                let (sx, sy) = self.crop.source_point(
                    (self.x0 + ox, self.y0 + oy),
                    self.canvas.0,
                    self.canvas.1,
                );
                min = (min.0.min(sx), min.1.min(sy));
                max = (max.0.max(sx), max.1.max(sy));
            }
            (min, max)
        } else {
            let (lx, ly) = ((self.left + x) as f32, (self.top + y) as f32);
            ((lx, ly), (lx + w as f32, ly + h as f32))
        };
        let m = margin as f32;
        let x0 = (min.0 - m).floor().clamp(0.0, width as f32 - 1.0) as u32;
        let y0 = (min.1 - m).floor().clamp(0.0, height as f32 - 1.0) as u32;
        let x1 = ((max.0 + m).ceil().max(0.0) as u32).clamp(x0 + 1, width);
        let y1 = ((max.1 + m).ceil().max(0.0) as u32).clamp(y0 + 1, height);
        [x0, y0, x1 - x0, y1 - y0]
    }
}

/// crop.rs, then finishing/mod.rs `after_crop`, producing the whole
/// cropped photo or (detail renders) just `region` of it.
fn plan_crop(
    plan: &mut Planner,
    s: &DevelopSettings,
    crop: &CropGeometry,
    region: Option<[u32; 4]>,
) {
    let mut params = plan.params();
    let (out_w, out_h) = (crop.out_w, crop.out_h);
    if crop.rotated {
        params.crop_rotate = 1;
        params.crop_x0 = crop.x0;
        params.crop_y0 = crop.y0;
        params.crop_sin = crop.sin;
        params.crop_cos = crop.cos;
        (params.persp_x, params.persp_y) = crop.persp;
    } else {
        params.crop_left = crop.left;
        params.crop_top = crop.top;
    }
    if let Some(vignette) = Vignette::new(s, out_w, out_h) {
        params.flags |= F_VIGNETTE;
        params.vignette = vignette.amount;
        (params.vig_cx, params.vig_cy) = vignette.centre;
        params.vig_round = vignette.round;
        params.vig_inner = vignette.inner;
        params.vig_outer = vignette.outer;
    }
    if let Some(grain) = Grain::new(s, out_w, out_h) {
        params.flags |= F_GRAIN;
        params.grain_strength = grain.strength;
        params.grain_cell = grain.cell;
    }
    if crop.identity && params.flags == 0 && region.is_none() {
        return;
    }
    let [rx, ry, rw, rh] = region.unwrap_or([0, 0, out_w, out_h]);
    params.width = rw;
    params.height = rh;
    params.off_x = rx;
    params.off_y = ry;
    params.src_off_x = plan.off_x;
    params.src_off_y = plan.off_y;
    params.full_w = plan.full_w;
    params.full_h = plan.full_h;
    let dst = plan.next();
    plan.push(Pass {
        kernel: Kernel::CropFinish,
        params,
        src: plan.cur,
        dst,
        img_a: Buf::NoInput,
        img_b: Buf::NoInput,
        groups: [rw.div_ceil(16), rh.div_ceil(16), 1],
        dst_bytes: rw as u64 * rh as u64 * HALF_PX,
        tmp_bytes: 0,
    });
    plan.cur = dst;
    plan.width = rw;
    plan.height = rh;
    plan.off_x = rx;
    plan.off_y = ry;
    plan.full_w = out_w;
    plan.full_h = out_h;
}

/// Per band: centre hue, span above and below it (as `band_weight`
/// measures them), then the hue, saturation and luminance shifts.
fn mixer_table(mixer: &rustyedit_core::ColorMixer) -> Vec<f32> {
    let all = ColorBand::ALL;
    let diff = |a: f32, b: f32| ((a - b + 540.0) % 360.0) - 180.0;
    all.iter()
        .flat_map(|band| {
            let ix = band.index();
            let prev = all[(ix + all.len() - 1) % all.len()].hue();
            let next = all[(ix + 1) % all.len()].hue();
            let centre = band.hue();
            let adjust = &mixer[ix];
            [
                centre,
                diff(next, centre).abs(),
                diff(centre, prev).abs(),
                adjust.hue,
                adjust.saturation,
                adjust.luminance,
            ]
        })
        .collect()
}

/// A gradient layer as the shader reads it (`mask_at`, `local_adjust`).
fn gradient_floats(g: &Gradient) -> [f32; GRADIENT_STRIDE] {
    let mut out = [0.0; GRADIENT_STRIDE];
    let a = &g.adjust;
    out[0] = match g.kind {
        GradientKind::Linear => 0.0,
        GradientKind::Radial => 1.0,
        GradientKind::Brush => 2.0,
        GradientKind::Range => 3.0,
    };
    out[1..10].copy_from_slice(&[
        g.start.0,
        g.start.1,
        g.end.0,
        g.end.1,
        g.radius.0,
        g.radius.1,
        g.feather,
        g.invert as u8 as f32,
        g.opacity,
    ]);
    out[10..20].copy_from_slice(&[
        a.exposure,
        a.contrast,
        a.highlights,
        a.shadows,
        a.temperature,
        a.tint,
        a.saturation,
        a.dehaze,
        a.clarity,
        a.sharpening,
    ]);
    if let Some(grading) = Grading::new(&a.grading) {
        out[20] = 1.0;
        out[21..31].copy_from_slice(&grading.packed());
    }
    // Brightness and colour ranges, then the brush's dabs.
    out[31] = g.dab_count() as f32;
    out[32..40].copy_from_slice(&[
        g.luma_range.0,
        g.luma_range.1,
        g.luma_feather,
        g.hue_on as u8 as f32,
        g.hue,
        g.hue_width,
        g.hue_feather,
        a.blur,
    ]);
    for (i, dab) in g.dabs().enumerate() {
        out[40 + i * 4..44 + i * 4].copy_from_slice(&[
            dab.x,
            dab.y,
            dab.radius,
            dab.erase as u8 as f32,
        ]);
    }
    out
}

fn histogram_from(words: &[u32]) -> Histogram {
    let levels = |at: usize| -> [u32; 256] { words[at..at + 256].try_into().unwrap() };
    let grid = |at: usize| words[at..at + WAVE_CELLS].to_vec();
    Histogram {
        red: levels(0),
        green: levels(256),
        blue: levels(512),
        luma: levels(768),
        scopes: Scopes {
            wave_red: grid(H_WAVES),
            wave_green: grid(H_WAVES + WAVE_CELLS),
            wave_blue: grid(H_WAVES + 2 * WAVE_CELLS),
            wave_luma: grid(H_WAVES + 3 * WAVE_CELLS),
            vector: words[H_VECTOR..HIST_WORDS].to_vec(),
        },
    }
}
