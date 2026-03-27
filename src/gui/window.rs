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

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct Selection {
    anchor: Point,
    end: Point,
    kind: SelectionKind,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
enum SelectionKind {
    #[default]
    Character,
    Word,
    Line,
}

struct App {
    window: Arc<Window>,
    gpu: GpuContext,
    atlas: GlyphAtlas,
    renderer: Renderer,
    terminal: Terminal,

    modifiers: Modifiers,
    cursor_position: [f64; 2],
    mouse_state: MouseButton,
    selection: Option<Selection>,
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
            cursor_position: [0.0, 0.0],
            mouse_state: MouseButton::default(),
            selection: None,
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
    fn convert_mouse_button_event(&self, button: MouseButton) -> MouseEvent {
        let Point { row, col } = self.cursor_cell();
        MouseEvent {
            kind: if self.mouse_state.is_pressed() {
                MouseEventKind::Press
            }
            else {
                MouseEventKind::Release
            },
            button,
            col,
            row,
            shift: self.modifiers.state().shift_key(),
            alt: self.modifiers.state().alt_key(),
            ctrl: self.modifiers.state().control_key(),
        }
    }
    fn convert_mouse_cursor_event(&self, button: MouseButton) -> MouseEvent {
        let Point { row, col } = self.cursor_cell();
        MouseEvent {
            kind: MouseEventKind::Motion,
            button,
            col,
            row,
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
    fn cursor_cell(&self) -> Point {
        let [cell_width, cell_height] = self.atlas.cell_size();
        let row = (self.cursor_position[1] / cell_height as f64) as usize;
        let col = (self.cursor_position[0] / cell_width as f64) as usize;
        let col = col.min(self.terminal.grid_cols() - 1);
        let row = row.min(self.terminal.grid_rows() - 1);
        Point { row, col }
    }
    fn cursor_boundary_cell(&self) -> Point {
        let [cell_width, cell_height] = self.atlas.cell_size();
        let row = (self.cursor_position[1] / cell_height as f64) as usize;
        let col = (self.cursor_position[0] / cell_width as f64 + 0.5) as usize;
        let col = col.min(self.terminal.grid_cols());
        let row = row.min(self.terminal.grid_rows() - 1);
        Point { row, col }
    }
    fn selection_text(&self) -> Option<String> {
        let Selection { anchor, end, .. } = self.selection.clone()?;
        Some(self.terminal.get_text(anchor, end))
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
                let selection = app.selection.clone().unwrap_or_default();
                let anchor = app
                    .terminal
                    .buffer_index_to_viewport_row(selection.anchor.row)
                    * app.terminal.grid_cols() as isize
                    + selection.anchor.col as isize;
                let end = app
                    .terminal
                    .buffer_index_to_viewport_row(selection.end.row)
                    * app.terminal.grid_cols() as isize
                    + selection.end.col as isize;
                let grid_size = (app.terminal.grid_rows()
                    * app.terminal.grid_cols())
                    as isize;
                let anchor = anchor.clamp(0, grid_size) as u32;
                let end = end.clamp(0, grid_size) as u32;

                let selection_range = [anchor.min(end), anchor.max(end)];
                app.renderer.render(
                    &app.window,
                    &app.gpu,
                    &mut app.atlas,
                    &app.terminal,
                    selection_range,
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
                app.cursor_position = [position.x, position.y];

                // ターミナル選択
                if app.selection.is_some() && app.mouse_state.is_pressed() {
                    let cursor_cell = app.cursor_boundary_cell();
                    let row = app
                        .terminal
                        .viewport_row_to_buffer_index(cursor_cell.row);
                    let col = cursor_cell.col;
                    if let Some(selection) = &mut app.selection {
                        selection.end = Point { row, col };
                    }
                    app.window.request_redraw();
                }
                // PTYへ送信
                else {
                    let mode = app.terminal.mode();
                    if mode.contains(TerminalMode::MOUSE_MOTION)
                        || (mode.contains(TerminalMode::MOUSE_DRAG)
                            && app.mouse_state.is_pressed())
                    {
                        let sgr_mode = app
                            .terminal
                            .mode()
                            .contains(TerminalMode::MOUSE_SGR);
                        let data = app
                            .convert_mouse_cursor_event(app.mouse_state)
                            .encode_mouse(&app.modifiers, sgr_mode);
                        app.terminal.write(&data);
                    }
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
                app.mouse_state = button;

                // PTYへ送信
                if app.mouse_report_active()
                    && !app.modifiers.state().shift_key()
                {
                    app.terminal.write(&data);
                }
                // ターミナルを選択
                else {
                    let cursor_cell = app.cursor_boundary_cell();
                    let row = app
                        .terminal
                        .viewport_row_to_buffer_index(cursor_cell.row);
                    let col = cursor_cell.col;
                    let point = Point { row, col };
                    if state.is_pressed() {
                        let selection = Selection {
                            anchor: point,
                            end: point,
                            kind: SelectionKind::Character,
                        };
                        app.selection = Some(selection);
                    }
                    app.window.request_redraw();
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
                    println!("{event:?}");
                if event.state.is_pressed() {
                    use winit::keyboard::*;

                    // コピー
                    if app.modifiers.state().control_key()
                        && app.modifiers.state().shift_key()
                        && event.physical_key
                            == PhysicalKey::Code(KeyCode::KeyC)
                    {
                        if let Some(selection) = app.selection_text()
                            && let Ok(mut clipboard) = arboard::Clipboard::new()
                        {
                            log::info!("copy!! : {selection}");
                            let _ = clipboard.set_text(selection);
                        }
                        return;
                    }
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

                    // 特殊キー
                    if let Key::Named(key) = event.logical_key
                        && app.terminal.write_key(app.modifiers, key)
                    {
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
                        if app.modifiers.state().alt_key() {
                            app.terminal.write(b"\x1b");
                        }
                        app.terminal.write(&[byte]);
                    }
                    // 通常キー
                    else if let Some(text) = &event.text {
                        if app.modifiers.state().alt_key() {
                            app.terminal.write(b"\x1b");
                        }
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
