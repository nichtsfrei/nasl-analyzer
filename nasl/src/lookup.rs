use tree_sitter::Node;

use crate::{
    types::{Argument, Identifier}, interpret::SearchParameter, node_ext::JumpableExt,
};
#[derive(Clone, Debug)]
pub enum Jumpable {
    FunDef(Identifier, Vec<Identifier>),
    IfDef(Identifier, Vec<Identifier>),
    Assign(Identifier),
    Block((Identifier, Lookup)),
    CallExpression(Identifier, Vec<Argument>),
}

impl Jumpable {
    pub fn is_definition(&self) -> bool {
        !matches!(self, Jumpable::CallExpression(_, _))
    }
}

// CodeContainer is used as a reference container to translate byte locations from a node to a comparable string
pub struct CodeContainer<'a> {
    pub code: &'a str,
    pub origin: &'a str,
    pub parent: Option<&'a Node<'a>>,
}

impl<'a> CodeContainer<'a> {
    pub fn new(origin: &'a str, code: &'a str, parent: Option<&'a Node<'a>>) -> Self {
        Self {
            code,
            origin,
            parent,
        }
    }
}


#[derive(Clone, Debug)]
pub struct Lookup {
    pub definitions: Vec<Jumpable>,
    pub calls: Vec<Jumpable>,
    pub origin: String,
    pub includes: Vec<String>,
}

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

pub fn find_calls<'a>(
    calls: &'a [Jumpable],
    name: &'a str,
) -> impl Iterator<Item = (Identifier, Vec<Argument>)> + 'a {
    calls.iter().flat_map(move |i| match i {
        Jumpable::CallExpression(id, params) => {
            if id.identifier == Some(name.to_string()) {
                return Some((id.clone(), params.clone()));
            }
            None
        }
        _ => None,
    })
}

impl Lookup {

    pub fn new(origin: &str, code: &str, node: &Node<'_>) -> Self {
        let mut definitions: Vec<Jumpable> = vec![];
        let mut calls: Vec<Jumpable> = vec![];
        let cp = &CodeContainer::new(origin, code, None);

        // nasl specific maybe better to hide between function?
        for j in node.jumpable(cp) {
            if j.is_definition() {
                definitions.push(j);
            } else {
                calls.push(j);
            }
        }

        let includes = find_calls(&calls, "include")
            .flat_map(|(_, params)| params)
            .filter_map(|i| i.to_string())
            .collect();
        Lookup {
            origin: origin.to_string(),
            definitions,
            calls,
            includes,
        }
    }

}

