#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(unused)]
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
    pub fn from_index(index: u8) -> Option<Self> {
        if index < 18 {
            // 18色まで定義されているので安全
            Some(unsafe { std::mem::transmute::<u8, NamedColor>(index) })
        }
        else {
            None
        }
    }
    pub fn into_color(self, palette: &[[u8; 3]; 18]) -> [u8; 3] {
        palette[self as u8 as usize]
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
impl Color {
    pub fn color_to_rgba(self, palette: &[[u8; 3]; 18]) -> [f32; 4] {
        let [r, g, b] = match self {
            Color::Named(named) => named.into_color(palette),
            Color::Indexed(index) => {
                match index {
                    // ANSI 16色
                    0..16 => palette[index as usize],
                    // 6x6x6 カラーキューブ
                    16..232 => {
                        let n = index - 16;
                        let r = n / 36;
                        let g = (n % 36) / 6;
                        let b = n % 6;
                        fn cube_component(v: u8) -> u8 {
                            if v == 0 { 0 } else { 55 + 40 * v }
                        }
                        [
                            cube_component(r),
                            cube_component(g),
                            cube_component(b),
                        ]
                    }
                    // 24段階グレースケール
                    232..=255 => [8 + 10 * (index - 232); 3],
                }
            }
            Color::Rgb(r, g, b) => [r, g, b],
        };
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    }
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

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub row: usize,
    pub col: usize,
}
