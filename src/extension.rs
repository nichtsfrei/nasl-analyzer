use lsp_types::{Range, Position};
use serde::{Deserialize, Serialize};
use tree_sitter::Point;

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct Paths {
    pub paths: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Paths>,
}


pub trait AsRangeExt {
    fn as_range(&self) -> Range;
}

impl AsRangeExt for Point {
    fn as_range(&self) -> Range {
        Range {
            start: Position {
                line: self.row as u32,
                character: self.column as u32,
            },
            end: Position {
                line: self.row as u32,
                character: self.column as u32,
            },
        }
    }
}

