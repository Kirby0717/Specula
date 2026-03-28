use super::cell::{Cell, CellFlags, Point};

use std::collections::VecDeque;

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
    viewport_offset: usize,
    /// カーソル
    /// 画面上の相対位置で、画面外に出ない
    cursor: CursorState,
    /// カーソルの保存先
    saved_cursor: CursorState,
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
            viewport_offset: 0,
            cursor: Default::default(),
            saved_cursor: Default::default(),
        }
    }
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(Self::MIN_ROWS);
        let cols = cols.max(Self::MIN_COLS);
        if rows != self.rows {
            if self.rows < rows {
                let add = rows - self.rows;
                self.rows = rows;
                for _ in 0..add {
                    self.add_row();
                }
            }
            else {
                let delete = (self.rows - rows)
                    .min(self.rows - self.cursor.point.row - 1);
                for _ in 0..delete {
                    self.buffer.pop_back();
                }
            }
            self.rows = rows;
        }
        if cols != self.cols {
            for y in 0..rows {
                let buffer_index = self.buffer.len() - rows + y;
                self.buffer[buffer_index]
                    .inner
                    .resize(cols, Cell::default());
            }
            self.cols = cols;
        }

        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn clear(&mut self) {
        for row in 0..self.rows {
            self.screen_row_mut(row).fill(Cell::default());
        }
    }
    pub fn grid_rows(&self) -> usize {
        self.rows
    }
    pub fn grid_cols(&self) -> usize {
        self.cols
    }
    pub fn save_cursor(&mut self) {
        self.saved_cursor = self.cursor.clone();
    }

    pub fn restore_cursor(&mut self) {
        self.cursor = self.saved_cursor.clone();
        self.clamp_cursor();
    }
    fn clamp_cursor(&mut self) {
        let point = &mut self.cursor.point;
        point.row = point.row.min(self.rows - 1);
        point.col = point.col.min(self.cols - 1);
    }
    pub fn cursor(&self) -> &CursorState {
        &self.cursor
    }
    pub fn cursor_template(&self) -> &Cell {
        &self.cursor.template
    }
    pub fn cursor_template_mut(&mut self) -> &mut Cell {
        &mut self.cursor.template
    }
    pub fn cursor_up(&mut self, n: usize) {
        self.cursor.point.row = self.cursor.point.row.saturating_sub(n);
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn cursor_down(&mut self, n: usize) {
        self.cursor.point.row += n;
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn cursor_right(&mut self, n: usize) {
        self.cursor.point.col += n;
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn cursor_left(&mut self, n: usize) {
        self.cursor.point.col = self.cursor.point.col.saturating_sub(n);
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn cursor_goto(&mut self, row: usize, col: usize) {
        self.cursor.point.row = row;
        self.cursor.point.col = col;
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn cursor_goto_row(&mut self, row: usize) {
        self.cursor.point.row = row;
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn cursor_goto_col(&mut self, col: usize) {
        self.cursor.point.col = col;
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    // 0: カーソル以下を消去
    // 1: カーソル以上を消去
    // 2: 画面全体を消去
    pub fn erase_display(&mut self, mode: usize) {
        let cell = Cell {
            bg: self.cursor_template().bg,
            ..Default::default()
        };
        let row = self.cursor.point.row;
        let col = self.cursor.point.col;
        match mode {
            0 => {
                // カーソルの右側
                self.screen_row_mut(row)[col..].fill(cell);
                // カーソルの下側
                for row in row + 1..self.rows {
                    self.screen_row_mut(row).fill(cell);
                }
            }
            1 => {
                // カーソルの左側
                self.screen_row_mut(row)[..=col].fill(cell);
                // カーソルの上側
                for row in 0..row {
                    self.screen_row_mut(row).fill(cell);
                }
            }
            2 => {
                for row in 0..self.rows {
                    self.screen_row_mut(row).fill(cell);
                }
            }
            _ => log::debug!("未対応の画面消去モード: {}", mode),
        }
    }
    // 0: カーソル以下を消去
    // 1: カーソル以上を消去
    // 2: 行全体を消去
    pub fn erase_row(&mut self, mode: usize) {
        let cell = Cell {
            bg: self.cursor_template().bg,
            ..Default::default()
        };
        let col = self.cursor.point.col;
        let row = self.screen_row_mut(self.cursor.point.row);
        match mode {
            0 => row[col..].fill(cell),
            1 => row[..=col].fill(cell),
            2 => row.fill(cell),
            _ => log::debug!("未対応の行消去モード: {}", mode),
        }
    }
    pub fn erase_chars(&mut self, n: usize) {
        let cell = Cell {
            bg: self.cursor_template().bg,
            ..Default::default()
        };
        let col = self.cursor.point.col;
        let cols = self.cols;
        let row = self.screen_row_mut(self.cursor.point.row);
        for i in 0..n {
            let c = col + i;
            if c >= cols {
                break;
            }
            row[c] = cell;
        }
    }
    pub fn linefeed(&mut self) {
        if self.rows - 1 == self.cursor.point.row {
            self.add_row();
        }
        else {
            self.cursor.point.row += 1;
        }
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn carriage_return(&mut self) {
        self.cursor.point.col = 0;
        self.clamp_cursor();
        self.cursor.pending_wrap = false;
    }
    pub fn backspace(&mut self) {
        self.cursor.point.col = self.cursor.point.col.saturating_sub(1);
        self.clamp_cursor();
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
        self.cursor.pending_wrap = false;
    }
    pub fn insert_lines(&mut self, n: usize) {
        let n = n.min(self.rows - self.cursor.point.row);
        let cursor_idx = self.buffer.len() - self.rows + self.cursor.point.row;

        for _ in 0..n {
            self.buffer.insert(cursor_idx, Row::new(self.cols));
        }
        for _ in 0..n {
            self.buffer.pop_back();
        }

        self.cursor.pending_wrap = false;
    }
    pub fn delete_lines(&mut self, n: usize) {
        let n = n.min(self.rows - self.cursor.point.row);
        let cursor_idx = self.buffer.len() - self.rows + self.cursor.point.row;

        for _ in 0..n {
            self.buffer.remove(cursor_idx);
        }
        for _ in 0..n {
            self.buffer.push_back(Row::new(self.cols));
        }

        self.cursor.pending_wrap = false;
    }
    pub fn screen_row(&self, row: usize) -> &[Cell] {
        debug_assert!(
            row < self.rows,
            "指定された行数({row})が0～{}の範囲外です",
            self.rows
        );
        let buffer_index = self.buffer.len() - self.rows + row;
        &self.buffer[buffer_index].inner
    }
    pub fn screen_row_mut(&mut self, row: usize) -> &mut [Cell] {
        debug_assert!(
            row < self.rows,
            "指定された行数({row})が0～{}の範囲外です",
            self.rows
        );
        let buffer_index = self.buffer.len() - self.rows + row;
        &mut self.buffer[buffer_index].inner
    }
    pub fn viewport_row(&self, row: usize) -> &[Cell] {
        debug_assert!(
            row < self.rows,
            "指定された行数({row})が0～{}の範囲外です",
            self.rows
        );
        let buffer_index =
            self.buffer.len() - self.rows - self.viewport_offset + row;
        &self.buffer[buffer_index].inner
    }
    pub fn viewport_row_to_buffer_index(&self, row: usize) -> usize {
        self.buffer.len() - self.rows - self.viewport_offset + row
    }
    pub fn buffer_index_to_viewport_row(&self, index: usize) -> isize {
        (index + self.viewport_offset + self.rows) as isize
            - self.buffer.len() as isize
    }
    pub fn get_text(&self, begin: Point, end: Point) -> String {
        let mut text = String::new();
        'a: for row in begin.row..=end.row {
            if self.buffer.len() <= row {
                break;
            }
            let line = &self.buffer[row].inner;
            for (col, cell) in line.iter().enumerate() {
                if row == begin.row && col < begin.col {
                    continue;
                }
                if row == end.row && end.col < col {
                    break 'a;
                }
                if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                    continue;
                }
                text.push(cell.c);
            }
            text.push('\n');
        }
        text
    }
    pub fn snap_selection(
        &self,
        mut anchor: Point,
        mut end: Point,
    ) -> (Point, Point) {
        if let Some(row) = self.buffer.get(anchor.row)
            && let Some(cell) = row.inner.get(anchor.col)
            && cell.flags.contains(CellFlags::WIDE_CHAR_SPACER)
        {
            anchor.col = anchor.col.saturating_sub(1);
        }
        if let Some(row) = self.buffer.get(end.row)
            && let Some(cell) = row.inner.get(end.col)
            && cell.flags.contains(CellFlags::WIDE_CHAR_SPACER)
        {
            end.col += 1;
        }
        (anchor, end)
    }
    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }
    pub fn scroll(&mut self, lines: isize) {
        self.viewport_offset = self
            .viewport_offset
            .saturating_add_signed(lines)
            .min(self.scrollback_len());
    }
    pub fn scroll_to_bottom(&mut self) {
        self.viewport_offset = 0;
    }
    fn scrollback_len(&self) -> usize {
        self.buffer.len() - self.rows
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
        &mut self.screen_row_mut(row)[col]
    }
    fn add_row(&mut self) {
        self.buffer.push_back(Row::new(self.cols));
        while self.max_scrollback + self.rows < self.buffer.len() {
            self.buffer.pop_front();
        }
    }
    pub fn write_char(&mut self, c: char) {
        self.viewport_offset = 0;

        let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if width == 0 {
            return;
        }

        let template = *self.cursor_template();

        // 折り返し処理
        if self.cursor.pending_wrap {
            let cell = self.cell_at_cursor();
            cell.flags.insert(CellFlags::WRAPLINE);
            self.linefeed();
            self.carriage_return();
        }

        // 全角
        if width == 2 {
            // 入らなければ改行
            if self.cols - 1 <= self.cursor.point.col {
                let cell = self.cell_at_cursor();
                *cell = template;
                cell.c = ' ';
                self.linefeed();
                self.carriage_return();
            }

            self.clear_wide_at_cursor();
            let cell = self.cell_at_cursor();
            *cell = template;
            cell.c = c;
            cell.flags.insert(CellFlags::WIDE_CHAR);
            self.cursor_right(1);
            let cell = self.cell_at_cursor();
            *cell = template;
            cell.c = ' ';
            cell.flags.insert(CellFlags::WIDE_CHAR_SPACER);
        }
        //半角
        else {
            self.clear_wide_at_cursor();
            let cell = self.cell_at_cursor();
            *cell = template;
            cell.c = c;
        }

        // 行末処理
        if self.cursor.point.col == self.cols - 1 {
            self.cursor.pending_wrap = true;
        }
        else {
            self.cursor.point.col += 1;
        }
    }
    fn clear_wide_at_cursor(&mut self) {
        let col = self.cursor.point.col;
        let row = self.cursor.point.row;

        let flags = self.screen_row(row)[col].flags;

        if flags.contains(CellFlags::WIDE_CHAR) && col + 1 < self.cols {
            let spacer = &mut self.screen_row_mut(row)[col + 1];
            spacer.c = ' ';
            spacer.flags = CellFlags::empty();
        }

        if flags.contains(CellFlags::WIDE_CHAR_SPACER) && col > 0 {
            let wide = &mut self.screen_row_mut(row)[col - 1];
            wide.c = ' ';
            wide.flags = CellFlags::empty();
        }
    }
}
