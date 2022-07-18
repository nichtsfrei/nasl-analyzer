use itertools::{Either, Itertools};

use tree_sitter::Node;

use crate::{
    lookup::Lookup,
    types::{Argument, Identifier},
};

#[derive(Clone, Debug)]
pub enum Jumpable {
    FunDef((Identifier, Vec<Identifier>)),
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

pub struct Parser<'a> {
    code: &'a str,
    parent: Option<&'a Node<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(code: &'a str, parent: Option<&'a Node<'a>>) -> Self {
        Self { code, parent }
    }
}

trait IdentifierExt {
    fn identifier(self, container: &Parser<'_>) -> Option<Identifier>;
}

impl IdentifierExt for Node<'_> {
    fn identifier(self, container: &Parser<'_>) -> Option<Identifier> {
        if self.kind() == "identifier" {
            return Some(Identifier {
                start: self.start_position(),
                end: self.end_position(),
                identifier: Some(container.code[self.byte_range()].to_string()),
            });
        }
        None
    }
}

trait FuncDeclaratorExt {
    fn func_declarator(self, container: &Parser<'_>) -> Option<Jumpable>;
    fn parameter_list(self, container: &Parser<'_>) -> Vec<Identifier>;
}

impl FuncDeclaratorExt for Node<'_> {
    fn func_declarator(self, container: &Parser<'_>) -> Option<Jumpable> {
        if self.kind() == "function_declarator" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            let mut id = None;
            let mut params = vec![];
            for cc in crsr {
                if let Some(x) = cc.identifier(container) {
                    id = x.identifier;
                } else {
                    params = cc.parameter_list(container);
                }
            }
            if let Some(p) = container.parent {
                return Some(Jumpable::FunDef((
                    Identifier {
                        start: p.start_position(),
                        end: p.end_position(),
                        identifier: id,
                    },
                    params,
                )));
            }
        }
        None
    }

    fn parameter_list(self, container: &Parser<'_>) -> Vec<Identifier> {
        if self.kind() == "parameter_list" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            return crsr.filter_map(|cc| cc.identifier(container)).collect();
        }
        return vec![];
    }
}

trait FuncDefExt {
    fn func_def(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl FuncDefExt for Node<'_> {
    fn func_def(self, container: &Parser<'_>) -> Vec<Jumpable> {
        if self.kind() == "function_definition" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            let mr: Vec<Jumpable> = crsr
                .filter_map(|c| {
                    c.func_declarator(&Parser::new(container.code, Some(&self)))
                        .or_else(|| {
                            let compounds = c.compound_statement(container);
                            if compounds.is_empty() {
                                return None;
                            }
                            Some(compounds[0].clone())
                        })
                })
                .collect();
            return mr;
        }
        vec![]
    }
}

