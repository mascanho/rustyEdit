//! wgpu compute backend for the develop pipeline. Mirrors the CPU modules
//! (`tone`, `color`, `detail`) in `shader.wgsl`. Everything here is
//! best-effort: any failure (no adapter, image too big for the device, a
//! validation error) yields `None` and the caller falls back to the CPU path.

use std::sync::OnceLock;

use bytemuck::{Pod, Zeroable};
use image::RgbImage;
use rustyedit_core::DevelopSettings;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
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

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    tone_color: wgpu::ComputePipeline,
    blur_h: wgpu::ComputePipeline,
    blur_v: wgpu::ComputePipeline,
    max_buffer: u64,
    name: String,
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
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rustyedit-develop"),
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .ok()?;

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("develop"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("develop-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1, true),
                storage(2, false),
                storage(3, false),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("develop-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        // Surface any shader/pipeline validation problem as "no GPU" rather
        // than a panic on first render.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let tone_color = pipeline("tone_color");
        let blur_h = pipeline("blur_h");
        let blur_v = pipeline("blur_v_unsharp");
        if pollster::block_on(scope.pop()).is_some() {
            return None;
        }

        let max_buffer = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size) as u64;
        Some(Self {
            device,
            queue,
            layout,
            tone_color,
            blur_h,
            blur_v,
            max_buffer,
            name: info.name,
        })
    }

    fn buffer(&self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        })
    }

    fn run(&self, image: &RgbImage, settings: &DevelopSettings) -> Option<RgbImage> {
        let (width, height) = image.dimensions();
        let pixels = width as u64 * height as u64;
        let packed_bytes = pixels * 4;
        let float_bytes = pixels * 16;
        if pixels == 0 || float_bytes > self.max_buffer {
            return None;
        }

        let packed: Vec<u32> = image
            .pixels()
            .map(|p| p[0] as u32 | (p[1] as u32) << 8 | (p[2] as u32) << 16 | 0xff00_0000)
            .collect();

        let storage = wgpu::BufferUsages::STORAGE;
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("input"),
                contents: bytemuck::cast_slice(&packed),
                usage: storage,
            });
        let ping = self.buffer(packed_bytes, storage | wgpu::BufferUsages::COPY_SRC);
        let pong = self.buffer(packed_bytes, storage | wgpu::BufferUsages::COPY_SRC);
        let scratch = self.buffer(float_bytes.max(16), storage);
        let readback = self.buffer(
            packed_bytes,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );

        let base = base_params(width, height, settings);
        let mut detail_passes = Vec::new();
        if settings.clarity.abs() > 0.5 {
            let amount = (settings.clarity / 100.0) * 0.6;
            detail_passes.push((12.0f32, (amount * 40.0) as i32));
        }
        if settings.sharpening.abs() > 0.5 {
            let amount = (settings.sharpening / 100.0).max(0.0) * 0.8;
            detail_passes.push((1.0f32, (amount * 40.0) as i32));
        }

        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let uniform = |params: Params| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                })
        };
        let bind = |params: &wgpu::Buffer, src: &wgpu::Buffer, dst: &wgpu::Buffer| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: src.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: dst.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: scratch.as_entire_binding(),
                    },
                ],
            })
        };
        let groups = (width.div_ceil(16), height.div_ceil(16));
        let mut dispatch = |pipeline: &wgpu::ComputePipeline, group: &wgpu::BindGroup| {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        };

        // Stage 1: tone + colour, input -> ping. Each later stage reads
        // `cur` and writes `next`, then they swap.
        let params = uniform(base);
        dispatch(&self.tone_color, &bind(&params, &input, &ping));
        let (mut cur, mut next) = (&ping, &pong);
        for (sigma, threshold) in detail_passes {
            let params = uniform(Params {
                sigma,
                radius: (sigma * 3.0).ceil() as i32,
                threshold: threshold as f32,
                ..base
            });
            let group = bind(&params, cur, next);
            dispatch(&self.blur_h, &group);
            dispatch(&self.blur_v, &group);
            std::mem::swap(&mut cur, &mut next);
        }
        encoder.copy_buffer_to_buffer(cur, 0, &readback, 0, packed_bytes);
        self.queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        rx.recv().ok()?.ok()?;
        if pollster::block_on(scope.pop()).is_some() {
            return None;
        }

        let data = slice.get_mapped_range().ok()?;
        let words: &[u32] = bytemuck::cast_slice(&data);
        let mut out = Vec::with_capacity(pixels as usize * 3);
        for w in words {
            out.extend_from_slice(&[*w as u8, (*w >> 8) as u8, (*w >> 16) as u8]);
        }
        drop(data);
        readback.unmap();
        RgbImage::from_raw(width, height, out)
    }
}

fn base_params(width: u32, height: u32, s: &DevelopSettings) -> Params {
    let black_point = (s.blacks / 100.0).clamp(-1.0, 1.0) * 0.25;
    let white_point = 1.0 + (s.whites / 100.0).clamp(-1.0, 1.0) * 0.25;
    Params {
        width,
        height,
        exposure_factor: 2f32.powf(s.exposure / 50.0),
        shadow_amount: s.shadows / 100.0,
        highlight_amount: s.highlights / 100.0,
        contrast_factor: 1.0 + s.contrast / 100.0,
        black_point,
        range: (white_point - black_point).max(0.05),
        temp: s.temperature / 100.0,
        tint: s.tint / 100.0,
        saturation_factor: 1.0 + s.saturation / 100.0,
        vibrance: s.vibrance / 100.0,
        sigma: 1.0,
        radius: 0,
        threshold: 0.0,
        _pad: 0.0,
    }
}

/// Runs tone, colour and detail on the GPU. `None` means "use the CPU path".
pub fn apply(image: &RgbImage, settings: &DevelopSettings) -> Option<RgbImage> {
    gpu()?.run(image, settings)
}
