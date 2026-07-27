use wgpu::util::DeviceExt;

use crate::{
    camera::Camera,
    render_data::RenderData,
    renderer::{self, GpuResources},
};

pub struct UniformResource {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

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
        Self {
            resolution: dimension.into(),
            mouse_move: [f32::MIN; 2],
            mouse_click: [f32::MIN; 2],
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

pub trait Scene: Sized {
    fn setup(
        context: &renderer::RendererContext,
        resources: &mut GpuResources,
        data: &mut RenderData,
    ) -> Self;
    fn bind_groups(&self) -> &[wgpu::BindGroup];
    fn handle_mouse_click(&mut self, x: f32, y: f32);
    fn handle_zoom(&mut self, delta_y: f32);
    fn handle_orbit(&mut self, dx: f32, dy: f32);
    fn set_camera_depth_range(&mut self, near: f32, far: f32);
    fn set_camera_look_at(&mut self, eye: ultraviolet::Vec3, center: ultraviolet::Vec3);
    fn frame_metadata_mut(&mut self) -> Option<&mut FrameMetadata> {
        None
    }
    fn camera_mut(&mut self) -> Option<&mut Camera> {
        None
    }
    fn uniform_buffers(&self) -> Option<[&wgpu::Buffer; 2]> {
        None
    }
    fn resize(&mut self, width: f64, height: f64, _: f64, queue: &wgpu::Queue) {
        if let Some(f) = self.frame_metadata_mut() {
            f.update_dimension(ultraviolet::Vec2::new(width as f32, height as f32));
        }
        if let Some(c) = self.camera_mut() {
            c.update_aspect_ratio(width as f32 / height as f32)
        }
        self.write_uniforms(queue);
    }
    fn update(&mut self, context: &renderer::RendererContext) {
        let position = match self.camera_mut() {
            Some(c) => c.position(),
            None => return,
        };
        if let Some(f) = self.frame_metadata_mut() {
            f.time = js_sys::Date::now() as f32 * 0.001;
            f.set_camera_position(position)
        }
        self.write_uniforms(&context.queue);
    }
    fn write_uniforms(&mut self, queue: &wgpu::Queue) {
        let frame = self.frame_metadata_mut().copied();
        let view = self.camera_mut().map(|c| c.view_proj);
        if let (Some(f), Some(v), Some([frame_buffer, camera_buffer])) =
            (frame, view, self.uniform_buffers())
        {
            queue.write_buffer(frame_buffer, 0, bytemuck::bytes_of(&f));
            queue.write_buffer(camera_buffer, 0, bytemuck::bytes_of(&v));
        }
    }
}
