use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;

use crate::gpu::Wgpu;
use crate::gpu_resource::GpuPass;
use crate::graph::{ColorAttachment, DepthAttachment};
use crate::render_data::RenderData;
use crate::store::{Loadout, Store};

pub struct RenderLoop {
    playing: Cell<bool>,
    fps: Cell<u32>,
    started: Cell<bool>,
    frame: Cell<u32>,
    elapsed: Cell<f64>,
    last: Cell<f64>,
}

impl RenderLoop {
    pub fn new() -> Self {
        Self {
            playing: Cell::new(true),
            fps: Cell::new(60),
            started: Cell::new(false),
            frame: Cell::new(0),
            elapsed: Cell::new(0.0),
            last: Cell::new(js_sys::Date::now()),
        }
    }

    pub fn play(&self) {
        self.last.set(js_sys::Date::now());
        self.playing.set(true);
    }

    pub fn pause(&self) {
        self.playing.set(false);
    }

    pub fn set_fps(&self, fps: u32) -> Result<(), &'static str> {
        if !(1..=1000).contains(&fps) {
            return Err("FPS");
        }
        self.fps.set(fps);
        Ok(())
    }

    pub fn start(
        self: &Rc<Self>,
        gpu: Rc<RefCell<Option<Wgpu>>>,
        store: Rc<RefCell<Store>>,
        data: Rc<RefCell<RenderData>>,
    ) {
        if self.started.replace(true) {
            return;
        }
        let control = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let started = js_sys::Date::now();
                if control.playing.get() {
                    let delta = (started - control.last.replace(started)) / 1000.0;
                    let elapsed = control.elapsed.get() + delta;
                    control.elapsed.set(elapsed);
                    let frame = control.frame.get().wrapping_add(1);
                    control.frame.set(frame);
                    let skip = {
                        let mut data = data.borrow_mut();
                        data.update_info(delta as f32, frame, elapsed as f32, control.fps.get());
                        data.skip_render()
                    };
                    if !skip {
                        if let (Some(gpu), Some(loadout)) =
                            (gpu.borrow_mut().as_mut(), store.borrow_mut().active_mut())
                        {
                            let _ = gpu.render(loadout, &data.borrow());
                        }
                    }
                }
                let target = 1000.0 / f64::from(control.fps.get());
                let wait = (target - (js_sys::Date::now() - started)).max(0.0) as u32;
                TimeoutFuture::new(if control.playing.get() { wait } else { 50 }).await;
            }
        });
    }
}

impl Wgpu {
    fn render(&mut self, loadout: &mut Loadout, data: &RenderData) -> Result<(), String> {
        for buffer in loadout.resources.buffers.values() {
            self.queue.write_buffer(
                &buffer.buffer,
                0,
                data.bytes(&buffer.source).ok_or("ROWS_UNKNOWN")?,
            );
        }
        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture().map_err(|_| "SURFACE")?
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(_) => return Err("SURFACE".into()),
        };
        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        for (pass, compiled) in loadout.graph.passes.iter().zip(&loadout.resources.passes) {
            match compiled {
                GpuPass::Compute {
                    pipeline,
                    bind_groups,
                } => {
                    let mut command = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some(&pass.id),
                        timestamp_writes: None,
                    });
                    command.set_pipeline(pipeline);
                    for (group, bind_group) in bind_groups {
                        command.set_bind_group(*group, bind_group, &[]);
                    }
                    command.dispatch_workgroups(
                        pass.dispatch[0],
                        pass.dispatch[1],
                        pass.dispatch[2],
                    );
                }
                GpuPass::Render(bundle) => {
                    let colors = pass
                        .color
                        .iter()
                        .map(|attachment| {
                            Ok(Some(wgpu::RenderPassColorAttachment {
                                view: view(
                                    &loadout.resources,
                                    &surface_view,
                                    &attachment.resource,
                                )?,
                                depth_slice: None,
                                resolve_target: None,
                                ops: color_ops(attachment)?,
                            }))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let depth = pass
                        .depth
                        .as_ref()
                        .map(|attachment| {
                            Ok::<_, String>(wgpu::RenderPassDepthStencilAttachment {
                                view: view(
                                    &loadout.resources,
                                    &surface_view,
                                    &attachment.resource,
                                )?,
                                depth_ops: Some(depth_ops(attachment)?),
                                stencil_ops: None,
                            })
                        })
                        .transpose()?;
                    let mut command = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some(&pass.id),
                        color_attachments: &colors,
                        depth_stencil_attachment: depth,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    command.execute_bundles([bundle]);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        output.present();
        Ok(())
    }
}

fn view<'a>(
    resources: &'a crate::gpu_resource::GpuResources,
    surface: &'a wgpu::TextureView,
    id: &str,
) -> Result<&'a wgpu::TextureView, String> {
    if id == "canvas" {
        Ok(surface)
    } else {
        resources
            .texture_view(id)
            .ok_or_else(|| "GRAPH_ATTACHMENT".into())
    }
}

fn color_ops(attachment: &ColorAttachment) -> Result<wgpu::Operations<wgpu::Color>, String> {
    let clear = attachment.clear.as_slice();
    Ok(wgpu::Operations {
        load: match attachment.load.as_str() {
            "load" => wgpu::LoadOp::Load,
            "clear" => wgpu::LoadOp::Clear(wgpu::Color {
                r: f64::from(clear.first().copied().unwrap_or(0.0)),
                g: f64::from(clear.get(1).copied().unwrap_or(0.0)),
                b: f64::from(clear.get(2).copied().unwrap_or(0.0)),
                a: f64::from(clear.get(3).copied().unwrap_or(1.0)),
            }),
            _ => return Err("GRAPH_LOAD_OP".into()),
        },
        store: store_op(&attachment.store)?,
    })
}

fn depth_ops(attachment: &DepthAttachment) -> Result<wgpu::Operations<f32>, String> {
    Ok(wgpu::Operations {
        load: match attachment.load.as_str() {
            "load" => wgpu::LoadOp::Load,
            "clear" => wgpu::LoadOp::Clear(attachment.clear),
            _ => return Err("GRAPH_LOAD_OP".into()),
        },
        store: store_op(&attachment.store)?,
    })
}

fn store_op(value: &str) -> Result<wgpu::StoreOp, String> {
    match value {
        "store" => Ok(wgpu::StoreOp::Store),
        "discard" => Ok(wgpu::StoreOp::Discard),
        _ => Err("GRAPH_STORE_OP".into()),
    }
}
