use std::{collections::HashMap, fs};

use tree_sitter::{Node, Parser, Point, Tree};

pub struct Interpret {
    path: String,
    code: String,
    tree: Tree,
    definition: HashMap<String, Point>, // stores file global
}

fn node_to_identifier(code: &str, i: Node) -> Option<(String, Point)> {
    if i.kind() == "identifier" {
        let name = &code[i.byte_range()];
        return Some((name.to_string(), i.range().start_point));
    }
    let icrsr = &mut i.walk();
    let icidx = i.named_children(icrsr);
    let fid = icidx
        .filter(|i| {
            eprintln!("filtering kind: {}", i.kind());
            i.kind() == "function_declarator" || i.kind() == "assignment_expression"
        })
        .map(|i| {
            let crsr = &mut i.walk();
            i.named_children(crsr)
                .filter(|x| x.kind() == "identifier")
                .map(|x| (x.byte_range(), x.range()))
                .last()
                .expect("expected identifier in function_declarator")
        })
        .map(|i| (&code[i.0], i.1.start_point))
        .map(|(k, v)| (k.to_string(), v));
    fid.last()
}

fn kind_is_jumpable(i: &Node<'_>) -> bool {
    let kind = i.kind();
    kind == "function_definition" || kind == "expression_statement"
}

fn node_functions(code: &str, node: &Node<'_>) -> HashMap<String, Point> {
    let rcrsr = &mut node.walk();
    let crsr = node.named_children(rcrsr);
    let ffuncimpl = crsr
        .filter(kind_is_jumpable)
        .map(|i| node_to_identifier(code, i).unwrap_or_default());
    ffuncimpl.collect()
}

fn tree(code: String, previous: Option<&Tree>) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_nasl::language())
        .expect("Error loading NASL grammar");
    parser.parse(code, previous).expect("Error parsing file")
}

fn to_pos(r: usize, c: usize) -> f32 {
    r as f32 + c as f32 / 100.0
}
// TODO extend to look for non global definitions e.g. parameters, in block assignments, ...
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

pub fn new(path: String, code: String) -> Interpret {
    let tree = tree(code.clone(), None);
    Interpret {
        path,
        code: code.clone(),
        tree: tree.clone(),
        definition: node_functions(&code, &tree.root_node()),
    }
}

pub fn from_path(path: &str) -> Result<Interpret, std::io::Error> {
    fs::read_to_string(path).map(|code| new(path.to_string(), code))
}
impl Interpret {
    pub fn identifier(&self, line: usize, column: usize) -> Option<String> {
        let pos = to_pos(line, column);
        return find_identifier(pos, &self.code, &self.tree.root_node().clone());
    }

    pub fn find_definition(&self, line: usize, column: usize) -> Option<&Point> {
        self.identifier(line, column)
            .map(|i| self.definition.get(&i))
            .unwrap_or(None)
    }
}

#[cfg(test)]
mod tests {
    use crate::interpret::new;

    #[test]
    fn it_works() {
        let result = new(
            "/tmp/test.nasl".to_string(),
            r#"
            function test(test) {
                return test;
            }
            testus = 1;
            test(testus);
            "#
            .to_string(),
        );
        assert!(result.definition.get("test").is_some());
        assert!(result.definition.get("testus").is_some());
        assert_eq!(result.identifier(5, 14), Some("test".to_string()));
        assert_eq!(result.identifier(5, 18), Some("testus".to_string()));
        assert_eq!(
            result.find_definition(5, 19),
            result.definition.get("testus")
        );
    }
}
