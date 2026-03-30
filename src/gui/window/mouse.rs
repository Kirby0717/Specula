use winit::event::Modifiers;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    #[default]
    None,
    ScrollUp,
    ScrollDown,
}
impl MouseButton {
    pub fn is_pressed(self) -> bool {
        MouseButton::None != self
    }
}
impl TryFrom<winit::event::MouseButton> for MouseButton {
    type Error = ();
    fn try_from(value: winit::event::MouseButton) -> Result<Self, Self::Error> {
        use winit::event::MouseButton as WinitMouseButton;
        Ok(match value {
            WinitMouseButton::Left => MouseButton::Left,
            WinitMouseButton::Middle => MouseButton::Middle,
            WinitMouseButton::Right => MouseButton::Right,
            _ => return Err(()),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEventKind {
    Press,
    Release,
    Motion,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub button: MouseButton,
    pub col: usize, // 0-indexed
    pub row: usize, // 0-indexed
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}
impl MouseEvent {
    fn encode_button(&self, modifiers: &Modifiers) -> u8 {
        let mut code: u8 = match self.button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::None => 3,
            MouseButton::ScrollUp => 64,
            MouseButton::ScrollDown => 65,
        };

        if modifiers.state().shift_key() {
            code += 4;
        }
        if modifiers.state().alt_key() {
            code += 8;
        }
        if modifiers.state().control_key() {
            code += 16;
        }

        if self.kind == MouseEventKind::Motion {
            code += 32;
        }

        code
    }
    fn encode_x10(&self, modifiers: &Modifiers) -> Vec<u8> {
        let mut button = self.encode_button(modifiers);
        if self.kind == MouseEventKind::Release {
            button = (button & !0b11) | 3; // 下位2bitを3にする
        }

        vec![
            0x1b,
            b'[',
            b'M',
            button + 32,
            (self.col as u8) + 1 + 32,
            (self.row as u8) + 1 + 32,
        ]
    }
    fn encode_sgr(&self, modifiers: &Modifiers) -> Vec<u8> {
        let button = self.encode_button(modifiers);
        let terminator = match self.kind {
            MouseEventKind::Release => 'm',
            _ => 'M',
        };

        format!(
            "\x1b[<{};{};{}{}",
            button,
            self.col + 1,
            self.row + 1,
            terminator,
        )
        .into_bytes()
    }
    pub fn encode_mouse(&self, modifiers: &Modifiers, sgr_mode: bool) -> Vec<u8> {
        if sgr_mode {
            self.encode_sgr(modifiers)
        }
        else {
            self.encode_x10(modifiers)
        }
    }
}
