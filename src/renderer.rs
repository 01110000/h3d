//! wgpu renderer that rasterizes the gaussians into an offscreen texture.

use crate::splats::Splat;
use glam::{Mat4, Vec4Swizzles};
use wgpu::util::DeviceExt;

pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    viewport_focal: [f32; 4],
}

pub struct SplatRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    splats: Vec<Splat>,
    sorted: Vec<Splat>,
    depths: Vec<f32>,
    order: Vec<u32>,
    counts: Vec<u32>,
    last_sorted_view: Option<Mat4>,
}

impl SplatRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, splats: Vec<Splat>) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("h3d splat shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("h3d uniforms layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("h3d uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("h3d uniforms bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("h3d pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Splat>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32x3,
                2 => Float32x3,
                3 => Float32x4,
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("h3d splat pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(instance_layout)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("h3d splats"),
            contents: bytemuck::cast_slice(&splats),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let n = splats.len();
        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            uniform_buffer,
            bind_group,
            instance_buffer,
            sorted: splats.clone(),
            splats,
            depths: vec![0.0; n],
            order: vec![0; n],
            counts: vec![0; 1 << 16],
            last_sorted_view: None,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Re-orders the splats back to front (16-bit counting sort on view depth).
    fn sort(&mut self, view: Mat4) {
        if self.last_sorted_view == Some(view) {
            return;
        }
        self.last_sorted_view = Some(view);

        let row = view.row(2);
        let (mut min, mut max) = (f32::MAX, f32::MIN);
        for (d, s) in self.depths.iter_mut().zip(&self.splats) {
            // View-space z: more negative is further away.
            *d = row.xyz().dot(s.center.into()) + row.w;
            min = min.min(*d);
            max = max.max(*d);
        }
        let scale = 65535.0 / (max - min).max(1e-6);
        self.counts.fill(0);
        for (i, d) in self.depths.iter().enumerate() {
            let key = ((d - min) * scale) as u32;
            self.order[i] = key;
            self.counts[key as usize] += 1;
        }
        let mut acc = 0;
        for c in self.counts.iter_mut() {
            let n = *c;
            *c = acc;
            acc += n;
        }
        for (i, key) in self.order.iter().enumerate() {
            let slot = &mut self.counts[*key as usize];
            self.sorted[*slot as usize] = self.splats[i];
            *slot += 1;
        }
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.sorted));
    }

    pub fn render(&mut self, target: &wgpu::Texture, view: Mat4, fov_y: f32) {
        let (w, h) = (target.width() as f32, target.height() as f32);
        let proj = Mat4::perspective_rh(fov_y, w / h, 0.05, 100.0);
        let focal = h / (2.0 * (fov_y * 0.5).tan());
        let uniforms = Uniforms {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            viewport_focal: [w, h, focal, focal],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        self.sort(view);

        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("h3d") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("h3d splat pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..4, 0..self.splats.len() as u32);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}

pub fn create_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("h3d target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
