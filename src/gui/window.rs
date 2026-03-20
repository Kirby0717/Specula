use super::{GlyphAtlas, GpuContext, Renderer};
use crate::core::Terminal;

use std::{sync::Arc, thread::JoinHandle};

use winit::{
    application::ApplicationHandler, event::WindowEvent,
    event_loop::EventLoopProxy, window::Window,
};

pub enum TermEvent {
    PtyOutput,
}

struct App {
    window: Arc<Window>,
    gpu: GpuContext,
    atlas: GlyphAtlas,
    renderer: Renderer,
    terminal: Terminal,
    pty_handle: JoinHandle<()>,
}
impl App {
    fn new(window: Window, proxy: &EventLoopProxy<TermEvent>) -> Self {
        let window = Arc::new(window);
        let mut gpu = GpuContext::new(&window);
        gpu.configure_surface();

        let atlas = GlyphAtlas::new(&gpu, 32.0);

        let window_size = window.inner_size();
        let rows = window_size.height / atlas.cell_height;
        let cols = window_size.width / atlas.cell_width;
        let proxy = proxy.clone();
        let notify = Box::new(move || {
            proxy.send_event(TermEvent::PtyOutput).ok();
        });
        let (terminal, pty_handle) = Terminal::new(
            rows as usize,
            cols as usize,
            1_000_000,
            "bash",
            notify,
        )
        .expect("ターミナルの起動に失敗しました");

        let renderer = Renderer::new(&gpu, &atlas, &terminal);

        App {
            gpu,
            window,
            atlas,
            renderer,
            terminal,
            pty_handle,
        }
    }
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.size = new_size;
        self.gpu.configure_surface();
    }
}

pub struct AppHandler {
    app: Option<App>,
    proxy: EventLoopProxy<TermEvent>,
}
impl AppHandler {
    pub fn new(proxy: EventLoopProxy<TermEvent>) -> Self {
        AppHandler { app: None, proxy }
    }
}
impl ApplicationHandler<TermEvent> for AppHandler {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("test")
            .with_inner_size(winit::dpi::PhysicalSize {
                width: 1920,
                height: 1080,
            });
        let window = event_loop
            .create_window(window_attributes)
            .expect("ウィンドウの作成に失敗しました");
        self.app = Some(App::new(window, &self.proxy));
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
                app.renderer.render(
                    &app.window,
                    &app.gpu,
                    &mut app.atlas,
                    &app.terminal,
                );
            }
            WindowEvent::Resized(size) => {
                app.resize(size);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed()
                    && let Some(text) = event.text
                {
                    app.terminal.write(text.as_bytes());
                }
            }
            _ => {}
        }
    }
    fn user_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        event: TermEvent,
    ) {
        let Some(app) = &mut self.app
        else {
            return;
        };

        match event {
            TermEvent::PtyOutput => {
                app.terminal.process_pty_output();
                app.window.request_redraw();
            }
        }
    }
}
