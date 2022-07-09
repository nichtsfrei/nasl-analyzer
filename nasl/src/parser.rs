use itertools::{Either, Itertools};
use tree_sitter::Node;

use crate::{lookup::Lookup, types::Identifier};

#[derive(Clone)]
pub enum Jumpable {
    FunDef((Identifier, Vec<Identifier>)),
    Assign(Identifier),
    Block((Identifier, Lookup)),
    CallExpression(Identifier, Vec<Argument>),
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

pub fn identifier(code: &str, node: &Node<'_>) -> Option<Identifier> {
    if node.kind() == "identifier" {
        return Some(Identifier {
            start: node.start_position(),
            end: node.end_position(),
            identifier: Some(code[node.byte_range()].to_string()),
        });
    }
    None
}

pub fn parameter_list(code: &str, node: &Node<'_>) -> Vec<Identifier> {
    if node.kind() == "parameter_list" {
        let rcrsr = &mut node.walk();
        let crsr = node.named_children(rcrsr);
        return crsr.filter_map(|cc| identifier(code, &cc)).collect();
    }
    return vec![];
}

pub fn func_declarator(code: &str, p: &Node<'_>, c: &Node<'_>) -> Option<Jumpable> {
    // TODO refactor
    if c.kind() == "function_declarator" {
        let rcrsr = &mut c.walk();
        let crsr = c.named_children(rcrsr);
        let mut id = None;
        let mut params = vec![];
        for cc in crsr {
            if let Some(x) = identifier(code, &cc) {
                id = x.identifier;
            } else {
                params = parameter_list(code, &cc);
            }
        }
        return Some(Jumpable::FunDef((
            Identifier {
                start: p.start_position(),
                end: p.end_position(),
                identifier: id,
            },
            params,
        )));
    }
    None
}

pub fn assignment(code: &str, node: &Node<'_>) -> Option<Jumpable> {
    let rcrsr = &mut node.walk();
    let crsr = node.named_children(rcrsr);
    for c in crsr {
        if let Some(x) = identifier(code, &c) {
            return Some(Jumpable::Assign(x));
        }
    }
    None
}

pub fn compound_statement(code: &str, node: &Node<'_>) -> Option<Jumpable> {
    if node.kind() == "compound_statement" {
        return Some(Jumpable::Block((
            Identifier {
                start: node.start_position(),
                end: node.end_position(),
                identifier: None,
            },
            Lookup::new(code, node),
        )));
    }
    None
}
pub fn func_def(code: &str, node: &Node<'_>) -> Option<(Jumpable, Jumpable)> {
    let rcrsr = &mut node.walk();
    let crsr = node.named_children(rcrsr);
    let mr = crsr
        .filter_map(|c| func_declarator(code, node, &c).or_else(|| compound_statement(code, &c)))
        .collect_tuple()
        .map(|(x, y)| Some((x, y)));
    mr.flatten()
}
pub fn assignment_expression(code: &str, node: &Node<'_>) -> Option<Jumpable> {
    if node.kind() == "assignment_expression" {
        return assignment(code, node);
    }
    None
}

pub fn string_literal(code: &str, node: &Node<'_>) -> Option<Argument> {
    if node.kind() == "string_literal" {
        let rcrsr = &mut node.walk();
        let mut crsr = node.named_children(rcrsr);
        if let Some(sln) = crsr.next() {
            if sln.kind() == "string_fragment" {
                return Some(Argument::StringLiteral(Identifier {
                    start: sln.start_position(),
                    end: sln.end_position(),
                    identifier: Some(code[sln.byte_range()].to_string()),
                }));
            }
        }
    }
    None
}

pub fn argument_list(code: &str, node: &Node<'_>) -> Option<Vec<Argument>> {
    if node.kind() == "argument_list" {
        let rcrsr = &mut node.walk();
        let crsr = node.named_children(rcrsr);
        let args: Vec<Argument> = crsr.filter_map(|i| string_literal(code, &i)).collect();
        return Some(args);
    }
    None
}

pub fn call_expression(code: &str, node: &Node<'_>) -> Option<Jumpable> {
    if node.kind() == "call_expression" {
        let rcrsr = &mut node.walk();
        let crsr = node.named_children(rcrsr);
        if let Some((Either::Left(id), Either::Right(arglist))) = crsr
            .filter_map(|i| {
                identifier(code, &i)
                    .map(Either::Left)
                    .or_else(|| argument_list(code, &i).map(Either::Right))
            })
            .collect_tuple()
        {
            return Some(Jumpable::CallExpression(id, arglist));
        }
    }
    None
}
