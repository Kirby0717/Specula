use super::{GlyphAtlas, GpuContext, Renderer};
use crate::core::{Point, Terminal, TerminalMode};

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

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
enum MouseButton {
    Left,
    Middle,
    Right,
    #[default]
    None,
    ScrollUp,
    ScrollDown,
}
impl MouseButton {
    fn is_pressed(self) -> bool {
        MouseButton::None != self
    }
}
impl TryFrom<winit::event::MouseButton> for MouseButton {
    type Error = ();
    fn try_from(value: winit::event::MouseButton) -> Result<Self, Self::Error> {
        use winit::event::MouseButton as WinitMouseButton;
        Ok(match value {
            WinitMouseButton::Left => MouseButton::Left,
            WinitMouseButton::Middle => MouseButton::Middle,
            WinitMouseButton::Right => MouseButton::Right,
            _ => return Err(()),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseEventKind {
    Press,
    Release,
    Motion,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct MouseEvent {
    kind: MouseEventKind,
    button: MouseButton,
    col: usize, // 0-indexed
    row: usize, // 0-indexed
    shift: bool,
    alt: bool,
    ctrl: bool,
}
impl MouseEvent {
    fn encode_button(&self, modifiers: &Modifiers) -> u8 {
        let mut code: u8 = match self.button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::None => 3,
            MouseButton::ScrollUp => 64,
            MouseButton::ScrollDown => 65,
        };

        if modifiers.state().shift_key() {
            code += 4;
        }
        if modifiers.state().alt_key() {
            code += 8;
        }
        if modifiers.state().control_key() {
            code += 16;
        }

        if self.kind == MouseEventKind::Motion {
            code += 32;
        }

        code
    }
    fn encode_x10(&self, modifiers: &Modifiers) -> Vec<u8> {
        let mut button = self.encode_button(modifiers);
        if self.kind == MouseEventKind::Release {
            button = (button & !0b11) | 3; // 下位2bitを3にする
        }

        vec![
            0x1b,
            b'[',
            b'M',
            button + 32,
            (self.col as u8) + 1 + 32,
            (self.row as u8) + 1 + 32,
        ]
    }
    fn encode_sgr(&self, modifiers: &Modifiers) -> Vec<u8> {
        let button = self.encode_button(modifiers);
        let terminator = match self.kind {
            MouseEventKind::Release => 'm',
            _ => 'M',
        };

        format!(
            "\x1b[<{};{};{}{}",
            button,
            self.col + 1,
            self.row + 1,
            terminator,
        )
        .into_bytes()
    }
    fn encode_mouse(&self, modifiers: &Modifiers, sgr_mode: bool) -> Vec<u8> {
        if sgr_mode {
            self.encode_sgr(modifiers)
        }
        else {
            self.encode_x10(modifiers)
        }
    }
}

struct App {
    window: Arc<Window>,
    gpu: GpuContext,
    atlas: GlyphAtlas,
    renderer: Renderer,
    terminal: Terminal,

    modifiers: Modifiers,
    mouse_cell: Point,
    mouse_state: MouseButton,
}
impl App {
    fn new(window: Window, proxy: &EventLoopProxy<TermEvent>) -> Self {
        let window = Arc::new(window);
        let mut gpu = GpuContext::new(&window);
        gpu.configure_surface();

        let atlas = GlyphAtlas::new(&gpu, 24.0);
        let cell_size = atlas.cell_size();
        let cell_width = cell_size[0];
        let cell_height = cell_size[1];

        let window_size = window.inner_size();
        let rows = window_size.height / cell_height;
        let cols = window_size.width / cell_width;
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
            &["--no-history"],
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
            mouse_cell: Point::default(),
            mouse_state: MouseButton::default(),
        }
    }
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.size = new_size;
        self.gpu.configure_surface();

        let [cell_width, cell_height] = self.atlas.cell_size();
        let rows = (new_size.height / cell_height) as usize;
        let cols = (new_size.width / cell_width) as usize;

        self.terminal.resize(rows, cols);
        self.renderer.resize(&self.gpu, &self.atlas, rows, cols);
    }
    fn convert_mouse_button_event(
        &mut self,
        button: MouseButton,
    ) -> MouseEvent {
        self.mouse_state = button;
        MouseEvent {
            kind: if self.mouse_state.is_pressed() {
                MouseEventKind::Press
            }
            else {
                MouseEventKind::Release
            },
            button,
            col: self.mouse_cell.col,
            row: self.mouse_cell.row,
            shift: self.modifiers.state().shift_key(),
            alt: self.modifiers.state().alt_key(),
            ctrl: self.modifiers.state().control_key(),
        }
    }
    fn convert_mouse_cursor_event(
        &mut self,
        button: MouseButton,
    ) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Motion,
            button,
            col: self.mouse_cell.col,
            row: self.mouse_cell.row,
            shift: self.modifiers.state().shift_key(),
            alt: self.modifiers.state().alt_key(),
            ctrl: self.modifiers.state().control_key(),
        }
    }
    fn mouse_report_active(&self) -> bool {
        self.terminal.mode().intersects(
            TerminalMode::MOUSE_REPORT
                | TerminalMode::MOUSE_DRAG
                | TerminalMode::MOUSE_MOTION,
        )
    }
    fn pixel_to_cell(&self, x: f64, y: f64) -> Point {
        let [cell_width, cell_height] = self.atlas.cell_size();
        let row = (y / cell_height as f64) as usize;
        let col = (x / cell_width as f64) as usize;
        let col = col.min(self.terminal.grid_cols() - 1);
        let row = row.min(self.terminal.grid_rows() - 1);
        Point { row, col }
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
            WindowEvent::CursorMoved { position, .. } => {
                let new_cell = app.pixel_to_cell(position.x, position.y);
                if app.mouse_cell == new_cell {
                    return;
                }
                app.mouse_cell = new_cell;

                let mode = app.terminal.mode();
                if mode.contains(TerminalMode::MOUSE_MOTION)
                    || (mode.contains(TerminalMode::MOUSE_DRAG)
                        && app.mouse_state.is_pressed())
                {
                    let sgr_mode =
                        app.terminal.mode().contains(TerminalMode::MOUSE_SGR);
                    let data = app
                        .convert_mouse_cursor_event(app.mouse_state)
                        .encode_mouse(&app.modifiers, sgr_mode);
                    app.terminal.write(&data);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Ok(button) = button.try_into()
                else {
                    return;
                };

                let button = if state.is_pressed() {
                    button
                }
                else {
                    MouseButton::None
                };
                let sgr_mode =
                    app.terminal.mode().contains(TerminalMode::MOUSE_SGR);
                let data = app
                    .convert_mouse_button_event(button)
                    .encode_mouse(&app.modifiers, sgr_mode);
                if app.mouse_report_active() {
                    app.terminal.write(&data);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta;
                let MouseScrollDelta::LineDelta(_, delta) = delta
                else {
                    return;
                };
                let delta = delta as i32;

                // アプリへ送信
                if app.mouse_report_active() {
                    let button = match delta {
                        ..0 => MouseButton::ScrollDown,
                        0 => return,
                        1.. => MouseButton::ScrollUp,
                    };
                    let count = delta.unsigned_abs();

                    let sgr_mode =
                        app.terminal.mode().contains(TerminalMode::MOUSE_SGR);
                    for _ in 0..count {
                        let data = app
                            .convert_mouse_button_event(button)
                            .encode_mouse(&app.modifiers, sgr_mode);
                        app.terminal.write(&data);
                    }
                }
                // ターミナルへ送信
                else {
                    app.terminal.scroll((delta * 3) as isize);
                }

                app.window.request_redraw();
            }
            WindowEvent::Focused(focused) => {
                if app.terminal.mode().contains(TerminalMode::FOCUS_REPORT) {
                    let seq = if focused { "\x1b[I" } else { "\x1b[O" };
                    app.terminal.write(seq.as_bytes());
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    use winit::keyboard::*;

                    // ペースト
                    if app.modifiers.state().control_key()
                        && app.modifiers.state().shift_key()
                        && event.physical_key
                            == PhysicalKey::Code(KeyCode::KeyV)
                    {
                        if let Ok(mut clipboard) = arboard::Clipboard::new()
                            && let Ok(text) = clipboard.get_text()
                        {
                            app.terminal.paste(&text);
                        }
                        return;
                    }

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
                                NamedKey::ArrowUp
                                | NamedKey::ArrowDown
                                | NamedKey::ArrowRight
                                | NamedKey::ArrowLeft => {
                                    app.terminal.write_arrow(*named_key);
                                    return;
                                }
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
