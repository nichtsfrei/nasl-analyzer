use std::fs;
use tree_sitter::{Node, Parser, Point, Tree};

use crate::{
    lookup::{Lookup, SearchParameter},
    types::{to_pos, Argument, Identifier},
};

#[derive(Clone, Debug)]
pub struct Interpret {
    code: String,
    tree: Tree,
    lookup: Lookup,
}

pub fn tree(code: String, previous: Option<&Tree>) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_nasl::language())
        .expect("Error loading NASL grammar");
    parser.parse(code, previous).expect("Error parsing file")
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
pub fn new(origin: String, code: String) -> Interpret {
    let tree = tree(code.clone(), None);
    Interpret {
        code: code.clone(),
        tree: tree.clone(),
        lookup: Lookup::new(&origin, &code, &tree.root_node()),
    }
}

pub fn from_path(path: &str) -> Result<Interpret, std::io::Error> {
    fs::read_to_string(path).map(|code| new(path.to_string(), code))
}

impl Interpret {
    pub fn identifier(
        &self,
        origin: &str,
        line: usize,
        column: usize,
    ) -> Option<SearchParameter> {
        let pos = to_pos(line, column);
        return find_identifier(pos, &self.code, &self.tree.root_node().clone()).map(|name| {
            SearchParameter {
                origin: origin.to_string(),
                name,
                pos,
            }
        });
    }

    pub fn find_definition(&self, name: &SearchParameter) -> Vec<Point> {
        self.lookup
            .find_definition(name)
            .map(|i| i.start)
            .iter()
            .copied()
            .collect()
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

#[cfg(test)]
mod tests {
    use tree_sitter::Point;

    use crate::interpret::new;

    #[test]
    fn global_definitions() {
        let result = new(
            "/tmp/test.nasl".to_string(),
            r#"
            function test(a) {
                return a;
            }
            testus = test(12);
            test(testus);
            "#
            .to_string(),
        );
        let testus = result.identifier("/tmp/test.nasl", 5, 18);
        assert_eq!(
            result.identifier("/tmp/test.nasl", 5, 14).map(|i| i.name),
            Some("test".to_string())
        );
        assert_eq!(testus.clone().map(|i| i.name), Some("testus".to_string()));
        assert_eq!(
            result.find_definition(&testus.unwrap())[0],
            Point { row: 4, column: 12 }
        );
    }
}
