use std::{error, fmt::Display, fs, ops::Range, path::Path};
use tracing::{trace, warn};
use tree_sitter::{Language, Node, Point, Tree};

use crate::{
    types::{to_pos, Identifier, Argument}, node_ext::{JumpableExt, CodeContainer},
};
#[derive(Clone, Debug)]
pub enum Jumpable {
    FunDef(Identifier, Vec<Identifier>),
    IfDef(Identifier, Vec<Identifier>),
    Assign(Identifier),
    Block((Identifier, NASLDefinitions)),
    CallExpression(Identifier, Vec<Argument>),
}

impl Jumpable {
    pub fn is_definition(&self) -> bool {
        !matches!(self, Jumpable::CallExpression(_, _))
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct SearchParameter<'a> {
    pub origin: &'a str,
    pub name: &'a str,
    pub pos: f32,
}

#[derive(Clone, Debug)]
pub struct NASLDefinitions {
    pub definitions: Vec<Jumpable>,
    pub origin: String,
    pub includes: Vec<String>,
    //lookup: NASLDefinitionContainer,
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

fn verify_args(
    id: &Identifier,
    origin: &str,
    args: &Vec<Identifier>,
    defco: &SearchParameter,
) -> Vec<Identifier> {
    let mut result = vec![];
    if id.matches(defco.name) {
        result.push(id.clone());
    }
    if origin == defco.origin && id.in_pos(defco.pos) {
        for p in args {
            if p.matches(defco.name) {
                result.push(p.clone());
            }
        }
    }
    result
}

pub fn find_definitions<'a>(
    definitions: &'a [Jumpable],
    origin: &'a str,
    sp: &'a SearchParameter,
) -> impl Iterator<Item = Identifier> + 'a {
    definitions.iter().flat_map(move |i| {
        let mut result = vec![];
        match i {
            Jumpable::Block((id, js)) => {
                if origin == sp.origin && id.in_pos(sp.pos) {
                    result.extend(find_definitions(&js.definitions, &js.origin, sp));
                }
            }
            Jumpable::IfDef(id, params) => {
                result.extend(verify_args(id, origin, params, sp));
            }
            Jumpable::FunDef(id, params) => {
                result.extend(verify_args(id, origin, params, sp));
            }
            Jumpable::Assign(id) => {
                // TODO when need the information if it is in the same file
                // if so control that the definition was done before
                if id.matches(sp.name) {
                    result.push(id.clone());
                }
            }
            _ => {}
        }
        result
    })
}

pub fn tree(language: Language, code: &str, previous: Option<&Tree>) -> Result<Tree, Error> {
    let mut parser = tree_sitter::Parser::new();
    match parser.set_language(language) {
        Ok(_) => parser
            .parse(code, previous)
            .map(Ok)
            .unwrap_or(Err(Error::ParseError)),
        Err(_) => Err(Error::LanguageError),
    }
}

pub fn nasl_tree(code: &str, previous: Option<&Tree>) -> Result<Tree, Error> {
    tree(tree_sitter_nasl::language(), code, previous)
}

fn find_identifier(pos: f32, code: &str, n: &Node<'_>) -> Option<Range<usize>> {
    let nspos = to_pos(n.range().start_point.row, n.range().start_point.column);
    let nepos = to_pos(n.range().end_point.row, n.range().end_point.column);
    if pos >= nspos && pos <= nepos {
        if n.child_count() == 0 && n.kind() == "identifier" {
            return Some(n.byte_range());
        }
        let crsr = &mut n.walk();
        let mut icidx = n
            .named_children(crsr)
            .filter_map(|i| find_identifier(pos, code, &i));
        return icidx.next();
    }
    None
}

impl NASLDefinitions {

pub fn new(origin: &str, code: &str, node: &Node<'_>) -> Self {
        let mut definitions: Vec<Jumpable> = vec![];
        let mut includes: Vec<String> = vec![];
        let cp = &CodeContainer::new(origin, code, None);

        for j in node.jumpable(cp) {
            if j.is_definition() {
                definitions.push(j);
            } else if let Jumpable::CallExpression(id, params) = j {
                if let Some(name) = id.clone().identifier {
                    if &name == "include" {
                        includes.extend(params.iter().filter_map(|i| i.to_string()));
                    }
                }
            }
        }

        NASLDefinitions {
            origin: origin.to_string(),
            definitions,
            includes,
        }
    }

    fn new_parse_tree(origin: &str, code: &str) -> Result<Self, Box<dyn error::Error>> {
        let tree = nasl_tree(code, None)?;
        let node = &tree.root_node();

        Ok(Self::new(origin, code, node))
    }

    pub fn new_with_includes(
        path: &str,
        paths: Vec<String>,
        code: Option<&str>,
    ) -> Result<Vec<NASLDefinitions>, Box<dyn error::Error>> {
        let code = if let Some(code) = code {
            code.to_string()
        } else {
            NASLDefinitions::read(path)?
        };
        let init = NASLDefinitions::new_parse_tree(path, &code)?;
        let pths = paths.clone();
        let incs: Vec<NASLDefinitions> = init
            .includes()
            .flat_map(|i| pths.iter().map(|p| format!("{p}/{}", i.clone())))
            .map(|p| p.strip_prefix("file://").unwrap_or(&p).to_string())
            .filter(|p| Path::new(p).exists())
            .flat_map(|p| {
                trace!("parsing {p}");
                Self::new_with_includes(&p, paths.clone(), None)
            })
            .flatten()
            .collect();
        let mut result = vec![init];
        result.extend(incs);
        Ok(result)
    }

