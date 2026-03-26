use winit::{event::Modifiers, keyboard::NamedKey};

// キーの種類を3パターンに分類するenum
enum KeySequence {
    /// CSI [1;mod] letter  /  SS3 letter (DECCKM, 修飾なし)
    Letter { code: u8, app_mode: bool },
    /// CSI num [;mod] ~
    Tilde(u16),
    /// 固定バイト列（Tab, Shift+Tab, Enter, Backspace, Escape）
    Fixed(&'static [u8]),
}

pub fn build(
    modifiers: Modifiers,
    key: NamedKey,
    decckm: bool,
) -> Option<Vec<u8>> {
    let mut modifiers_code = 1;
    if modifiers.state().shift_key() {
        modifiers_code += 1;
    }
    if modifiers.state().alt_key() {
        modifiers_code += 2;
    }
    if modifiers.state().control_key() {
        modifiers_code += 4;
    }
    let sequence = classify_key(key, modifiers.state().shift_key())?;
    let buf = match sequence {
        KeySequence::Letter { code, app_mode } => {
            if modifiers_code == 1 {
                if decckm && app_mode {
                    vec![0x1b, b'O', code]
                }
                else {
                    vec![0x1b, b'[', code]
                }
            }
            else {
                let code = code as char;
                format!("\x1b[1;{modifiers_code}{code}").into_bytes()
            }
        }
        KeySequence::Tilde(num) => {
            if modifiers_code == 1 {
                format!("\x1b[{num}~").into_bytes()
            }
            else {
                format!("\x1b[{num};{modifiers_code}~").into_bytes()
            }
        }
        KeySequence::Fixed(bytes) => bytes.to_vec(),
    };
    Some(buf)
}

fn classify_key(key: NamedKey, shift: bool) -> Option<KeySequence> {
    use KeySequence::*;
    use NamedKey::*;
    match key {
        // 固定バイト列
        Tab if shift => Some(Fixed(b"\x1b[Z")),
        Tab => Some(Fixed(b"\t")),
        Enter => Some(Fixed(b"\r")),
        Backspace => Some(Fixed(b"\x7f")),
        Escape => Some(Fixed(b"\x1b")),

        // 文字終端型
        ArrowUp => Some(Letter {
            code: b'A',
            app_mode: true,
        }),
        ArrowDown => Some(Letter {
            code: b'B',
            app_mode: true,
        }),
        ArrowRight => Some(Letter {
            code: b'C',
            app_mode: true,
        }),
        ArrowLeft => Some(Letter {
            code: b'D',
            app_mode: true,
        }),
        Home => Some(Letter {
            code: b'H',
            app_mode: true,
        }),
        End => Some(Letter {
            code: b'F',
            app_mode: true,
        }),

        // チルダ終端型
        Insert => Some(Tilde(2)),
        Delete => Some(Tilde(3)),
        PageUp => Some(Tilde(5)),
        PageDown => Some(Tilde(6)),
        F5 => Some(Tilde(15)),
        F6 => Some(Tilde(17)),
        F7 => Some(Tilde(18)),
        F8 => Some(Tilde(19)),
        F9 => Some(Tilde(20)),
        F10 => Some(Tilde(21)),
        F11 => Some(Tilde(23)),
        F12 => Some(Tilde(24)),

        // SS3型
        F1 => Some(Letter {
            code: b'P',
            app_mode: false,
        }),
        F2 => Some(Letter {
            code: b'Q',
            app_mode: false,
        }),
        F3 => Some(Letter {
            code: b'R',
            app_mode: false,
        }),
        F4 => Some(Letter {
            code: b'S',
            app_mode: false,
        }),

        _ => None,
    }
}
