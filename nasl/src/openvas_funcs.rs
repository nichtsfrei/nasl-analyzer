use std::{error::Error, fs, ops::Range};

use tracing::debug;
use tree_sitter::{Node, Point};

use crate::{
    interpret::{tree, SearchParameter, Jumpable, find_definitions},
    types::Identifier,
};

#[derive(Clone, Debug)]
pub struct OpenVASInBuildFunctions {
    definitions: Vec<Jumpable>,
    origin: String,
}

fn string_literal_range(r: &Range<usize>) -> Range<usize> {
    Range {
        start: r.start + 1,
        end: r.end - 1,
    }
}

fn naslfuncnames(node: &Node<'_>, code: &str) -> Vec<(Range<usize>, Point)> {
    if node.kind() == "declaration" {
        if let Some(d) = node.child_by_field_name("declarator") {
            if d.kind() == "init_declarator" {
                if let Some(d) = d.child_by_field_name("declarator") {
                    if let Some(d) = d.child_by_field_name("declarator") {
                        if code[d.byte_range()] != *"libfuncs" {
                            return vec![];
                        }
                    }
                }
            }

            if let Some(v) = d.child_by_field_name("value") {
                if v.kind() == "initializer_list" {
                    let crsr = &mut v.walk();
                    return v
                        .named_children(crsr)
                        .flat_map(|vc| {
                            if vc.kind() == "initializer_list" {
                                if let Some(sl) = vc.named_child(0) {
                                    if sl.kind() == "string_literal" {
                                        if let Some(id) = vc.named_child(1) {
                                            if id.kind() == "identifier" {
                                                return Some((
                                                    sl.byte_range(),
                                                    sl.start_position(),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                            None
                        })
                        .collect();
                }
            }
        }
    }

    vec![]
}

pub trait DefResponseContainer<T> {
    fn items<'a>(&'a self, sp: &'a SearchParameter) -> Box<dyn Iterator<Item = Point> + '_>;
}

impl OpenVASInBuildFunctions {
    pub fn from_path(path: &str) -> Result<OpenVASInBuildFunctions, Box<dyn Error>> {
        debug!("parsing {} for internal functions", path);
        let code = fs::read_to_string(path)?;
        OpenVASInBuildFunctions::new(path.to_string(), code)
    }
    pub fn new(origin: String, code: String) -> Result<OpenVASInBuildFunctions, Box<dyn Error>> {
        //let code = fs::read_to_string(path)?;
        let tree = tree(tree_sitter_c::language(), &code, None)?;
        let rn = tree.root_node();
        let rnw = &mut rn.walk();
        let nc = rn.named_children(rnw);
        let mut definitions = vec![];
        for c in nc {
            definitions.extend(naslfuncnames(&c, &code).iter().map(|(br, start)| {
                let id = Identifier {
                    identifier: Some(code[string_literal_range(br)].to_string()),
                    start: *start,
                    end: Point::default(),
                };
                debug!(
                    "add {} as internal function",
                    id.identifier.clone().unwrap_or_default()
                );
                Jumpable::FunDef(id, vec![])
            }));
        }
        Ok(OpenVASInBuildFunctions {
            definitions,
            origin,
        })
    }

    pub fn find_origin_location<'a>(&'a self, sp: &'a SearchParameter) -> impl Iterator<Item = (String, Point)> + 'a {
        find_definitions(&self.definitions, &self.origin, sp)
            .map(|x| (self.origin.clone(), x.start))
    }
}

#[cfg(test)]
mod tests {
    use tree_sitter::Point;


    use crate::interpret::SearchParameter;

    use super::OpenVASInBuildFunctions;

    #[test]
    fn funcnames() {
        let code = r#"
        #include "nasl_me.h"
        #include <stdio.h>
        static init_func libfuncs[] = { {"script_name", script_name_internal} };
        "#;
        let ut = OpenVASInBuildFunctions::new("nasl_init.c".to_string(), code.to_string()).unwrap();
        let sp = SearchParameter {
            origin: "nasl_init.c",
            name: "script_name",
            pos: 0.0,
        };
        assert_eq!(
            ut.find_origin_location(&sp).next(),
            Some(("nasl_init.c".to_string(), Point { row: 3, column: 41 })),
        );
    }
}
