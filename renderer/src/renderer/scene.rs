use wgpu::util::DeviceExt;

use crate::{
    camera::Camera,
    message::WheelMessage,
    renderer::{self, GpuResources},
};

pub struct UniformResource {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// Simple uniform data.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug, Default)]
pub struct FrameMetadata {
    pub mouse_move: [f32; 2],
    pub mouse_click: [f32; 2],
    pub resolution: [f32; 2],
    time: f32,
    _padding0: f32,
    pub camera_position: [f32; 4],
}

impl FrameMetadata {
    pub fn new(dimension: ultraviolet::Vec2) -> Self {
        FrameMetadata {
            resolution: dimension.into(),
            mouse_move: [std::f32::MIN, std::f32::MIN],
            mouse_click: [std::f32::MIN, std::f32::MIN],
            _padding0: 0.0,
            camera_position: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        }
    }

    pub fn set_camera_position(&mut self, position: ultraviolet::Vec3) {
        self.camera_position = [position.x, position.y, position.z, 1.0];
    }

    pub fn update_dimension(&mut self, dimension: ultraviolet::Vec2) {
        self.resolution = dimension.into();
    }

    pub fn create_uniform_resource(self, device: &wgpu::Device) -> UniformResource {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("frame metadata uniform buffer"),
            contents: bytemuck::cast_slice(&[self][..]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        UniformResource {
            buffer,
            bind_group_layout,
            bind_group,
        }
    }
}

pub fn mesh_vertex_layout() -> [wgpu::VertexBufferLayout<'static>; 4] {
    [
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        },
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            }],
        },
        wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            }],
        },
        wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        },
    ]
}

pub trait Scene: Sized {
    fn setup(renderer_context: &renderer::RendererContext, resources: &mut GpuResources) -> Self;
    fn bind_groups(&self) -> &[wgpu::BindGroup];
    fn handle_mouse_click(&mut self, x: f32, y: f32);
    fn handle_zoom(&mut self, message: &WheelMessage);
    fn handle_orbit(&mut self, delta_x: f32, delta_y: f32);
    fn set_camera_depth_range(&mut self, near: f32, far: f32);
    fn set_camera_look_at(&mut self, eye: ultraviolet::Vec3, center: ultraviolet::Vec3);

    fn frame_metadata_mut(&mut self) -> Option<&mut FrameMetadata> {
        None
    }

    fn camera_mut(&mut self) -> Option<&mut Camera> {
        None
    }

    fn view_proj(&mut self) -> Option<[[f32; 4]; 4]> {
        Some(self.camera_mut()?.view_proj)
    }

    fn uniform_buffers(&self) -> Option<&[wgpu::Buffer]> {
        None
    }

    fn resize(&mut self, width: f64, height: f64, _scale_factor: f64, queue: &wgpu::Queue) {
        let fm_copy = if let Some(fm) = self.frame_metadata_mut() {
            let dimension = ultraviolet::Vec2::new(width as f32, height as f32);
            fm.update_dimension(dimension);
            *fm
        } else {
            return;
        };

        let view_proj_copy = if let Some(cam) = self.camera_mut() {
            cam.update_aspect_ratio(width as f32 / height as f32);
            cam.view_proj
        } else {
            return;
        };

        if let Some(buffers) = self.uniform_buffers() {
            if buffers.len() >= 2 {
                queue.write_buffer(&buffers[0], 0, bytemuck::cast_slice(&[fm_copy]));
                queue.write_buffer(&buffers[1], 0, bytemuck::cast_slice(&[view_proj_copy]));
            }
        }
    }

    fn update(
        &mut self,
        renderer_context: &renderer::RendererContext,
        _resources: &mut GpuResources,
    ) {
        let camera_position = if let Some(cam) = self.camera_mut() {
            cam.position()
        } else {
            return;
        };

        let fm_copy = if let Some(fm) = self.frame_metadata_mut() {
            let time = (js_sys::Date::now() as f32) * 0.001;
            fm.time = time;
            fm.set_camera_position(camera_position);
            *fm
        } else {
            return;
        };

        let view_proj_copy = if let Some(cam) = self.camera_mut() {
            cam.view_proj
        } else {
            return;
        };

        if let Some(buffers) = self.uniform_buffers() {
            if buffers.len() >= 2 {
                renderer_context.queue.write_buffer(
                    &buffers[0],
                    0,
                    bytemuck::cast_slice(&[fm_copy]),
                );
                renderer_context.queue.write_buffer(
                    &buffers[1],
                    0,
                    bytemuck::cast_slice(&[view_proj_copy]),
                );
            }
        }
    }
}
