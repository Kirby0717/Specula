use super::grid::{CursorState, Grid};

use std::{
    io::{Read, Write},
    sync::mpsc,
};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct TerminalMode: u32 {
        // 別の画面
        // Vimなどの実行時に専用のスクリーンを用意する時に使う
        const ALT_SCREEN       = 1 << 0;   // ESC[?1049h/l
        // コピペの目印
        const BRACKETED_PASTE  = 1 << 1;   // ESC[?2004h/l
        // 今後追加: マウスレポート、カーソル非表示 等
    }
}

#[derive(Clone, Debug)]
pub struct TerminalCore {
    /// メイン画面のグリッド
    grid: Grid,
    /// オルタネートスクリーンのグリッド
    alt_grid: Grid,
    /// ターミナルモードフラグ
    mode: TerminalMode,
    /// PTY に書き戻すデータのバッファ
    write_back: Vec<u8>,
}
impl TerminalCore {
    pub fn new(rows: usize, cols: usize, max_scrollback: usize) -> Self {
        Self {
            grid: Grid::new(rows, cols, max_scrollback),
            alt_grid: Grid::new(rows, cols, max_scrollback),
            mode: TerminalMode::empty(),
            write_back: vec![],
        }
    }
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.grid.resize(rows, cols);
        self.alt_grid.resize(rows, cols);
    }
    fn active_grid(&self) -> &Grid {
        if self.mode.contains(TerminalMode::ALT_SCREEN) {
            &self.alt_grid
        }
        else {
            &self.grid
        }
    }
    fn active_grid_mut(&mut self) -> &mut Grid {
        if self.mode.contains(TerminalMode::ALT_SCREEN) {
            &mut self.alt_grid
        }
        else {
            &mut self.grid
        }
    }
}
// 最初のパラメータを取り出すヘルパー
fn param(params: &vte::Params, index: usize, default: usize) -> usize {
    params
        .iter()
        .nth(index)
        .and_then(|p| p.first())
        .map(|&v| v as usize)
        .filter(|&v| v != 0) // 0 は「省略」と同じ扱い
        .unwrap_or(default)
}
impl vte::Perform for TerminalCore {
    fn print(&mut self, c: char) {
        self.active_grid_mut().scroll_to_bottom();
        let grid = self.active_grid_mut();
        grid.write_char(c);
    }
    fn execute(&mut self, byte: u8) {
        let grid = self.active_grid_mut();
        match byte {
            // 改行 LF カーソルを1つ下に移動
            0x0A => grid.linefeed(),
            // 復帰 CR カーソルを左端に移動
            0x0D => grid.carriage_return(),
            // バックスペース BS カーソルを1つ左に移動
            0x08 => grid.backspace(),
            // タブ HT 次のタブストップへ移動
            0x09 => grid.tab(),
            _ => log::debug!("未対応の制御文字: 0x{:02X}", byte),
        }
    }
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }
        self.active_grid_mut().scroll_to_bottom();
        let grid = self.active_grid_mut();
        match (action, intermediates) {
            // カーソル移動
            ('A', []) => {
                let n = param(params, 0, 1);
                grid.cursor_up(n);
            }
            ('B', []) => {
                let n = param(params, 0, 1);
                grid.cursor_down(n);
            }
            ('C', []) => {
                let n = param(params, 0, 1);
                grid.cursor_right(n);
            }
            ('D', []) => {
                let n = param(params, 0, 1);
                grid.cursor_left(n);
            }
            ('H', []) => {
                // 1-indexed -> 0-indexed
                let row = param(params, 0, 1) - 1;
                let col = param(params, 1, 1) - 1;
                grid.cursor_goto(row, col);
            }
            ('G', []) => {
                // 1-indexed -> 0-indexed
                let col = param(params, 0, 1) - 1;
                grid.cursor_goto_col(col);
            }
            ('d', []) => {
                // 1-indexed -> 0-indexed
                let row = param(params, 0, 1) - 1;
                grid.cursor_goto_row(row);
            }

            // 消去
            ('J', []) => {
                let mode = param(params, 0, 0);
                grid.erase_display(mode);
            }
            ('K', []) => {
                let mode = param(params, 0, 0);
                grid.erase_row(mode);
            }

            // DSR（デバイス状態レポート）
            ('n', []) => {
                let mode = param(params, 0, 0);
                if mode == 6 {
                    let cursor = grid.cursor();
                    // 0-indexed -> 1-indexed
                    let row = cursor.point.row + 1;
                    let col = cursor.point.col + 1;
                    self.write_back.extend_from_slice(
                        format!("\x1b[{};{}R", row, col).as_bytes(),
                    );
                }
            }

            // SGR
            ('m', []) => {
                use super::cell::*;

                let template = grid.cursor_template_mut();
                if params.is_empty() {
                    *template = Cell::default();
                    return;
                }

                for subparam in params {
                    let code = subparam[0] as usize;
                    match code {
                        // リセット
                        0 => {
                            *template = Cell::default();
                        }
                        // 太字
                        1 => {
                            template.flags.insert(CellFlags::BOLD);
                        }
                        // 前景色
                        30..=37 => {
                            template.fg = Color::Named(unsafe {
                                std::mem::transmute::<u8, NamedColor>(
                                    code as u8 - 30,
                                )
                            });
                        }
                        // 背景色
                        40..=47 => {
                            template.bg = Color::Named(unsafe {
                                std::mem::transmute::<u8, NamedColor>(
                                    code as u8 - 40,
                                )
                            });
                        }
                        // 高輝度前景色
                        90..=97 => {
                            template.fg = Color::Named(unsafe {
                                std::mem::transmute::<u8, NamedColor>(
                                    code as u8 - 90 + 8,
                                )
                            });
                        }
                        // 高輝度前景色
                        100..=107 => {
                            template.bg = Color::Named(unsafe {
                                std::mem::transmute::<u8, NamedColor>(
                                    code as u8 - 100 + 8,
                                )
                            });
                        }
                        // デフォルト前景色
                        39 => {
                            template.fg = Color::Named(NamedColor::Foreground)
                        }
                        // デフォルト背景色
                        49 => {
                            template.bg = Color::Named(NamedColor::Background)
                        }
                        _ => log::debug!("未対応 SGR: code={code}",),
                    }
                }
            }

            _ => log::debug!(
                "未対応 CSI: action='{action}', intermediates={intermediates:?}",
            ),
        }
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        self.active_grid_mut().scroll_to_bottom();
        let grid = self.active_grid_mut();
        match (byte, intermediates) {
            (b'M', []) => grid.reverse_index(),
            _ => log::debug!(
                "未対応 ESC: byte='{byte}', intermediates={intermediates:?}",
            ),
        }
    }
}

