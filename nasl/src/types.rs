use tree_sitter::Point;

#[derive(Clone, Debug, PartialEq)]
pub struct Identifier {
    pub start: Point,
    pub end: Point,
    pub identifier: Option<String>,
}

pub fn to_pos(r: usize, c: usize) -> f32 {
    r as f32 + c as f32 / 100.0
}
#[derive(Clone, Debug)]
pub enum Argument {
    StringLiteral(Identifier),
}

impl Argument {
    pub fn to_string(&self) -> Option<String> {
        match self {
            Argument::StringLiteral(id) => id.clone().identifier,
        }
    }
}

impl Identifier {
    pub fn as_pos(&self) -> (f32, f32) {
        (
            to_pos(self.start.row, self.start.column),
            to_pos(self.end.row, self.end.column),
        )
    }

    pub fn in_pos(&self, pos: f32) -> bool {
        let (start, end) = self.as_pos();
        pos >= start && pos <= end
    }

    pub fn matches(&self, name: &str) -> bool {
        Some(name.to_string()) == self.identifier
    }
}
