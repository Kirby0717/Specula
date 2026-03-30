use crate::core::Point;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Point,
    pub end: Point,
    pub kind: SelectionKind,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    #[default]
    Character,
    Word,
    Line,
}