pub struct Pty {
    #[allow(unused)]
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    // PTYからの出力が送られてくる
    pty_rx: mpsc::Receiver<Vec<u8>>,
}
impl Pty {
    pub fn new(
        shell: &str,
        size: portable_pty::PtySize,
        notify: Box<dyn Fn() + Send>,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> anyhow::Result<Self> {
        use portable_pty::{CommandBuilder, PtyPair, native_pty_system};

        let system = native_pty_system();
        let PtyPair { slave, master } = system.openpty(size)?;

        let cmd = CommandBuilder::new(shell);
        let mut shell = slave.spawn_command(cmd)?;

        let reader = master.try_clone_reader()?;
        let writer = master.take_writer()?;

        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0; 1 << 12];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(len) => {
                        if let Err(e) = tx.send(buf[..len].to_vec()) {
                            log::error!("ターミナルへの送信エラー : {e}");
                            break;
                        }
                        notify();
                    }
                    Err(e) => {
                        log::error!("PTYからの受信エラー : {e}");
                        break;
                    }
                }
            }
            notify();
        });

        std::thread::spawn(move || {
            let _ = shell.wait();
            on_exit();
        });

        Ok(Self {
            master,
            writer,
            pty_rx: rx,
        })
    }
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if let Ok(size) = self.master.get_size()
            && size.rows == rows
            && size.cols == cols
        {
            return;
        }
        self.master
            .resize(portable_pty::PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .ok();
    }
}

pub struct Terminal {
    /// ターミナル本体
    pub core: TerminalCore,
    /// VTEパーサー
    pub parser: vte::Parser,
    // PTY
    pub pty: Pty,
}
impl Terminal {
    pub fn new(
        rows: usize,
        cols: usize,
        max_scrollback: usize,
        shell: &str,
        notify: Box<dyn Fn() + Send>,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> anyhow::Result<Self> {
        let core = TerminalCore::new(rows, cols, max_scrollback);
        let parser = vte::Parser::new();
        let pty = Pty::new(
            shell,
            portable_pty::PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 1920 / 2,
                pixel_height: 1080,
            },
            notify,
            on_exit,
        )?;
        Ok(Self { core, parser, pty })
    }
    /// チャネルに溜まったデータを処理する（メインスレッドから呼ぶ）
    pub fn process_pty_output(&mut self) {
        while let Ok(data) = self.pty.pty_rx.try_recv() {
            self.parser.advance(&mut self.core, &data);
            if !self.core.write_back.is_empty() {
                let wb = std::mem::take(&mut self.core.write_back);
                self.pty.writer.write_all(&wb).ok();
            }
        }
    }
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.process_pty_output();
        self.core.resize(rows, cols);
        self.pty.resize(rows as u16, cols as u16);
    }
    pub fn write(&mut self, data: &[u8]) {
        self.core.active_grid_mut().scroll_to_bottom();
        self.pty.writer.write_all(data).ok();
    }
    pub fn cursor(&self) -> &CursorState {
        self.active_grid().cursor()
    }
    pub fn grid_rows(&self) -> usize {
        self.core.active_grid().grid_rows()
    }
    pub fn grid_cols(&self) -> usize {
        self.core.active_grid().grid_cols()
    }
    pub fn scroll(&mut self, lines: isize) {
        self.core.active_grid_mut().scroll(lines);
    }
    pub fn active_grid(&self) -> &Grid {
        self.core.active_grid()
    }
}
impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.core.fmt(f)
    }
}
