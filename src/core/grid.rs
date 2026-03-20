use super::cell::{Cell, CellFlags};

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorState {
    /// カーソル位置
    pub point: Point,
    /// 新しい入力に適用するテンプレートセル ( 現在のSGR属性を反映 )
    pub template: Cell,
    /// 次の入力時に先に折り返し処理が必要かどうか
    pub pending_wrap: bool,
}
impl Default for CursorState {
    fn default() -> Self {
        Self {
            point: Point { row: 0, col: 0 },
            template: Cell::default(),
            pending_wrap: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    inner: Vec<Cell>,
    // 最適化は必要になった時に行う
}
impl Row {
    pub fn new(cols: usize) -> Self {
        Self {
            inner: vec![Cell::default(); cols],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    /// 全行（スクロールバック + 画面領域）
    /// back側が画面領域で、必ず画面のサイズ分のセルが確保されている
    buffer: VecDeque<Row>,
    /// 画面の行数
    rows: usize,
    /// 画面の列数
    cols: usize,
    /// スクロールバックの最大行数
    max_scrollback: usize,
    /// 現在のスクロール表示オフセット（0 = 最下部）
    display_offset: usize,
    /// カーソル
    /// 画面上の相対位置で、画面外に出ない
    cursor: CursorState,
}
impl Grid {
    const MIN_ROWS: usize = 1;
    const MIN_COLS: usize = 1;
    pub fn new(rows: usize, cols: usize, max_scrollback: usize) -> Self {
        assert!(
            Self::MIN_ROWS <= rows,
            "行数は{}以上にしてください",
            Self::MIN_ROWS
        );
        assert!(
            Self::MIN_COLS <= cols,
            "列数は{}以上にしてください",
            Self::MIN_COLS
        );

        let mut buffer = VecDeque::with_capacity(rows);
        for _ in 0..rows {
            buffer.push_back(Row::new(cols));
        }

        Self {
            buffer,
            rows,
            cols,
            max_scrollback,
            display_offset: 0,
            cursor: Default::default(),
        }
    }
    pub fn grid_rows(&self) -> usize {
        self.rows
    }
    pub fn grid_cols(&self) -> usize {
        self.cols
    }
    fn clamp_cursor(&mut self) {
        let point = &mut self.cursor.point;
        point.row = point.row.min(self.rows - 1);
        point.col = point.col.min(self.cols - 1);
    }
    pub fn cursor(&self) -> &CursorState {
        &self.cursor
    }
    pub fn cursor_up(&mut self, n: usize) {
        self.cursor.point.row += n;
        self.clamp_cursor();
    }
    pub fn cursor_down(&mut self, n: usize) {
        self.cursor.point.row = self.cursor.point.row.wrapping_sub(n);
    }
    pub fn cursor_right(&mut self, n: usize) {
        self.cursor.point.col += n;
        self.clamp_cursor();
    }
    pub fn cursor_left(&mut self, n: usize) {
        self.cursor.point.col = self.cursor.point.col.wrapping_sub(n);
    }
    pub fn cursor_goto(&mut self, row: usize, col: usize) {
        self.cursor.point.row = row;
        self.cursor.point.col = col;
    }
    pub fn cursor_goto_row(&mut self, row: usize) {
        self.cursor.point.row = row;
    }
    pub fn cursor_goto_col(&mut self, col: usize) {
        self.cursor.point.col = col;
    }
    // 0: カーソル以下を消去
    // 1: カーソル以上を消去
    // 2: 画面全体を消去
    pub fn erase_display(&mut self, mode: usize) {
        let row = self.cursor.point.row;
        let col = self.cursor.point.col;
        match mode {
            0 => {
                // カーソルの右側
                self.visible_row(row).inner[col..].fill(Cell::default());
                // カーソルの下側
                for row in row + 1..self.rows {
                    self.visible_row(row).inner.fill(Cell::default());
                }
            }
            1 => {
                // カーソルの左側
                self.visible_row(row).inner[..=col].fill(Cell::default());
                // カーソルの上側
                for row in 0..row {
                    self.visible_row(row).inner.fill(Cell::default());
                }
            }
            2 => {
                for row in 0..self.rows {
                    self.visible_row(row).inner.fill(Cell::default());
                }
            }
            _ => log::debug!("未対応の画面消去モード: {}", mode),
        }
    }
    // 0: カーソル以下を消去
    // 1: カーソル以上を消去
    // 2: 行全体を消去
    pub fn erase_row(&mut self, mode: usize) {
        let col = self.cursor.point.col;
        let row = self.visible_row(self.cursor.point.row);
        match mode {
            0 => row.inner[col..].fill(Cell::default()),
            1 => row.inner[..=col].fill(Cell::default()),
            2 => row.inner.fill(Cell::default()),
            _ => log::debug!("未対応の行消去モード: {}", mode),
        }
    }
    pub fn linefeed(&mut self) {
        if self.rows - 1 == self.cursor.point.row {
            self.add_row();
        }
        else {
            self.cursor.point.row += 1;
        }
        self.cursor.pending_wrap = false;
    }
    pub fn carriage_return(&mut self) {
        self.cursor.point.col = 0;
        self.cursor.pending_wrap = false;
    }
    pub fn backspace(&mut self) {
        self.cursor.point.col = self.cursor.point.col.wrapping_sub(1);
        self.cursor.pending_wrap = false;
    }
    pub fn tab(&mut self) {
        self.cursor.point.col = (self.cursor.point.col + 1).next_multiple_of(8);
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn reverse_index(&mut self) {
        if 0 < self.cursor.point.row {
            self.cursor.point.row -= 1;
        }
        else {
            // 最下行を削除し、可視領域の先頭に空行を挿入
            self.buffer.pop_back();
            let insert_idx = self.buffer.len() - (self.rows - 1);
            self.buffer.insert(insert_idx, Row::new(self.cols));
        }
    }
    fn visible_row(&mut self, row: usize) -> &mut Row {
        let buffer_index = self.buffer.len() - self.rows + row;
        &mut self.buffer[buffer_index]
    }
    fn cell_at_cursor(&mut self) -> &mut Cell {
        // 本来なら起こらない
        if self.rows <= self.cursor.point.row
            || self.cols <= self.cursor.point.col
        {
            log::warn!(
                "カーソルが範囲外です カーソル: ({}, {}) 画面サイズ: {} x {}",
                self.cursor.point.row,
                self.cursor.point.col,
                self.rows,
                self.cols,
            );
            self.clamp_cursor();
        }

        let Point { row, col } = self.cursor.point;
        &mut self.visible_row(row).inner[col]
    }
    fn add_row(&mut self) {
        self.buffer.push_back(Row::new(self.cols));
        if self.max_scrollback + self.cols <= self.buffer.len() {
            self.buffer.pop_front();
        }
    }
    pub fn write_char(&mut self, c: char) {
        self.display_offset = 0;

        if self.cursor.pending_wrap {
            let cell = self.cell_at_cursor();
            cell.flags.insert(CellFlags::WRAPLINE);
            if self.cursor.point.row == self.rows - 1 {
                self.add_row();
            }
            else {
                self.cursor.point.row += 1;
            }
            self.cursor.point.col = 0;
            self.cursor.pending_wrap = false;
        }

        let cell = self.cell_at_cursor();
        cell.c = c;

        if self.cursor.point.col == self.cols - 1 {
            self.cursor.pending_wrap = true;
        }
        else {
            self.cursor.point.col += 1;
        }
    }

    /// 可視領域の内容をデバッグ用文字列として返す
    pub fn dump_visible(&self) -> String {
        let mut result = String::new();
        for r in 0..self.rows {
            let idx = self.buffer.len() - self.rows + r;
            for cell in &self.buffer[idx].inner {
                result.push(cell.c);
            }
            /*/ 末尾の空白を除去すると見やすい
            let trimmed = result.trim_end();
            result.truncate(trimmed.len());*/
            result.push('\n');
        }
        result
    }
}
