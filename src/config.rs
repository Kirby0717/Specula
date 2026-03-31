use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub log_level: log::LevelFilter,
    pub font: FontConfig,
    pub shell: ShellConfig,
    pub colors: ColorsConfig,
    pub scrollback: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: Option<String>,
    pub size: f32,
    pub bold: Option<FontStyleConfig>,
    pub italic: Option<FontStyleConfig>,
    pub bold_italic: Option<FontStyleConfig>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FontStyleConfig {
    pub family: Option<String>,
    pub weight: Option<String>,
    pub style: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    pub program: String,
    pub args: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    #[serde(default = "ColorPair::default_cursor")]
    pub cursor: ColorPair,
    #[serde(default = "ColorPair::default_ime")]
    pub ime: ColorPair,
    #[serde(default = "ColorPair::default_primary")]
    pub primary: ColorPair,
    #[serde(default = "AnsiColors::default_normal")]
    pub normal: AnsiColors,
    #[serde(default = "AnsiColors::default_bright")]
    pub bright: AnsiColors,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorPair {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub background: [u8; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AnsiColors {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub black: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub red: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub green: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub yellow: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub blue: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub magenta: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub cyan: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub white: [u8; 3],
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: None,
            size: 16.0,
            bold: None,
            italic: None,
            bold_italic: None,
        }
    }
}
impl FontStyleConfig {
    pub fn resolve<'a>(
        &'a self,
        default_family: &'a str,
        default_weight: fontdb::Weight,
        default_style: fontdb::Style,
    ) -> (&'a str, fontdb::Weight, fontdb::Style) {
        (
            self.family.as_deref().unwrap_or(default_family),
            self.weight
                .as_deref()
                .map(parse_weight)
                .unwrap_or(default_weight),
            self.style
                .as_deref()
                .map(parse_style)
                .unwrap_or(default_style),
        )
    }
}
fn parse_weight(s: &str) -> fontdb::Weight {
    match s.to_lowercase().as_str() {
        "thin" => fontdb::Weight::THIN,
        "extralight" => fontdb::Weight::EXTRA_LIGHT,
        "light" => fontdb::Weight::LIGHT,
        "normal" | "regular" => fontdb::Weight::NORMAL,
        "medium" => fontdb::Weight::MEDIUM,
        "semibold" => fontdb::Weight::SEMIBOLD,
        "bold" => fontdb::Weight::BOLD,
        "extrabold" => fontdb::Weight::EXTRA_BOLD,
        "black" => fontdb::Weight::BLACK,
        other => other
            .parse::<u16>()
            .map(fontdb::Weight)
            .unwrap_or(fontdb::Weight::NORMAL),
    }
}
fn parse_style(s: &str) -> fontdb::Style {
    match s.to_lowercase().as_str() {
        "italic" => fontdb::Style::Italic,
        "oblique" => fontdb::Style::Oblique,
        _ => fontdb::Style::Normal,
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: log::LevelFilter::Warn,
            font: FontConfig::default(),
            shell: ShellConfig::default(),
            colors: ColorsConfig::default(),
            scrollback: 1_000_000,
        }
    }
}
impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            program: std::env::var("SHELL")
                .unwrap_or_else(|_| "sh".to_string()),
            args: vec![],
        }
    }
}
impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            cursor: ColorPair::default_cursor(),
            ime: ColorPair::default_ime(),
            primary: ColorPair::default_primary(),
            normal: AnsiColors::default_normal(),
            bright: AnsiColors::default_bright(),
        }
    }
}
impl ColorsConfig {
    pub fn to_cursor_colors(&self) -> [[u8; 3]; 2] {
        [self.cursor.foreground, self.cursor.background]
    }
    pub fn to_ime_colors(&self) -> [[u8; 3]; 2] {
        [self.ime.foreground, self.ime.background]
    }
    pub fn to_palette(&self) -> [[u8; 3]; 18] {
        [
            self.normal.black,
            self.normal.red,
            self.normal.green,
            self.normal.yellow,
            self.normal.blue,
            self.normal.magenta,
            self.normal.cyan,
            self.normal.white,
            self.bright.black,
            self.bright.red,
            self.bright.green,
            self.bright.yellow,
            self.bright.blue,
            self.bright.magenta,
            self.bright.cyan,
            self.bright.white,
            self.primary.foreground,
            self.primary.background,
        ]
    }
}
impl Default for ColorPair {
    fn default() -> Self {
        ColorPair {
            foreground: [0, 0, 0],
            background: [255, 255, 255],
        }
    }
}
impl ColorPair {
    fn default_cursor() -> Self {
        ColorPair {
            foreground: [0, 0, 0],
            background: [255, 255, 255],
        }
    }
    fn default_ime() -> Self {
        ColorPair {
            foreground: [255, 255, 255],
            background: [0, 0, 0],
        }
    }
    fn default_primary() -> Self {
        ColorPair {
            foreground: [255, 255, 255],
            background: [0, 0, 0],
        }
    }
}
impl Default for AnsiColors {
    fn default() -> Self {
        AnsiColors::default_normal()
    }
}
impl AnsiColors {
    fn default_normal() -> Self {
        Self {
            black: [0, 0, 0],
            red: [205, 0, 0],
            green: [0, 205, 0],
            yellow: [205, 205, 0],
            blue: [0, 0, 238],
            magenta: [205, 0, 205],
            cyan: [0, 205, 205],
            white: [229, 229, 229],
        }
    }
    fn default_bright() -> Self {
        Self {
            black: [127, 127, 127],
            red: [255, 0, 0],
            green: [0, 255, 0],
            yellow: [255, 255, 0],
            blue: [92, 92, 255],
            magenta: [255, 0, 255],
            cyan: [0, 255, 255],
            white: [255, 255, 255],
        }
    }
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<[u8; 3], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_hex_color(&s)
        .ok_or_else(|| serde::de::Error::custom(format!("無効な色指定: {s}")))
}

fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

const APP_NAME: &str = "specula";

impl Config {
    pub fn load() -> Option<Self> {
        let Some(path) = dirs::config_dir()
        else {
            log::info!("設定ディレクトリが見つかりません");
            return None;
        };
        let path = path.join(APP_NAME).join("config.toml");
        println!("{}", path.display());

        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => Some(config),
                Err(e) => {
                    log::error!("設定ファイルの解析に失敗しました: {e}");
                    None
                }
            },
            Err(e) => {
                log::error!("ファイルの読み込みに失敗しました: {e}");
                None
            }
        }
    }
}
