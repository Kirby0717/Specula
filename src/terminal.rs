use super::grid::Grid;

use std::{
    io::{Read, Write},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
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
    fn active_grid(&mut self) -> &mut Grid {
        if self.mode.contains(TerminalMode::ALT_SCREEN) {
            &mut self.alt_grid
        }
        else {
            &mut self.grid
        }
    }
    pub fn dump_visible(&mut self) -> String {
        self.active_grid().dump_visible()
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
        let grid = self.active_grid();
        grid.write_char(c);
    }
    fn execute(&mut self, byte: u8) {
        let grid = self.active_grid();
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
        let p: Vec<Vec<u16>> = params.iter().map(|s| s.to_vec()).collect();
        if ignore {
            return;
        }
        let grid = self.active_grid();
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

            _ => log::debug!(
                "未対応 CSI: action='{}', intermediates={:?}",
                action,
                intermediates
            ),
        }
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        let grid = self.active_grid();
        match (byte, intermediates) {
            (b'M', []) => grid.reverse_index(),
            _ => log::debug!(
                "未対応 ESC: byte='{}', intermediates={:?}",
                byte,
                intermediates
            ),
        }
    }
}

pub struct Pty {
    #[allow(unused)]
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    shell: Box<dyn portable_pty::Child + Send + Sync>,
    // PTYからの出力が送られてくる
    pty_rx: mpsc::Receiver<Vec<u8>>,
}
impl Pty {
    pub fn new(
        shell: &str,
        size: portable_pty::PtySize,
    ) -> anyhow::Result<(Self, JoinHandle<()>)> {
        use portable_pty::{CommandBuilder, PtyPair, native_pty_system};

        let system = native_pty_system();
        let PtyPair { slave, master } = system.openpty(size)?;

        let cmd = CommandBuilder::new(shell);
        let shell = slave.spawn_command(cmd)?;

        let reader = master.try_clone_reader()?;
        let writer = master.take_writer()?;

        let (tx, rx) = mpsc::channel();

        let handle = std::thread::spawn(move || {
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
                    }
                    Err(e) => {
                        log::error!("PTYからの受信エラー : {e}");
                        break;
                    }
                }
            }
        });

        Ok((
            Self {
                master,
                writer,
                shell,
                pty_rx: rx,
            },
            handle,
        ))
    }
    pub fn wait(&mut self) -> std::io::Result<portable_pty::ExitStatus> {
        self.shell.wait()
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
    ) -> anyhow::Result<(Self, JoinHandle<()>)> {
        let core = TerminalCore::new(rows, cols, max_scrollback);
        let parser = vte::Parser::new();
        let (pty, handle) = Pty::new(
            shell,
            portable_pty::PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 1920 / 2,
                pixel_height: 1080,
            },
        )?;
        Ok((Self { core, parser, pty }, handle))
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
    pub fn write(&mut self, data: &[u8]) {
        self.pty.writer.write_all(data).ok();
    }
}
impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.core.fmt(f)
    }
}
