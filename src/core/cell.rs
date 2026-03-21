#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NamedColor {
    // ANSI 標準 8色
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,
    // 高輝度 8色
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
    // 端末デフォルト
    Foreground = 16,
    Background = 17,
}
impl NamedColor {
    pub fn into_color(self) -> [u8; 3] {
        [
            // Black
            [0, 0, 0],
            // Red
            [205, 0, 0],
            // Green
            [0, 205, 0],
            // Yellow
            [205, 205, 0],
            // Blue
            [0, 0, 238],
            // Magenta
            [205, 0, 205],
            // Cyan
            [0, 205, 205],
            // White
            [229, 229, 229],
            // BrightBlack
            [127, 127, 127],
            // BrightRed
            [255, 0, 0],
            // BrightGreen
            [0, 255, 0],
            // BrightYellow
            [255, 255, 0],
            // BrightBlue
            [92, 92, 255],
            // BrightMagenta
            [255, 0, 255],
            // BrightCyan
            [0, 255, 255],
            // BrightWhite
            [255, 255, 255],
            // Foreground
            [229, 229, 229],
            // Background
            [0, 0, 0],
        ][unsafe { std::mem::transmute::<NamedColor, u8>(self) as usize }]
    }
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
