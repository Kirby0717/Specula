use super::grid::Grid;

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
pub struct Terminal {
    /// メイン画面のグリッド
    grid: Grid,
    /// オルタネートスクリーンのグリッド
    alt_grid: Grid,
    /// 現在オルタネートスクリーンがアクティブか
    alt_screen_active: bool,
    /// ターミナルモードフラグ
    mode: TerminalMode,
}
