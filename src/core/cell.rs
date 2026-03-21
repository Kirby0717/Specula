#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedColor {
    // ANSI 標準 8色
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    // 高輝度 8色
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    // 端末デフォルト
    Foreground,
    Background,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct CellFlags: u16 {
        // 太字
        const BOLD             = 0b0000_0000_0001;
        // 減光
        const DIM              = 0b0000_0000_0010;
        // 斜体
        const ITALIC           = 0b0000_0000_0100;
        // 下線
        const UNDERLINE        = 0b0000_0000_1000;
        // 点滅 ( あまり使われない )
        const BLINK            = 0b0000_0001_0000;
        // 背景色の反転
        const INVERSE          = 0b0000_0010_0000;
        // 不可視
        const HIDDEN           = 0b0000_0100_0000;
        // 取り消し線
        const STRIKEOUT        = 0b0000_1000_0000;
        // ワイド幅
        const WIDE_CHAR        = 0b0001_0000_0000;
        // ワイド幅の次の空白部分
        const WIDE_CHAR_SPACER = 0b0010_0000_0000;
        // 折り返し
        const WRAPLINE         = 0b0100_0000_0000;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Named(NamedColor), // 16色 + Default前景/背景
    Indexed(u8),       // 256色パレット（0〜255）
    Rgb(u8, u8, u8),   // True Color（24bit）
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,          // 4 bytes
    pub fg: Color,        // 4 bytes (enumのサイズ)
    pub bg: Color,        // 4 bytes
    pub flags: CellFlags, // 2 bytes
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: CellFlags::empty(),
        }
    }
}
