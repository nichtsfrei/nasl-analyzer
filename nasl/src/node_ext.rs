use tree_sitter::Node;

use crate::{
    lookup::{Lookup, CodeContainer, Jumpable},
    types::{Argument, Identifier},
};


// walk_named_children uses a cursor of a node, walks through named_children and calls f with the childs to return its result
fn walk_named_children<T>(n: Node<'_>, f: impl Fn(Node, &mut Vec<T>)) -> Vec<T> {
    let mut result = vec![];
    let rcrsr = &mut n.walk();
    let crsr = n.named_children(rcrsr);
    for c in crsr {
        f(c, &mut result)
    }
    result
}

trait IdentifierExt {
    fn identifier(self, container: &CodeContainer<'_>) -> Option<Identifier>;
}

impl IdentifierExt for Node<'_> {
    fn identifier(self, container: &CodeContainer<'_>) -> Option<Identifier> {
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
    fn func_declarator(self, container: &CodeContainer<'_>) -> Option<Jumpable>;
    fn parameter_list(self, container: &CodeContainer<'_>) -> Vec<Identifier>;
}

impl FuncDeclaratorExt for Node<'_> {
    fn func_declarator(self, container: &CodeContainer<'_>) -> Option<Jumpable> {
        if self.kind() == "function_declarator" {
            if let Some(c) = self.child_by_field_name("declarator") {
                if let Some(x) = c.identifier(container) {
                    let id = x.identifier;
                    if let Some(p) = container.parent {
                        return Some(Jumpable::FunDef(
                            Identifier {
                                start: p.start_position(),
                                end: p.end_position(),
                                identifier: id,
                            },
                            self.child_by_field_name("parameters")
                                .map(|c| c.parameter_list(container))
                                .unwrap_or_default(),
                        ));
                    }
                }
            }
        }
        None
    }

    fn parameter_list(self, container: &CodeContainer<'_>) -> Vec<Identifier> {
        if self.kind() == "parameter_list" {
            return walk_named_children(self, |n, r| {
                if let Some(i) = n.identifier(container) {
                    r.push(i);
                }
            });
        }
        return vec![];
    }
}

trait FuncDefExt {
    fn func_def(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl FuncDefExt for Node<'_> {
    fn func_def(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "function_definition" {
            return walk_named_children(self, |c, r| {
                if let Some(fd) =
                    c.func_declarator(&CodeContainer::new(container.origin, container.code, Some(&self)))
                {
                    r.push(fd);
                } else {
                    r.extend(c.compound_statement(container));
                }
            });
        }
        vec![]
    }
}

trait CompoundExt {
    fn compound_statement(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl CompoundExt for Node<'_> {
    fn compound_statement(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "compound_statement" {
            return vec![Jumpable::Block((
                Identifier {
                    start: self.start_position(),
                    end: self.end_position(),
                    identifier: None,
                },
                Lookup::new(container.origin, container.code, &self),
            ))];
        }
        vec![]
    }
}

trait AssignmentExpressionExt {
    fn assignment_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl AssignmentExpressionExt for Node<'_> {
    fn assignment_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "assignment_expression" {
            // we only care for the left operator since we are just interested to jump to
            // initial definitions anyway
            if let Some(c) = self.child_by_field_name("left") {
                if let Some(id) = c.identifier(container) {
                    return vec![Jumpable::Assign(id)];
                }
            }
        }
        vec![]
    }
}

trait StringLiteralExt {
    fn string_literal(self, container: &CodeContainer<'_>) -> Option<Argument>;
}

impl StringLiteralExt for Node<'_> {
    fn string_literal(self, container: &CodeContainer<'_>) -> Option<Argument> {
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
    fn argument_list(self, container: &CodeContainer<'_>) -> Vec<Argument>;
    fn call_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl CallExpressionExt for Node<'_> {
    fn argument_list(self, container: &CodeContainer<'_>) -> Vec<Argument> {
        if self.kind() == "argument_list" {
            return walk_named_children(self, |c, r| {
                if let Some(sl) = c.string_literal(container) {
                    r.push(sl);
                }
            });
        }
        vec![]
    }

    fn call_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "call_expression" {
            if let Some(nf) = self.child_by_field_name("function") {
                if let Some(id) = nf.identifier(container) {
                    if let Some(an) = self.child_by_field_name("arguments") {
                        return vec![Jumpable::CallExpression(id, an.argument_list(container))];
                    }
                    return vec![Jumpable::CallExpression(id, vec![])];
                }
            }
        }
        vec![]
    }
}

trait ExpressionStatementExt {
    fn expression_statement(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl ExpressionStatementExt for Node<'_> {
    fn expression_statement(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "expression_statement" {
            return walk_named_children(self, |c, r| {
                r.extend(c.call_expression(container));
                r.extend(c.assignment_expression(container))
            });
        }
        vec![]
    }
}

trait BinaryExpressionExt {
    fn binary_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl BinaryExpressionExt for Node<'_> {
    fn binary_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "binary_expression" {
            return walk_named_children(self, |c, r| {
                r.extend(c.parenthesized_expression(container));
            });
        }
        vec![]
    }
}

trait ParenthesizedExpressionExt {
    fn parenthesized_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl ParenthesizedExpressionExt for Node<'_> {
    fn parenthesized_expression(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        if self.kind() == "parenthesized_expression" {
            return walk_named_children(self, |c, r| {
                r.extend(c.binary_expression(container));
                r.extend(c.assignment_expression(container));
                r.extend(c.parenthesized_expression(container));
            });
        }
        vec![]
    }
}

trait IfStatementExt {
    fn if_statement(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl IfStatementExt for Node<'_> {
    fn if_statement(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        let mut result = vec![];
        if self.kind() == "if_statement" {
            if let Some(c) = self.child_by_field_name("condition") {
                let mut assignments = vec![];
                for j in c.parenthesized_expression(container) {
                    if let Jumpable::Assign(id) = j {
                        assignments.push(id)
                    }
                }
                let ifdef = Jumpable::IfDef(
                    Identifier {
                        start: self.start_position(),
                        end: self.end_position(),
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
    fn jumpable(self, container: &CodeContainer<'_>) -> Vec<Jumpable>;
}

impl JumpableExt for Node<'_> {
    fn jumpable(self, container: &CodeContainer<'_>) -> Vec<Jumpable> {
        walk_named_children(self, |c, result| {
            result.extend(c.func_def(container));
            result.extend(c.expression_statement(container));
            result.extend(c.compound_statement(container));
            result.extend(c.if_statement(container));
        })
    }
}
