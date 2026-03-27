use super::{
    cell::{Cell, Color, Point},
    grid::{CursorState, Grid},
};

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
        // カーソル表示
        const CURSOR_VISIBLE   = 1 << 2;   // ESC[?25h/l
        // マウスのボタンを送信
        const MOUSE_REPORT     = 1 << 3;   // ESC[?1000h
        // マウスのドラッグを送信
        const MOUSE_DRAG       = 1 << 4;   // ESC[?1002h
        // マウスの位置を送信
        const MOUSE_MOTION     = 1 << 5;   // ESC[?1003h
        // マウスの送信形式をSGRにする
        const MOUSE_SGR        = 1 << 6;   // ESC[?1006h
        // フォーカス状態を送信
        const FOCUS_REPORT     = 1 << 7;   // ESC[?1004h
        // 矢印キーで送信するシーケンスの形式切り替え
        const DECCKM           = 1 << 8;   // ESC[?1h/l
        // テンキーで送信するシーケンスの形式切り替え
        const DECKPAM          = 1 << 9;   // ESC = / ESC >
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    Hidden = 0,
    Block = 1,
    Underline = 2,
    Bar = 3,
    BlinkBlock = 4,
    BlinkUnderline = 5,
    BlinkBar = 6,
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
    // カーソルの種類
    cursor_style: CursorStyle,
}
impl TerminalCore {
    fn new(rows: usize, cols: usize, max_scrollback: usize) -> Self {
        Self {
            grid: Grid::new(rows, cols, max_scrollback),
            alt_grid: Grid::new(rows, cols, 0),
            mode: TerminalMode::CURSOR_VISIBLE,
            write_back: vec![],
            cursor_style: CursorStyle::Block,
        }
    }
    fn resize(&mut self, rows: usize, cols: usize) {
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
    fn set_dec_mode(&mut self, mode: usize, enable: bool) {
        match mode {
            // オルタネートスクリーン
            1049 => {
                self.mode.set(TerminalMode::ALT_SCREEN, enable);
                if enable {
                    self.alt_grid.clear();
                }
            }
            // ブラケットペースト
            2004 => {
                self.mode.set(TerminalMode::BRACKETED_PASTE, enable);
            }
            // カーソル表示/非表示
            25 => {
                self.mode.set(TerminalMode::CURSOR_VISIBLE, enable);
            }
            // マウスレポート
            1000 | 1002 | 1003 => {
                self.mode.remove(
                    TerminalMode::MOUSE_REPORT
                        | TerminalMode::MOUSE_DRAG
                        | TerminalMode::MOUSE_MOTION,
                );
                match mode {
                    1000 => self.mode.set(TerminalMode::MOUSE_REPORT, enable),
                    1002 => self.mode.set(TerminalMode::MOUSE_DRAG, enable),
                    1003 => self.mode.set(TerminalMode::MOUSE_MOTION, enable),
                    _ => {}
                }
            }
            // SGRエンコーディング
            1006 => {
                self.mode.set(TerminalMode::MOUSE_SGR, enable);
            }
            // フォーカスレポート
            1004 => {
                self.mode.set(TerminalMode::FOCUS_REPORT, enable);
            }
            // Win32 Input Mode — 現時点では非対応
            9001 => {
                log::debug!("Win32 Input Mode (9001): 未実装のため無視");
            }
            // 矢印キーで送信するシーケンスの形式
            1 => {
                self.mode.set(TerminalMode::DECCKM, enable);
            }

            _ => log::warn!("未対応 DEC mode: {mode}"),
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
fn handle_extend_color(iter: &mut vte::ParamsIter<'_>) -> Option<Color> {
    if let Some(kind) = iter.next() {
        match kind[0] {
            // 256色
            5 => {
                if let Some(idx) = iter.next() {
                    return Some(Color::Indexed(idx[0] as u8));
                }
            }
            // TrueColor
            2 => {
                let mut get_code =
                    || iter.next().map(|p| p[0] as u8).unwrap_or(0);
                return Some(Color::Rgb(get_code(), get_code(), get_code()));
            }
            _ => {}
        }
    }
    None
}
fn handle_sgr(template: &mut Cell, params: &vte::Params) {
    use super::cell::*;
    if params.is_empty() {
        *template = Cell::default();
        return;
    }

    let mut iter = params.iter();
    while let Some(subparam) = iter.next() {
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
            // 減光
            2 => {
                template.flags.insert(CellFlags::DIM);
            }
            // 太字/減光のリセット
            22 => {
                template.flags.remove(CellFlags::BOLD);
                template.flags.remove(CellFlags::DIM);
            }
            // イタリック
            3 => {
                template.flags.insert(CellFlags::ITALIC);
            }
            23 => {
                template.flags.remove(CellFlags::ITALIC);
            }
            // 下線
            4 => {
                template.flags.insert(CellFlags::UNDERLINE);
            }
            24 => {
                template.flags.remove(CellFlags::UNDERLINE);
            }
            // 点滅
            5 | 6 => {
                template.flags.insert(CellFlags::BLINK);
            }
            25 => {
                template.flags.remove(CellFlags::BLINK);
            }
            // 反転
            7 => {
                template.flags.insert(CellFlags::INVERSE);
            }
            27 => {
                template.flags.remove(CellFlags::INVERSE);
            }
            // 不可視
            8 => {
                template.flags.insert(CellFlags::HIDDEN);
            }
            28 => {
                template.flags.remove(CellFlags::HIDDEN);
            }
            // 取り消し線
            9 => {
                template.flags.insert(CellFlags::STRIKEOUT);
            }
            29 => {
                template.flags.remove(CellFlags::STRIKEOUT);
            }
            // 前景色
            30..=37 => {
                template.fg = Color::Named(
                    NamedColor::from_index((code - 30) as u8).unwrap(),
                );
            }
            // 拡張前景色
            38 => {
                if let Some(color) = handle_extend_color(&mut iter) {
                    template.fg = color;
                }
            }
            // デフォルト前景色
            39 => template.fg = Color::Named(NamedColor::Foreground),
            // 背景色
            40..=47 => {
                template.bg = Color::Named(
                    NamedColor::from_index((code - 40) as u8).unwrap(),
                );
            }
            // 拡張背景色
            48 => {
                if let Some(color) = handle_extend_color(&mut iter) {
                    template.bg = color;
                }
            }
            // デフォルト背景色
            49 => template.bg = Color::Named(NamedColor::Background),
            // 高輝度前景色
            90..=97 => {
                template.fg = Color::Named(
                    NamedColor::from_index((code - 90 + 8) as u8).unwrap(),
                );
            }
            // 高輝度前景色
            100..=107 => {
                template.bg = Color::Named(
                    NamedColor::from_index((code - 100 + 8) as u8).unwrap(),
                );
            }

            _ => log::warn!("未対応 SGR: code={code}",),
        }
    }
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

            _ => log::warn!("未対応の制御文字: 0x{:02X}", byte),
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
            ('X', []) => {
                let n = param(params, 0, 1);
                grid.erase_chars(n);
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
                handle_sgr(grid.cursor_template_mut(), params);
            }

            // オルタネートスクリーン
            ('h', [b'?']) => {
                for p in params.iter() {
                    self.set_dec_mode(p[0] as usize, true);
                }
            }
            ('l', [b'?']) => {
                for p in params.iter() {
                    self.set_dec_mode(p[0] as usize, false);
                }
            }

            // 行削除/挿入
            ('L', []) => {
                let n = param(params, 0, 1);
                grid.insert_lines(n);
            }
            ('M', []) => {
                let n = param(params, 0, 1);
                grid.delete_lines(n);
            }

            // カーソル設定
            ('q', [b' ']) => {
                let style = param(params, 0, 0);
                self.cursor_style = match style {
                    0 | 1 => CursorStyle::BlinkBlock,
                    2 => CursorStyle::Block,
                    3 => CursorStyle::BlinkUnderline,
                    4 => CursorStyle::Underline,
                    5 => CursorStyle::BlinkBar,
                    6 => CursorStyle::Bar,
                    _ => CursorStyle::BlinkBlock,
                };
            }

            // ウィンドウ関連
            ('t', []) => {
                let ps = param(params, 0, 0);
                log::debug!("未対応 XTWINOPS: Ps={ps}");
            }

            // キーボード拡張の確認
            ('u', [b'?']) => {
                log::debug!("Kitty keyboard protocol query: 未対応");
            }
            ('m', [b'>']) => {
                log::debug!("xterm modifyOtherKeys reset: 未対応");
            }

            _ => log::warn!(
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
            // ST (String Terminator) — 単独で届いた場合は無視
            (b'\\', []) => {}
            // 行挿入
            (b'M', []) => grid.reverse_index(),
            // カーソル保存/復元
            (b'7', []) => grid.save_cursor(),
            (b'8', []) => grid.restore_cursor(),
            // テンキーの送信するシーケンスの形式
            (b'=', []) => self.mode.insert(TerminalMode::DECKPAM),
            (b'>', []) => self.mode.remove(TerminalMode::DECKPAM),

            _ => log::warn!(
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
    fn new(
        shell: &str,
        args: &[&str],
        size: portable_pty::PtySize,
        notify: Box<dyn Fn() + Send>,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> anyhow::Result<Self> {
        use portable_pty::{CommandBuilder, PtyPair, native_pty_system};

        let system = native_pty_system();
        let PtyPair { slave, master } = system.openpty(size)?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.args(args);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
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
    fn resize(&mut self, rows: u16, cols: u16) {
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
    core: TerminalCore,
    /// VTEパーサー
    parser: vte::Parser,
    // PTY
    pty: Pty,
}
impl Terminal {
    pub fn new(
        rows: usize,
        cols: usize,
        max_scrollback: usize,
        shell: &str,
        args: &[&str],
        notify: Box<dyn Fn() + Send>,
        on_exit: Box<dyn FnOnce() + Send>,
    ) -> anyhow::Result<Self> {
        let core = TerminalCore::new(rows, cols, max_scrollback);
        let parser = vte::Parser::new();
        let pty = Pty::new(
            shell,
            args,
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
    pub fn paste(&mut self, text: &str) {
        self.core.active_grid_mut().scroll_to_bottom();
        if self.core.mode.contains(TerminalMode::BRACKETED_PASTE) {
            self.pty.writer.write_all(b"\x1b[200~").ok();
            self.pty.writer.write_all(text.as_bytes()).ok();
            self.pty.writer.write_all(b"\x1b[201~").ok();
        }
        else {
            self.pty.writer.write_all(text.as_bytes()).ok();
        }
    }
    pub fn write_key(
        &mut self,
        modifiers: winit::event::Modifiers,
        key: winit::keyboard::NamedKey,
    ) -> bool {
        if let Some(buf) = super::input::build(
            modifiers,
            key,
            self.core.mode.contains(TerminalMode::DECCKM),
        ) {
            self.write(&buf);
            return true;
        }
        false
    }
    pub fn cursor(&self) -> &CursorState {
        self.active_grid().cursor()
    }
    pub fn cursor_style(&self) -> CursorStyle {
        self.core.cursor_style
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
    pub fn mode(&self) -> TerminalMode {
        self.core.mode
    }
    pub fn viewport_row_to_buffer_index(&self, row: usize) -> usize {
        self.active_grid().viewport_row_to_buffer_index(row)
    }
    pub fn buffer_index_to_viewport_row(&self, index: usize) -> isize {
        self.active_grid().buffer_index_to_viewport_row(index)
    }
    pub fn get_text(&self, begin: Point, end: Point) -> String {
        self.active_grid().get_text(begin, end)
    }
}
impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.core.fmt(f)
    }
}
