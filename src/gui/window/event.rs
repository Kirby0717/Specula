use super::{MouseButton, Selection, SelectionKind};
use crate::{
    core::{Point, TerminalMode},
    gui::App,
};

use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Ime, KeyEvent, MouseScrollDelta},
};

use std::time::Instant;

pub(super) fn handle_redraw(app: &mut App) {
    // 選択範囲の計算
    let grid = app.terminal.active_grid();
    let grid_rows = grid.grid_rows() as isize;
    let grid_cols = grid.grid_cols() as isize;
    let grid_size = grid_rows * grid_cols;
    let into_grid_index = |Point { row, col }: Point| -> u32 {
        let col = col.min(grid.grid_cols()) as isize;
        let viewport_row = grid.buffer_index_to_viewport_row(row);
        (viewport_row * grid_cols + col).clamp(0, grid_size) as u32
    };
    let (begin, end) = app.selection_range().unwrap_or_default();
    let begin = into_grid_index(begin);
    let end = into_grid_index(end);

    app.renderer.render(
        &app.window,
        &app.gpu,
        &mut app.atlas,
        &app.terminal,
        [begin, end],
    );
}
pub(super) fn handle_ime(app: &mut App, ime: Ime) {
    match ime {
        Ime::Commit(text) => {
            app.terminal.write(text.as_bytes());
        }
        Ime::Preedit(text, cursor) => {
            app.renderer.set_preedit(text, cursor);
            app.window.request_redraw();
        }
        _ => {}
    }
}
pub(super) fn handle_cursor_moved(
    app: &mut App,
    position: PhysicalPosition<f64>,
) {
    app.cursor_position = [position.x, position.y];

    // ターミナル選択
    if let Some(selection) = app.selection
        && app.mouse_state.is_pressed()
    {
        let cursor_cell = if selection.kind == SelectionKind::Word {
            app.cursor_cell()
        }
        else {
            app.cursor_boundary_cell()
        };
        let row = app
            .terminal
            .active_grid()
            .viewport_row_to_buffer_index(cursor_cell.row);
        let col = cursor_cell.col;
        app.selection = Some(Selection {
            end: Point { row, col },
            ..selection
        });
        app.snap_selection();
        app.window.request_redraw();
    }
    // PTYへ送信
    else {
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
}
pub(super) fn handle_mouse_input(
    app: &mut App,
    state: ElementState,
    button: winit::event::MouseButton,
) {
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
    let now = Instant::now();

    // PTYへ送信
    if app.mouse_report_active() && !app.modifiers.state().shift_key() {
        let sgr_mode = app.terminal.mode().contains(TerminalMode::MOUSE_SGR);
        let data = app
            .convert_mouse_button_event(button)
            .encode_mouse(&app.modifiers, sgr_mode);
        app.terminal.write(&data);
        app.selection = None;
    }
    // ターミナルを選択
    else {
        let cursor_cell = app.cursor_boundary_cell();
        let row = app
            .terminal
            .active_grid()
            .viewport_row_to_buffer_index(cursor_cell.row);
        let col = cursor_cell.col;
        let point = Point { row, col };
        if state.is_pressed() {
            let kind = if now.duration_since(app.last_click_time)
                < App::MULTI_CLICK_INTERVAL
                && let Some(selection) = &app.selection
            {
                match selection.kind {
                    SelectionKind::Character => SelectionKind::Word,
                    SelectionKind::Word => SelectionKind::Line,
                    SelectionKind::Line => SelectionKind::Character,
                }
            }
            else {
                SelectionKind::Character
            };
            let selection = Selection {
                anchor: point,
                end: point,
                kind,
            };
            app.selection = Some(selection);
            app.snap_selection();
            app.last_click_time = now;
        }
        app.window.request_redraw();
    }

    app.mouse_state = button;
}
pub(super) fn handle_mouse_wheel(app: &mut App, delta: MouseScrollDelta) {
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

        let sgr_mode = app.terminal.mode().contains(TerminalMode::MOUSE_SGR);
        for _ in 0..count {
            let data = app
                .convert_mouse_button_event(button)
                .encode_mouse(&app.modifiers, sgr_mode);
            app.terminal.write(&data);
        }
    }
    // ターミナルへ送信
    else {
        app.terminal.active_grid_mut().scroll((delta * 3) as isize);
    }

    app.window.request_redraw();
}
pub(super) fn handle_focused(app: &mut App, focused: bool) {
    if app.terminal.mode().contains(TerminalMode::FOCUS_REPORT) {
        let seq = if focused { "\x1b[I" } else { "\x1b[O" };
        app.terminal.write(seq.as_bytes());
    }
}
pub(super) fn handle_keyboard(app: &mut App, event: KeyEvent) {
    if event.state.is_pressed() {
        use winit::keyboard::*;

        // コピー
        if app.modifiers.state().control_key()
            && app.modifiers.state().shift_key()
            && event.physical_key == PhysicalKey::Code(KeyCode::KeyC)
        {
            if let Some(selection) = app.selection_text()
                && let Ok(mut clipboard) = arboard::Clipboard::new()
            {
                let _ = clipboard.set_text(selection);
            }
            app.selection = None;
            app.window.request_redraw();
            return;
        }
        // ペースト
        if app.modifiers.state().control_key()
            && app.modifiers.state().shift_key()
            && event.physical_key == PhysicalKey::Code(KeyCode::KeyV)
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
