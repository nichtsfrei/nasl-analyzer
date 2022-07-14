use tree_sitter::Node;

use crate::{
    parser::{self, Jumpable},
    types::{Identifier, Argument},
};

#[derive(Clone, Debug)]
pub struct Lookup {
    definitions: DefContainer,
    calls: CallContainer,
    includes: Vec<String>,
}

trait NameContainer<T> {
    fn items(&self, name: String) -> Box<dyn Iterator<Item = T> + '_>;
}

trait NamePosContainer<T> {
    fn items(&self, name: String, pos: f32) -> Box<dyn Iterator<Item = T> + '_>;
}

#[derive(Clone, Debug)]
struct CallContainer {
    calls: Vec<Jumpable>,
}

impl NameContainer<(Identifier, Vec<Argument>)> for CallContainer {
    fn items(&self, name: String) -> Box<dyn Iterator<Item = (Identifier, Vec<Argument>)> + '_> {
        Box::new(self.calls.iter().flat_map(move |i| match i {
            Jumpable::CallExpression(id, params) => {
                if id.identifier == Some(name.clone()) {
                    return Some((id.clone(), params.clone()));
                }
                None
            }
            _ => None,
        }))
    }
}

#[derive(Clone, Debug)]
struct DefContainer {
    definitions: Vec<Jumpable>,
}

impl NamePosContainer<Identifier> for DefContainer {
    fn items(&self, name: String, pos: f32) -> Box<dyn Iterator<Item = Identifier> + '_> {
        let hum = self.definitions.iter().flat_map(move |i| {
            let mut result = vec![];
            match i {
                Jumpable::Block((id, js)) => {
                    if id.in_pos(pos) {
                        result.extend(js.find_definition(&name, pos));
                    }
                }
                Jumpable::FunDef((id, params)) => {
                    if id.matches(&name) {
                        result.push(id.clone());
                    }
                    if id.in_pos(pos) {
                        for p in params {
                            if p.matches(&name) {
                                result.push(p.clone());
                            }
                        }
                    }
                }
                Jumpable::Assign(id) => {
                    // if there is already a match ignore outside definitions
                    if id.matches(&name) {
                        result.push(id.clone());
                    }
                }
                _ => {}
            }
            result
        });
        Box::new(hum)
    }
}
impl Lookup {
    // currently we don't care about position since there is no function declaration in blocks
    pub fn find_calls<'a>(
        &'a self,
        name: String,
    ) -> Box<dyn Iterator<Item = (Identifier, Vec<Argument>)> + 'a> {
        self.calls.items(name)
    }

    pub fn includes<'a>(&'a self) -> impl Iterator<Item=&String> + 'a {
        self.includes.iter()
    }

    pub fn find_definition(&self, name: &str, pos: f32) -> Option<Identifier> {
        self.definitions.items(name.to_string(), pos).next()
    }

    pub fn new(code: &str, node: &Node<'_>) -> Self {
        let rcrsr = &mut node.walk();
        let crsr = node.named_children(rcrsr);
        let mut definitions: Vec<Jumpable> = vec![];
        let mut calls: Vec<Jumpable> = vec![];
        for c in crsr {
            match c.kind() {
                "function_definition" => {
                    if let Some((func, loc)) = parser::func_def(code, &c) {
                        definitions.push(func);
                        definitions.push(loc);
                    }
                }
                "expression_statement" => {
                    let rccrsr = &mut node.walk();
                    let ccrsr = c.named_children(rccrsr);
                    for cc in ccrsr {
                        if let Some(x) = parser::assignment_expression(code, &cc) {
                            definitions.push(x);
                        }
                        if let Some(x) = parser::call_expression(code, &cc) {
                            calls.push(x);
                        }
                    }
                }
                "compound_statement" => {
                    definitions.push(Jumpable::Block((
                        Identifier {
                            start: c.start_position(),
                            end: c.end_position(),
                            identifier: None,
                        },
                        Lookup::new(code, &c),
                    )));
                }
                _ => {} // ignore
            }
        }

        let cc = CallContainer {
            calls: calls.clone(),
        };
        let dc = DefContainer {
            definitions,
        };

        let includes = cc
            .items("include".to_string())
            .flat_map(|(_, params)| params)
            .filter_map(|i| i.to_string())
            .collect();
        Lookup {
            definitions: dc,
            calls: cc,
            includes,
        }
    }
}

#[cfg(test)]
mod tests {

    use itertools::Itertools;
    use tree_sitter::Point;

    use crate::{
        interpret::tree,
        types::{to_pos, Identifier},
    };

    use super::Lookup;

    #[test]
    fn find_calls() {
        let code = r#"
            include("testus");
            test(testus);
            test("testus");
            "#;
        let tree = tree(code.to_string(), None);
        let js = Lookup::new(code, &tree.root_node());
        assert_eq!(js.calls.calls.len(), 3);
        assert_eq!(js.find_calls("test".to_string()).collect_vec().len(), 2);
        assert_eq!(js.find_calls("include".to_string()).collect_vec().len(), 1);
        assert_eq!(js.includes.len(), 1);
        assert_eq!(js.includes[0], "testus".to_string());
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
        let tree = tree(code.to_string(), None);
        let js = Lookup::new(code, &tree.root_node());
        assert_eq!(js.definitions.definitions.len(), 4);
        assert_eq!(
            js.find_definition("a", to_pos(2, 21)),
            Some(Identifier {
                start: Point { row: 1, column: 26 },
                end: Point { row: 1, column: 27 },
                identifier: Some("a".to_string())
            })
        );
        assert_eq!(
            js.find_definition("b", to_pos(3, 24)),
            Some(Identifier {
                start: Point { row: 2, column: 16 },
                end: Point { row: 2, column: 17 },
                identifier: Some("b".to_string())
            })
        );
    }
}
