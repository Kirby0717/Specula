use super::cell::Cell;

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub line: usize,
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
        CursorState {
            point: Point { line: 0, col: 0 },
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

#[derive(Clone, Debug)]
pub struct Grid {
    /// 全行（スクロールバック + 可視領域）
    raw: VecDeque<Row>,
    /// 可視領域の行数
    screen_lines: usize,
    /// 列数
    columns: usize,
    /// スクロールバックの最大行数
    max_scrollback: usize,
    /// 現在のスクロール表示オフセット（0 = 最下部）
    display_offset: usize,
    /// カーソル
    cursor: CursorState,
}
