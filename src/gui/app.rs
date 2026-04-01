use super::{
    GlyphAtlas, GpuContext, Renderer, TermEvent,
    window::{
        MouseButton, MouseEvent, MouseEventKind, Selection, SelectionKind,
    },
};
use crate::core::{Grid, Point, Terminal, TerminalMode};

use std::{sync::Arc, time::Instant};

use winit::{event::Modifiers, event_loop::EventLoopProxy, window::Window};

pub(super) struct App {
    pub window: Arc<Window>,
    pub gpu: GpuContext,
    pub atlas: GlyphAtlas,
    pub renderer: Renderer,
    pub terminal: Terminal,

    pub modifiers: Modifiers,
    pub cursor_position: [f64; 2],
    pub mouse_state: MouseButton,
    pub last_click_time: Instant,
    pub selection: Option<Selection>,
}
impl App {
    pub const MULTI_CLICK_INTERVAL: std::time::Duration =
        std::time::Duration::from_millis(500);
    pub fn new(
        window: Window,
        proxy: &EventLoopProxy<TermEvent>,
        config: &crate::config::Config,
    ) -> Self {
        let window = Arc::new(window);
        let mut gpu = GpuContext::new(&window);
        gpu.configure_surface();

        let atlas = GlyphAtlas::new(&gpu, &config.font);
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
            config.scrollback,
            &config.shell.program,
            &config.shell.args,
            notify,
            on_exit,
        )
        .expect("ターミナルの起動に失敗しました");

        let renderer = Renderer::new(&gpu, &atlas, &terminal, config);

        App {
            gpu,
            window,
            atlas,
            renderer,
            terminal,

            modifiers: Modifiers::default(),
            cursor_position: [0.0, 0.0],
            mouse_state: MouseButton::default(),
            last_click_time: Instant::now(),
            selection: None,
        }
    }
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gpu.size = new_size;
        self.gpu.configure_surface();

        let [cell_width, cell_height] = self.atlas.cell_size();
        let rows = (new_size.height / cell_height) as usize;
        let cols = (new_size.width / cell_width) as usize;
        let rows = rows.clamp(Grid::MIN_ROWS, Grid::MAX_ROWS);
        let cols = cols.clamp(Grid::MIN_COLS, Grid::MAX_COLS);

        self.terminal.resize(rows, cols);
        self.renderer.resize(&self.gpu, &self.atlas, rows, cols);
    }
    pub fn convert_mouse_button_event(
        &self,
        button: MouseButton,
    ) -> MouseEvent {
        let Point { row, col } = self.cursor_cell();
        MouseEvent {
            kind: if button.is_pressed() {
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
    pub fn convert_mouse_cursor_event(
        &self,
        button: MouseButton,
    ) -> MouseEvent {
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
    pub fn mouse_report_active(&self) -> bool {
        self.terminal.mode().intersects(
            TerminalMode::MOUSE_REPORT
                | TerminalMode::MOUSE_DRAG
                | TerminalMode::MOUSE_MOTION,
        )
    }
    pub fn cursor_cell(&self) -> Point {
        let grid = self.terminal.active_grid();
        let [cell_width, cell_height] = self.atlas.cell_size();
        let col = (self.cursor_position[0] / cell_width as f64) as usize;
        let row = (self.cursor_position[1] / cell_height as f64) as usize;
        let col = col.min(grid.grid_cols() - 1);
        let row = row.min(grid.grid_rows() - 1);
        Point { row, col }
    }
    pub fn cursor_boundary_cell(&self) -> Point {
        let grid = self.terminal.active_grid();
        let [cell_width, cell_height] = self.atlas.cell_size();
        let col = (self.cursor_position[0] / cell_width as f64 + 0.5) as usize;
        let row = (self.cursor_position[1] / cell_height as f64) as usize;
        let col = col.min(grid.grid_cols());
        let row = row.min(grid.grid_rows() - 1);
        Point { row, col }
    }
    pub fn snap_selection(&mut self) {
        if let Some(selection) = &mut self.selection {
            let (anchor, end) = self
                .terminal
                .active_grid()
                .snap_selection(selection.anchor, selection.end);
            selection.anchor = anchor;
            selection.end = end;
        }
    }
    pub fn selection_range(&self) -> Option<(Point, Point)> {
        let Selection { anchor, end, kind } = self.selection?;
        let (l, r) = if anchor < end {
            (anchor, end)
        }
        else {
            (end, anchor)
        };
        match kind {
            SelectionKind::Character => Some((l, r)),
            SelectionKind::Word => {
                let grid = self.terminal.active_grid();
                let l = l.min(Point {
                    row: l.row,
                    col: grid.get_word_range(l).0,
                });
                let r = r.max(Point {
                    row: r.row,
                    col: grid.get_word_range(r).1,
                });
                Some((l, r))
            }
            SelectionKind::Line => {
                let l = l.min(Point { row: l.row, col: 0 });
                let r = r.max(Point {
                    row: r.row,
                    col: Grid::MAX_COLS,
                });
                Some((l, r))
            }
        }
    }
    pub fn selection_text(&self) -> Option<String> {
        let (begin, end) = self.selection_range()?;
        Some(self.terminal.active_grid().get_text(begin, end))
    }
}
