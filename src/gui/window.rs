use super::{GlyphAtlas, GpuContext, Renderer};
use crate::core::Terminal;

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{Ime, Modifiers, WindowEvent},
    event_loop::EventLoopProxy,
    window::Window,
};

pub enum TermEvent {
    PtyOutput,
    PtyExit,
}

struct App {
    window: Arc<Window>,
    gpu: GpuContext,
    atlas: GlyphAtlas,
    renderer: Renderer,
    terminal: Terminal,

    modifiers: Modifiers,
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
        let notify_proxy = proxy.clone();
        let notify = Box::new(move || {
            notify_proxy.send_event(TermEvent::PtyOutput).ok();
        });
        let exit_proxy = proxy.clone();
        let on_exit = Box::new(move || {
            exit_proxy.send_event(TermEvent::PtyExit).ok();
        });
        let terminal = Terminal::new(
            rows as usize,
            cols as usize,
            1_000_000,
            "nu",
            //"powershell",
            //"bash",
            //"cmd",
            notify,
            on_exit,
        )
        .expect("ターミナルの起動に失敗しました");

        let renderer = Renderer::new(&gpu, &atlas, &terminal);

        App {
            gpu,
            window,
            atlas,
            renderer,
            terminal,

            modifiers: Modifiers::default(),
        }
    }
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.size = new_size;
        self.gpu.configure_surface();

        let rows = (new_size.height / self.atlas.cell_height) as usize;
        let cols = (new_size.width / self.atlas.cell_width) as usize;

        self.terminal.resize(rows, cols);
        self.renderer.resize(&self.gpu, &self.atlas, rows, cols);
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
            .with_title("Specula")
            .with_inner_size(winit::dpi::PhysicalSize {
                width: 1920,
                height: 1080,
            });
        let window = event_loop
            .create_window(window_attributes)
            .expect("ウィンドウの作成に失敗しました");
        window.set_ime_allowed(true);
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
            WindowEvent::Ime(ime) => {
                match ime {
                    Ime::Commit(text) => {
                        app.terminal.write(text.as_bytes());
                    }
                    Ime::Preedit(_text, _cursor) => {
                        // TODO: プレビュー表示
                    }
                    _ => {}
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                app.modifiers = new_modifiers;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::*;
                let MouseScrollDelta::LineDelta(_, delta) = delta
                else {
                    return;
                };

                app.terminal.scroll((delta * 3.0) as isize);
                app.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    use winit::keyboard::*;

                    // Ctrl
                    if app.modifiers.state().control_key()
                        && let Key::Character(c) = &event.logical_key
                        && let Some(ch) = c.chars().next()
                    {
                        let byte = match ch {
                            'a'..='z' => ch as u8 - b'a' + 1,
                            'A'..='Z' => ch as u8 - b'A' + 1,
                            // Ctrl+[ → ESC (0x1B)
                            '[' | '{' => 0x1b,
                            // Ctrl+\ → 0x1C (SIGQUIT)
                            '\\' | '|' => 0x1c,
                            // Ctrl+] → 0x1D
                            ']' | '}' => 0x1d,
                            _ => return,
                        };
                        app.terminal.write(&[byte]);
                        return;
                    }
                    // 特殊キー
                    'a: {
                        if let Key::Named(named_key) = &event.logical_key {
                            let data = match named_key {
                                NamedKey::Enter => "\r",
                                NamedKey::Backspace => "\x7f",
                                NamedKey::Escape => "\x1b",
                                NamedKey::Tab => "\t",
                                NamedKey::ArrowUp => "\x1b[A",
                                NamedKey::ArrowDown => "\x1b[B",
                                NamedKey::ArrowRight => "\x1b[C",
                                NamedKey::ArrowLeft => "\x1b[D",
                                _ => break 'a,
                            };
                            app.terminal.write(data.as_bytes());
                            return;
                        }
                    }
                    // 通常キー
                    if let Some(text) = &event.text {
                        app.terminal.write(text.as_bytes());
                    }
                }
            }
            _ => {}
        }
    }
    fn user_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
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
            TermEvent::PtyExit => {
                event_loop.exit();
            }
        }
    }
}