    pub fn read(path: &str) -> Result<String, std::io::Error> {
        fs::read(path).map(|bs| bs.iter().map(|&b| b as char).collect())
    }

    pub fn origin(self) -> String {
        self.origin
    }

    pub fn search_parameter<'a>(
        origin: &'a str,
        code: &'a str,
        line: usize,
        column: usize,
    ) -> Option<SearchParameter<'a>> {
        let pos = to_pos(line, column);
        match nasl_tree(code, None) {
            Ok(tree) => {
                return find_identifier(pos, code, &tree.root_node().clone()).map(|name| {
                    SearchParameter {
                        origin,
                        name: &code[name],
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
        self.includes.iter()
    }

    
    pub fn find_points<'a>(&'a self, sp: &'a SearchParameter) -> impl Iterator<Item = Point> + 'a {
        find_definitions(&self.definitions, &self.origin, sp)
            .map(|i| i.start)
    }
}

#[cfg(test)]
mod tests {
    use tree_sitter::Point;

    use crate::{
        interpret::NASLDefinitions,
        types::to_pos,
    };

    use super::SearchParameter;

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
        let result = NASLDefinitions::new_parse_tree("/tmp/test.nasl", &code).unwrap();
        let testus = NASLDefinitions::search_parameter("/tmp/test.nasl", &code, 5, 18);
        assert_eq!(
            NASLDefinitions::search_parameter("/tmp/test.nasl", &code, 5, 14).map(|i| i.name),
            Some("test")
        );
        assert_eq!(testus.clone().map(|i| i.name), Some("testus"));
        assert_eq!(
            result.find_points(&testus.unwrap()).next(),
            Some(Point { row: 4, column: 12 }),
        );
    }

    // #[test]
    // fn find_calls() {
    //     let code = r#"
    //         include("testus");
    //         test(testus);
    //         test("testus");
    //         "#;
    //     let js = NASLInterpreter::new("", code).unwrap();
    //     assert_eq!(js.lookup.calls.len(), 3);
    //     assert_eq!(js.calls("test").count(), 2);
    //     assert_eq!(js.calls("include").count(), 1);
    //     assert_eq!(js.lookup.includes.len(), 1);
    //     assert_eq!(js.lookup.includes[0], "testus".to_string());
    // }

    fn str_to_defco(name: &str, line: usize, column: usize) -> SearchParameter {
        SearchParameter {
            origin: "aha.nasl",
            name,
            pos: to_pos(line, column),
        }
    }

    #[test]
    fn binary_expression() {
        let code = r#"
            if (((d = 23) == 1) || ((e = 42) == 42)) {
                f = d;
            } 
        "#;
        let js = NASLDefinitions::new_parse_tree("aha.nasl", code).unwrap();
        assert_eq!(
            js.find_points(&str_to_defco("d", 2, 20)).next(),
            Some(Point { row: 1, column: 18 }),
        );
    }

    #[test]
    fn if_handling() {
        let code = r#"
            if (description) {
                b = 13;
                c = b;
            } else if (42) {
                b = 14;
                c = b;
            } else {
                b = 12;
                c = b;
            }
            b = 1;
            c = b;
            if ((d = 12))
              test(d);
    "#;
        let js = NASLDefinitions::new_parse_tree("aha.nasl", code).unwrap();
        assert_eq!(
            js.find_points(&str_to_defco("b", 3, 20)).next(),
            Some(Point { row: 2, column: 16 })
        );
        assert_eq!(
            js.find_points(&str_to_defco("b", 6, 20)).next(),
            Some(Point { row: 5, column: 16 }),
        );
        assert_eq!(
            js.find_points(&str_to_defco("b", 9, 20)).next(),
            Some(Point { row: 8, column: 16 }),
        );
        assert_eq!(
            js.find_points(&str_to_defco("b", 12, 16)).next(),
            Some(Point {
                row: 11,
                column: 12
            }),
        );
        assert_eq!(
            js.find_points(&str_to_defco("d", 14, 19)).next(),
            Some(Point {
                row: 13,
                column: 17
            }),
        );
    }

    #[test]
    fn definition_locations() {
        let code = r#"
            function test(a) {
                b = a;
                return b;
            }
            b = 12;
            testus = test(b);
            test(testus);
            "#;
        let js = NASLDefinitions::new_parse_tree("aha.nasl", code).unwrap();
        assert_eq!(js.definitions.len(), 4);
        assert_eq!(
            js.find_points(&str_to_defco("a", 2, 21)).next(),
            Some(Point { row: 1, column: 26 }),
        );
        assert_eq!(
            js.find_points(&str_to_defco("b", 3, 24)).next(),
            Some(Point { row: 2, column: 16 }),
        );
    }

}
