use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub font: FontConfig,
    pub shell: ShellConfig,
    pub colors: ColorsConfig,
    pub scrollback: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub bold: Option<FontStyleConfig>,
    pub italic: Option<FontStyleConfig>,
    pub bold_italic: Option<FontStyleConfig>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FontStyleConfig {
    pub family: Option<String>,
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
    pub primary: PrimaryColors,
    pub normal: AnsiColors,
    pub bright: AnsiColors,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PrimaryColors {
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub foreground: [u8; 3],
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub background: [u8; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnsiColors {
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub black: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub red: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub green: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub yellow: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub blue: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub magenta: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub cyan: [u8; 3],
    #[serde(default, deserialize_with = "deserialize_hex_color")]
    pub white: [u8; 3],
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "monospace".to_string(),
            size: 16.0,
            bold: None,
            italic: None,
            bold_italic: None,
        }
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
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
            primary: PrimaryColors::default(),
            normal: AnsiColors::default_normal(),
            bright: AnsiColors::default_bright(),
        }
    }
}
impl Default for PrimaryColors {
    fn default() -> Self {
        Self {
            foreground: [229, 229, 229],
            background: [0, 0, 0],
        }
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
            log::debug!("設定ディレクトリが見つかりません");
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