trait CompoundExt {
    fn compound_statement(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl CompoundExt for Node<'_> {
    fn compound_statement(self, container: &Parser<'_>) -> Vec<Jumpable> {
        if self.kind() == "compound_statement" {
            return vec![Jumpable::Block((
                Identifier {
                    start: self.start_position(),
                    end: self.end_position(),
                    identifier: None,
                },
                Lookup::new(container.code, &self),
            ))];
        }
        vec![]
    }
}

trait AssignmentExpressionExt {
    fn assignment_expression(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl AssignmentExpressionExt for Node<'_> {
    fn assignment_expression(self, container: &Parser<'_>) -> Vec<Jumpable> {
        if self.kind() == "assignment_expression" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            for c in crsr {
                if let Some(x) = c.identifier(container) {
                    return vec![Jumpable::Assign(x)];
                }
            }
        }
        vec![]
    }
}

trait StringLiteralExt {
    fn string_literal(self, container: &Parser<'_>) -> Option<Argument>;
}

impl StringLiteralExt for Node<'_> {
    fn string_literal(self, container: &Parser<'_>) -> Option<Argument> {
        if self.kind() == "string_literal" {
            let rcrsr = &mut self.walk();
            let mut crsr = self.named_children(rcrsr);
            if let Some(sln) = crsr.next() {
                if sln.kind() == "string_fragment" {
                    return Some(Argument::StringLiteral(Identifier {
                        start: sln.start_position(),
                        end: sln.end_position(),
                        identifier: Some(container.code[sln.byte_range()].to_string()),
                    }));
                }
            }
        }
        None
    }
}

trait CallExpressionExt {
    fn argument_list(self, container: &Parser<'_>) -> Option<Vec<Argument>>;
    fn call_expression(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl CallExpressionExt for Node<'_> {
    fn argument_list(self, container: &Parser<'_>) -> Option<Vec<Argument>> {
        if self.kind() == "argument_list" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            let args: Vec<Argument> = crsr.filter_map(|i| i.string_literal(container)).collect();
            return Some(args);
        }
        None
    }

    fn call_expression(self, container: &Parser<'_>) -> Vec<Jumpable> {
        if self.kind() == "call_expression" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            // TODO refactor
            if let Some((Either::Left(id), Either::Right(arglist))) = crsr
                .filter_map(|i| {
                    i.identifier(container)
                        .map(Either::Left)
                        .or_else(|| i.argument_list(container).map(Either::Right))
                })
                .collect_tuple()
            {
                return vec![Jumpable::CallExpression(id, arglist)];
            }
        }
        vec![]
    }
}

trait ExpressionStatementExt {
    fn expression_statement(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl ExpressionStatementExt for Node<'_> {
    fn expression_statement(self, container: &Parser<'_>) -> Vec<Jumpable> {
        if self.kind() == "expression_statement" {
            let rccrsr = &mut self.walk();
            let ccrsr = self.named_children(rccrsr);
            let combined = |i: Node<'_>| -> Vec<Jumpable> {
                let mut result = i.call_expression(container);
                result.extend(i.assignment_expression(container));
                result
            };

            let result: Vec<Jumpable> = ccrsr.flat_map(combined).collect();
            return result;
        }
        vec![]
    }
}

trait BinaryExpressionExt {
    fn binary_expression(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl BinaryExpressionExt for Node<'_> {
    fn binary_expression(self, container: &Parser<'_>) -> Vec<Jumpable> {
        let mut result = vec![];
        if self.kind() == "binary_expression" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            for c in crsr {
                result.extend(c.paranthesized_expression(container));
            }
            return result;
        }
        result
    }
}

trait ParanthesizedExpressionExt {
    fn paranthesized_expression(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl ParanthesizedExpressionExt for Node<'_> {
    fn paranthesized_expression(self, container: &Parser<'_>) -> Vec<Jumpable> {
        let mut result = vec![];
        if self.kind() == "paranthesized_expression" {
            let rcrsr = &mut self.walk();
            let crsr = self.named_children(rcrsr);
            for c in crsr {
                result.extend(c.binary_expression(container));
                result.extend(c.assignment_expression(container))
            }
        }
        result
    }
}

trait IfStatementExt {
    fn if_statement(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl IfStatementExt for Node<'_> {
    fn if_statement(self, container: &Parser<'_>) -> Vec<Jumpable> {
        let mut result = vec![];
        if self.kind() == "if_statement" {
            if let Some(c) = self.child_by_field_name("condition") {
                let mut assignments = vec![];
                for j in c.paranthesized_expression(container) {
                    if let Jumpable::Assign(id) = j {
                        assignments.push(id)
                    }
                }
                let ifdef = Jumpable::IfDef(
                    Identifier {
                        start: c.start_position(),
                        end: c.end_position(),
                        identifier: None,
                    },
                    assignments,
                );
                result.push(ifdef);
            }
            if let Some(c) = self.child_by_field_name("consequence") {
                result.extend(c.compound_statement(container));
                result.extend(c.expression_statement(container));
            }
            if let Some(c) = self.child_by_field_name("alternative") {
                result.extend(c.if_statement(container));
                result.extend(c.compound_statement(container));
                result.extend(c.expression_statement(container));
            }
        }
        result
    }
}

pub trait JumpableExt {
    fn jumpable(self, container: &Parser<'_>) -> Vec<Jumpable>;
}

impl JumpableExt for Node<'_> {
    fn jumpable(self, container: &Parser<'_>) -> Vec<Jumpable> {
        let mut result = vec![];

        let rcrsr = &mut self.walk();
        let crsr = self.named_children(rcrsr);
        for c in crsr {
            result.extend(c.func_def(container));
            result.extend(c.expression_statement(container));
            result.extend(c.compound_statement(container));
            result.extend(c.if_statement(container));
        }
        result
    }
}
