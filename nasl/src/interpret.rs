use std::{error, fmt::Display, fs};
use tracing::warn;
use tree_sitter::{Language, Node, Parser, Point, Tree};

use crate::{
    lookup::{Lookup, SearchParameter},
    types::{to_pos, Argument, Identifier},
};

#[derive(Clone, Debug)]
pub struct Interpret {
    lookup: Lookup,
}

#[derive(Debug)]
pub enum Error {
    LanguageError,
    ParseError,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "An error occured while parsing.")
    }
}
impl error::Error for Error {}

pub fn tree(language: Language, code: &str, previous: Option<&Tree>) -> Result<Tree, Error> {
    let mut parser = Parser::new();
    match parser.set_language(language) {
        Ok(_) => parser
            .parse(code, previous)
            .map(Ok)
            .unwrap_or(Err(Error::ParseError)),
        Err(_) => Err(Error::LanguageError),
    }
}

pub fn nasl_tree(code: String, previous: Option<&Tree>) -> Result<Tree, Error> {
    tree(tree_sitter_nasl::language(), &code, previous)
}

fn find_identifier(pos: f32, code: &str, n: &Node<'_>) -> Option<String> {
    let nspos = to_pos(n.range().start_point.row, n.range().start_point.column);
    let nepos = to_pos(n.range().end_point.row, n.range().end_point.column);
    if pos >= nspos && pos <= nepos {
        if n.child_count() == 0 && n.kind() == "identifier" {
            return Some(code[n.byte_range()].to_string());
        }
        let crsr = &mut n.walk();
        let mut icidx = n
            .named_children(crsr)
            .filter_map(|i| find_identifier(pos, code, &i));
        return icidx.next();
    }
    None
}

// TODO change signature
pub fn new(origin: String, code: &str) -> Result<Interpret, Error> {
    let tree = nasl_tree(code.to_string(), None)?;
    Ok(Interpret {
        lookup: Lookup::new(&origin, code, &tree.root_node()),
    })
}

pub fn from_path(path: &str) -> Result<(String, Interpret), Box<dyn error::Error>> {
    let code: String = fs::read(path).map(|bs| bs.iter().map(|&b| b as char).collect())?;
    let r = (code.clone(), new(path.to_string(), &code)?);
    Ok(r)
}

pub trait FindDefinitionExt {
    fn find_definition(&self, name: &SearchParameter) -> Vec<Point>;
}

impl Interpret {
    pub fn identifier(
        &self,
        origin: &str,
        code: &str,
        line: usize,
        column: usize,
    ) -> Option<SearchParameter> {
        let pos = to_pos(line, column);
        match nasl_tree(code.to_string(), None) {
            Ok(tree) => {
                return find_identifier(pos, code, &tree.root_node().clone()).map(|name| {
                    SearchParameter {
                        origin: origin.to_string(),
                        name,
                        pos,
                    }
                });
            }
            Err(err) => {
                warn!("unable to parse {origin}: {err}");
                None
            }
        }
    }

    pub fn includes<'a>(&'a self) -> impl Iterator<Item = &String> + 'a {
        self.lookup.includes()
    }

    pub fn calls<'a>(
        &'a self,
        name: &str,
    ) -> Box<dyn Iterator<Item = (Identifier, Vec<Argument>)> + 'a> {
        Box::new(self.lookup.find_calls(name))
    }
}

impl FindDefinitionExt for Interpret {
    fn find_definition(&self, name: &SearchParameter) -> Vec<Point> {
        self.lookup
            .find_definition(name)
            .map(|i| i.start)
            .iter()
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use tree_sitter::Point;

    use crate::interpret::{new, FindDefinitionExt};

    #[test]
    fn global_definitions() {
        let code = r#"
            function test(a) {
                return a;
            }
            testus = test(12);
            test(testus);
            "#
        .to_string();
        let result = new("/tmp/test.nasl".to_string(), &code).unwrap();
        let testus = result.identifier("/tmp/test.nasl", &code, 5, 18);
        assert_eq!(
            result
                .identifier("/tmp/test.nasl", &code, 5, 14)
                .map(|i| i.name),
            Some("test".to_string())
        );
        assert_eq!(testus.clone().map(|i| i.name), Some("testus".to_string()));
        assert_eq!(
            result.find_definition(&testus.unwrap())[0],
            Point { row: 4, column: 12 }
        );
    }
}
