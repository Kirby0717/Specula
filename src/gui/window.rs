use super::{GlyphAtlas, GpuContext, Renderer};

use std::sync::Arc;

use winit::{
    application::ApplicationHandler, event::WindowEvent, window::Window,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
}
impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}
const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, 1.0],
        uv: [0.0, 0.0],
    },
    Vertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, -1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [1.0, -1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [-1.0, -1.0],
        uv: [0.0, 1.0],
    },
];

struct App {
    window: Arc<Window>,
    gpu: GpuContext,
    atlas: GlyphAtlas,
    renderer: Renderer,
}
impl App {
    fn new(window: Window) -> Self {
        let window = Arc::new(window);
        let mut gpu = GpuContext::new(&window);
        gpu.configure_surface();

        let mut atlas = GlyphAtlas::new(&gpu, 32.0);
        let test_data = include_str!(
            "../../「Ｒｅ：ゼロから始める異世界生活」[2024-01-28_20h57m].csv"
        );
        for c in test_data.chars() {
            let _ = atlas.get_or_insert(&gpu, c);
        }

        let renderer = Renderer::new(&gpu, &atlas /*terminal*/);

        App {
            gpu,
            window,
            atlas,
            renderer,
        }
    }
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.size = new_size;
        self.gpu.configure_surface();
    }
}

#[derive(Default)]
pub struct AppHandler {
    app: Option<App>,
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("test")
            .with_inner_size(winit::dpi::PhysicalSize {
                width: 1024,
                height: 1024,
            });
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
                app.renderer.render(&app.window, &app.gpu, &app.atlas);
            }
            WindowEvent::Resized(size) => {
                app.resize(size);
            }
            /*WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::*;
                if event.state.is_pressed()
                    && event.physical_key == PhysicalKey::Code(KeyCode::Space)
                {
                    for _ in 0..1000 {
                        if let Some(c) = app.s.pop() {
                            let _ = app.atlas.get_or_insert(&app.gpu, c);
                            app.window.request_redraw();
                        }
                    }
                }
            }*/
            _ => {}
        }
    }
}
