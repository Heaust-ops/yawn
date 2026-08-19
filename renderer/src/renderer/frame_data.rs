#[cfg(target_arch = "wasm32")]
use wgpu::util::DeviceExt;

use crate::render_data::camera::Camera;
#[cfg(target_arch = "wasm32")]
use crate::renderer::{self, PipelineLibrary};

#[cfg(target_arch = "wasm32")]
pub struct UniformResource {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug, Default)]
pub struct FrameMetadata {
    pub resolution: [f32; 2],
    time: f32,
    _padding0: f32,
    pub camera_position: [f32; 4],
}
impl FrameMetadata {
    #[cfg(target_arch = "wasm32")]
    pub fn new(dimension: ultraviolet::Vec2) -> Self {
        Self {
            resolution: dimension.into(),
            camera_position: [0., 0., 0., 1.],
            ..Default::default()
        }
    }
    pub fn set_camera_position(&mut self, p: ultraviolet::Vec3) {
        self.camera_position = [p.x, p.y, p.z, 1.];
    }
    pub fn update_dimension(&mut self, d: ultraviolet::Vec2) {
        self.resolution = d.into();
    }
    #[cfg(target_arch = "wasm32")]
    pub fn create_uniform_resource(self, device: &wgpu::Device) -> UniformResource {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("frame metadata"),
            contents: bytemuck::bytes_of(&self),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame layout"),
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
            label: Some("frame group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        UniformResource {
            buffer,
            bind_group,
            bind_group_layout,
        }
    }
}

pub(crate) struct FrameData {
    uniform_buffers: [wgpu::Buffer; 2],
    bind_groups: [wgpu::BindGroup; 2],
    metadata: FrameMetadata,
    camera: Camera,
}

impl FrameData {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn new(
        context: &renderer::RendererContext,
        resources: &mut PipelineLibrary,
    ) -> Self {
        let dimensions = ultraviolet::Vec2::new(
            context.surface_config.width as f32,
            context.surface_config.height as f32,
        );
        let mut metadata = FrameMetadata::new(dimensions);
        let camera = Camera::new(dimensions.x / dimensions.y);
        metadata.set_camera_position(camera.position());
        let frame = metadata.create_uniform_resource(&context.device);
        let camera_uniform = camera.create_uniform_resource(&context.device);
        resources
            .set_bind_group_layouts(&[frame.bind_group_layout, camera_uniform.bind_group_layout]);
        Self {
            uniform_buffers: [frame.buffer, camera_uniform.buffer],
            bind_groups: [frame.bind_group, camera_uniform.bind_group],
            metadata,
            camera,
        }
    }

    pub(crate) fn bind_groups(&self) -> &[wgpu::BindGroup] {
        &self.bind_groups
    }

    pub(crate) fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub(crate) fn frustum_planes(
        &mut self,
    ) -> Result<[[f32; 4]; 6], crate::render_data::camera::FrustumError> {
        self.camera.frustum_planes()
    }

    pub(crate) fn resize(&mut self, width: f64, height: f64, queue: &wgpu::Queue) {
        self.metadata
            .update_dimension(ultraviolet::Vec2::new(width as f32, height as f32));
        self.camera
            .update_aspect_ratio(width as f32 / height as f32);
        self.write_uniforms(queue);
    }

    pub(crate) fn update(&mut self, queue: &wgpu::Queue) {
        self.metadata.time = js_sys::Date::now() as f32 * 0.001;
        self.metadata.set_camera_position(self.camera.position());
        self.write_uniforms(queue);
    }

    fn write_uniforms(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.uniform_buffers[0],
            0,
            bytemuck::bytes_of(&self.metadata),
        );
        queue.write_buffer(
            &self.uniform_buffers[1],
            0,
            bytemuck::bytes_of(&self.camera.view_proj),
        );
    }
}
