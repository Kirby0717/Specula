use std::sync::Arc;

use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::EventLoop,
    window::Window,
};

pub fn run_app() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    event_loop.run_app(&mut AppHandler::default())?;
    Ok(())
}

struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
}
impl GpuContext {
    fn new(window: &Arc<Window>) -> Self {
        // デバイスとキューの作成
        let instance =
            wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        use pollster::FutureExt as _;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .block_on()
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .block_on()
            .unwrap();

        // 描画先のウィンドウ情報の取得
        let size = window.inner_size();
        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        GpuContext {
            device,
            queue,
            size,
            surface,
            surface_format,
        }
    }
    fn configure_surface(&mut self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            view_formats: vec![self.surface_format],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width.max(1),
            height: self.size.height.max(1),
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoNoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }
}

struct App {
    window: Arc<Window>,
    gpu: GpuContext,
}
impl App {
    fn new(window: Window) -> Self {
        let window = Arc::new(window);
        let mut gpu = GpuContext::new(&window);
        gpu.configure_surface();
        App { gpu, window }
    }
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.size = new_size;
        self.gpu.configure_surface();
    }
    fn render(&mut self) {
        // スワップチェーンのバックバッファの取得
        let surface_texture = self
            .gpu
            .surface
            .get_current_texture()
            .expect("failed to acquire next swapchain texture");
        let texture_view =
            surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor {
                    // Without add_srgb_suffix() the image we will be working with
                    // might not be "gamma correct".
                    //format: Some(self.surface_format),
                    ..Default::default()
                });

        let mut encoder =
            self.gpu.device.create_command_encoder(&Default::default());

        // レンダーパスの設定
        let render_pass =
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
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
            });

        drop(render_pass);

        self.gpu.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
    }
}

#[derive(Default)]
pub struct AppHandler {
    app: Option<App>,
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title("test");
        let window = event_loop
            .create_window(window_attributes)
            .expect("ウィンドウの作成に失敗しました");
        self.app = Some(App::new(window));
    }
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(app) = &mut self.app
        else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                app.render();
            }
            WindowEvent::Resized(size) => {
                app.resize(size);
            }
            _ => {}
        }
    }
}
