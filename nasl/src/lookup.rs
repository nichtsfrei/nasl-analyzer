use tree_sitter::Node;

use crate::{
    parser::{Jumpable, JumpableExt, Parser},
    types::{Argument, Identifier},
};

#[derive(Debug, PartialEq, Clone)]
pub struct SearchParameter {
    pub origin: String,
    pub name: String,
    pub pos: f32,
}

#[derive(Clone, Debug)]
pub struct Lookup {
    definitions: DefContainer,
    calls: CallContainer,
    includes: Vec<String>,
}

trait NameContainer<T> {
    fn items(&self, name: String) -> Box<dyn Iterator<Item = T> + '_>;
}

pub trait NamePosContainer<T> {
    fn items<'a>(&'a self, sp: &'a SearchParameter) -> Box<dyn Iterator<Item = Identifier> + '_>;
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
pub struct DefContainer {
    pub definitions: Vec<Jumpable>,
    pub origin: String,
}

fn verify_args(
    id: &Identifier,
    origin: &str,
    args: &Vec<Identifier>,
    defco: &SearchParameter,
) -> Vec<Identifier> {
    let mut result = vec![];
    if id.matches(&defco.name) {
        result.push(id.clone());
    }
    if origin == defco.origin && id.in_pos(defco.pos) {
        for p in args {
            if p.matches(&defco.name) {
                result.push(p.clone());
            }
        }
    }
    result
}

impl NamePosContainer<Identifier> for DefContainer {
    fn items<'a>(&'a self, sp: &'a SearchParameter) -> Box<dyn Iterator<Item = Identifier> + '_> {
        let hum = self.definitions.iter().flat_map(move |i| {
            let mut result = vec![];
            match i {
                Jumpable::Block((id, js)) => {
                    if self.origin == sp.origin && id.in_pos(sp.pos) {
                        result.extend(js.find_definition(sp));
                    }
                }
                Jumpable::IfDef(id, params) => {
                    result.extend(verify_args(id, &self.origin, params, sp));
                }
                Jumpable::FunDef(id, params) => {
                    result.extend(verify_args(id, &self.origin, params, sp));
                }
                Jumpable::Assign(id) => {
                    // TODO when need the information if it is in the same file
                    // if so control that the definition was done before
                    if id.matches(&sp.name) {
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
    pub fn find_calls<'a>(
        &'a self,
        name: &str,
    ) -> Box<dyn Iterator<Item = (Identifier, Vec<Argument>)> + 'a> {
        self.calls.items(name.to_string())
    }

    pub fn includes<'a>(&'a self) -> impl Iterator<Item = &String> + 'a {
        self.includes.iter()
    }

    pub fn find_definition(&self, sp: &SearchParameter) -> Option<Identifier> {
        self.definitions.items(sp).next()
    }

    pub fn new(origin: &str, code: &str, node: &Node<'_>) -> Self {
        let mut definitions: Vec<Jumpable> = vec![];
        let mut calls: Vec<Jumpable> = vec![];
        let cp = &Parser::new(origin, code, None);

        for j in node.jumpable(cp) {
            if j.is_definition() {
                definitions.push(j);
            } else {
                calls.push(j);
            }
        }

        let cc = CallContainer {
            calls: calls.clone(),
        };
        let dc = DefContainer {
            definitions,
            origin: origin.to_string(),
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
        interpret::nasl_tree,
        types::{to_pos, Identifier},
    };

    use super::{Lookup, SearchParameter};

    #[test]
    fn find_calls() {
        let code = r#"
            include("testus");
            test(testus);
            test("testus");
            "#;
        let tree = nasl_tree(code.to_string(), None).unwrap();
        let js = Lookup::new("", code, &tree.root_node());
        assert_eq!(js.calls.calls.len(), 3);
        assert_eq!(js.find_calls("test").collect_vec().len(), 2);
        assert_eq!(js.find_calls("include").collect_vec().len(), 1);
        assert_eq!(js.includes.len(), 1);
        assert_eq!(js.includes[0], "testus".to_string());
    }

    fn str_to_defco(name: &str, line: usize, column: usize) -> SearchParameter {
        SearchParameter {
            origin: "aha.nasl".to_string(),
            name: name.to_string(),
            pos: to_pos(line, column),
        }
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
        let tree = nasl_tree(code.to_string(), None).unwrap();
        let js = Lookup::new("aha.nasl", code, &tree.root_node());
        assert_eq!(
            js.find_definition(&str_to_defco("b", 3, 20)),
            Some(Identifier {
                start: Point { row: 2, column: 16 },
                end: Point { row: 2, column: 17 },
                identifier: Some("b".to_string())
            })
        );
        assert_eq!(
            js.find_definition(&str_to_defco("b", 6, 20)),
            Some(Identifier {
                start: Point { row: 5, column: 16 },
                end: Point { row: 5, column: 17 },
                identifier: Some("b".to_string())
            })
        );
        assert_eq!(
            js.find_definition(&str_to_defco("b", 9, 20)),
            Some(Identifier {
                start: Point { row: 8, column: 16 },
                end: Point { row: 8, column: 17 },
                identifier: Some("b".to_string())
            })
        );
        assert_eq!(
            js.find_definition(&str_to_defco("b", 12, 16)),
            Some(Identifier {
                start: Point {
                    row: 11,
                    column: 12
                },
                end: Point {
                    row: 11,
                    column: 13
                },
                identifier: Some("b".to_string())
            })
        );
        assert_eq!(
            js.find_definition(&str_to_defco("d", 14, 19)),
            Some(Identifier {
                start: Point {
                    row: 13,
                    column: 17
                },
                end: Point {
                    row: 13,
                    column: 18
                },
                identifier: Some("d".to_string())
            })
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
        let tree = nasl_tree(code.to_string(), None).unwrap();
        let js = Lookup::new("aha.nasl", code, &tree.root_node());
        assert_eq!(js.definitions.definitions.len(), 4);
        assert_eq!(
            js.find_definition(&str_to_defco("a", 2, 21)),
            Some(Identifier {
                start: Point { row: 1, column: 26 },
                end: Point { row: 1, column: 27 },
                identifier: Some("a".to_string())
            })
        );
        assert_eq!(
            js.find_definition(&str_to_defco("b", 3, 24)),
            Some(Identifier {
                start: Point { row: 2, column: 16 },
                end: Point { row: 2, column: 17 },
                identifier: Some("b".to_string())
            })
        );
    }
}
