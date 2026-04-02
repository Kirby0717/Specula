mod event;
mod mouse;
mod selection;

pub use mouse::{MouseButton, MouseEvent, MouseEventKind};
pub use selection::{Selection, SelectionKind};

use super::{App, TermEvent};
use crate::config::Config;

use winit::{
    application::ApplicationHandler, event::WindowEvent,
    event_loop::EventLoopProxy, window::Window,
};

pub struct AppHandler {
    app: Option<App>,
    config: Config,
    proxy: EventLoopProxy<TermEvent>,
}
impl AppHandler {
    pub fn new(proxy: EventLoopProxy<TermEvent>, config: Config) -> Self {
        AppHandler {
            app: None,
            config,
            proxy,
        }
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
        self.app = Some(App::new(
            window,
            Box::new(event_loop.owned_display_handle()),
            &self.proxy,
            &self.config,
        ));
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
                event::handle_redraw(app);
            }
            WindowEvent::Resized(size) => {
                event::handle_resize(app, size);
            }
            WindowEvent::Ime(ime) => {
                event::handle_ime(app, ime);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                app.modifiers = new_modifiers;
            }
            WindowEvent::CursorMoved { position, .. } => {
                event::handle_cursor_moved(app, position);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                event::handle_mouse_input(app, state, button);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                event::handle_mouse_wheel(app, delta);
            }
            WindowEvent::Focused(focused) => {
                event::handle_focused(app, focused)
            }
            WindowEvent::KeyboardInput { event, .. } => {
                event::handle_keyboard(app, event);
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
