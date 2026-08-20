pub struct Wgpu {
    _instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub timestamp_queries: bool,
    pub adapter: String,
}

impl Wgpu {
    pub async fn new(canvas: web_sys::OffscreenCanvas) -> Result<Self, String> {
        let width = canvas.width();
        let height = canvas.height();
        if width == 0 || height == 0 {
            return Err("CANVAS_SIZE".into());
        }
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
            .map_err(|_| "SURFACE")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| "WEBGPU_UNAVAILABLE")?;
        let required_features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let info = adapter.get_info();
        let adapter_name = if info.name.is_empty() {
            format!("{:?} · {:?}", info.device_type, info.backend)
        } else {
            format!(
                "{} · {:?} · {:?}",
                info.name, info.device_type, info.backend
            )
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features,
                ..Default::default()
            })
            .await
            .map_err(|_| "DEVICE")?;
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("SURFACE")?;
        let format = config.format;
        surface.configure(&device, &config);
        Ok(Self {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            format,
            width,
            height,
            timestamp_queries: required_features.contains(wgpu::Features::TIMESTAMP_QUERY),
            adapter: adapter_name,
        })
    }
}
