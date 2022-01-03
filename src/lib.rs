use std::cell::RefCell;
use std::rc::Rc;

use pest::iterators::Pairs;
use pest::prec_climber::{Assoc, Operator, PrecClimber};
use pest::Parser;
use pest_derive::Parser;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, ToTokens};

#[derive(Parser)]
#[grammar = "expression.pest"] // relative to src
struct ExpressionParser;

#[derive(Debug)]
pub struct Ast {
    name: String,
    root: AstNode,
    parameters: Vec<Ident>,
    variables: Rc<RefCell<Vec<Ident>>>,
    imports: Rc<RefCell<Vec<Ident>>>,
    // TODO: dtype Float or Int
}

// TODO: builder pattern
impl Ast {
    pub fn new(name: String, parameters: &[String], input: &str) -> Self {
        let mut this = Self {
            name,
            root: AstNode::Constant(0.), // TODO: this is bad
            parameters: parameters.iter().map(|v| format_ident!("{}", v)).collect(),
            variables: Rc::new(RefCell::new(Vec::new())),
            imports: Rc::new(RefCell::new(vec![])),
        };

        this.parse(input);

        this
    }

    pub fn code(&self) -> String {
        let tokens = self.to_token_stream();
        // TODO: format only for debug
        rustfmt_wrapper::rustfmt(tokens).unwrap()
    }

    pub fn root(&self) -> &AstNode {
        &self.root
    }

    fn parse(&mut self, input: &str) {
        let pairs = ExpressionParser::parse(Rule::main, input).unwrap_or_else(|e| panic!("{}", e));

        self.root = self.build_ast(pairs).unwrap();
    }

    fn build_ast(&self, pairs: Pairs<'_, Rule>) -> Result<AstNode, String> {
        // TODO: global var
        let precedence = PrecClimber::new(vec![
            Operator::new(Rule::add, Assoc::Left) | Operator::new(Rule::subtract, Assoc::Left),
            Operator::new(Rule::multiply, Assoc::Left) | Operator::new(Rule::divide, Assoc::Left),
            Operator::new(Rule::power, Assoc::Right),
        ]);

        precedence.climb(
            pairs,
            |pair| {
                // dbg!(&pair);
                match pair.as_rule() {
                    Rule::number => Ok(AstNode::Constant(pair.as_str().parse().unwrap())),
                    Rule::identifier => {
                        let identifier = format_ident!("{}", pair.as_str());
                        if self.parameters.contains(&identifier)
                            || self.variables.borrow().contains(&identifier)
                        {
                            Ok(AstNode::Variable(identifier))
                        } else {
                            Err(format!("unknown variable {}", identifier))
                        }
                    }
                    Rule::expression => self.build_ast(pair.into_inner()),
                    Rule::function => {
                        let mut pairs = pair.into_inner();

                        // first one is name
                        let name = format_ident!("{}", pairs.next().unwrap().as_str());

                        let args = pairs
                            .map(|pair| self.build_ast(pair.into_inner()))
                            .collect::<Result<Vec<_>, _>>()?;

                        self.imports.borrow_mut().push(name.clone());

                        Ok(AstNode::Function { name, args })
                    }
                    Rule::branch => {
                        // pairs are boolean -> expression
                        // and last one is just an expression
                        let mut pairs = pair.into_inner();

                        let mut condition_branches: Vec<Branch> = vec![];

                        while let Some(pair) = pairs.next() {
                            if matches!(pair.as_rule(), Rule::boolean_expression) {
                                let boolean = self.build_boolean_expression(pair.into_inner())?;

                                let next_pair = pairs.next().ok_or("branch structure malformed")?;
                                let expression = self.build_ast(next_pair.into_inner())?;

                                condition_branches.push(Branch {
                                    condition: boolean,
                                    body: expression,
                                });
                            } else {
                                let expression = self.build_ast(pair.into_inner())?;

                                return Ok(AstNode::Branch {
                                    condition_branches,
                                    else_branch: Box::new(expression),
                                });
                            }
                        }

                        Err("unexpected branch structure".to_string())
                    }
                    Rule::boolean_expression => {
                        Err(format!("boolean expression {}", pair.as_str()))
                    }
                    Rule::assignments_and_expression => {
                        let mut assignments: Vec<Assignment> = vec![];

                        for pair in pair.into_inner() {
                            if matches!(pair.as_rule(), Rule::assignment) {
                                let mut pairs = pair.into_inner();

                                let first_pair =
                                    pairs.next().ok_or("assignment needs first pair")?;
                                let second_pair =
                                    pairs.next().ok_or("assignment needs second pair")?;

                                let identifier = format_ident!("{}", first_pair.as_str());

                                if self.parameters.contains(&identifier) {
                                    return Err(format!(
                                        "cannot assign to parameter {}",
                                        identifier
                                    ));
                                } else {
                                    // having an assignment allows more variables
                                    self.variables.borrow_mut().push(identifier.clone());
                                }

                                let expression = self.build_ast(second_pair.into_inner())?;

                                assignments.push(Assignment {
                                    identifier,
                                    expression,
                                });
                            } else {
                                let expression = self.build_ast(pair.into_inner())?;

                                return Ok(AstNode::AssignmentsAndExpression {
                                    assignments,
                                    expression: Box::new(expression),
                                });
                            }
                        }

                        Err(
                            "unexpected assignment structure: should end with expression"
                                .to_string(),
                        )
                    }
                    _ => unreachable!("unexpected rule: {:?}", pair.as_rule()),
                }
            },
            |left, op, right| {
                let (left, right) = (left?, right?);

                // change some operators to functions
                if matches!(op.as_rule(), Rule::power) {
                    self.imports.borrow_mut().push(format_ident!("pow"));

                    return Ok(AstNode::Function {
                        name: format_ident!("pow"),
                        args: vec![left, right],
                    });
                }

                // dbg!("merge", &left, &op, &right);
                let ast_operator = match op.as_rule() {
                    Rule::add => AstOperator::Add,
                    Rule::subtract => AstOperator::Subtract,
                    Rule::multiply => AstOperator::Multiply,
                    Rule::divide => AstOperator::Divide,
                    _ => unreachable!("unexpected operator: {:?}", op.as_rule()),
                };

                Ok(AstNode::Operation {
                    left: Box::new(left),
                    op: ast_operator,
                    right: Box::new(right),
                })
            },
        )
    }

    fn build_boolean_expression(
        &self,
        pairs: Pairs<'_, Rule>,
    ) -> Result<BooleanExpression, String> {
        // TODO: global var
        // TODO: reverse??
        let precedence = PrecClimber::new(vec![
            Operator::new(Rule::and, Assoc::Left),
            Operator::new(Rule::or, Assoc::Left),
        ]);

        precedence.climb(
            pairs,
            |pair| match pair.as_rule() {
                Rule::boolean_true => Ok(BooleanExpression::Constant(true)),
                Rule::boolean_false => Ok(BooleanExpression::Constant(false)),
                Rule::boolean_comparison => {
                    let mut pairs = pair.into_inner();

                    let first_pair = pairs.next().ok_or("comparison needs second pair")?;
                    let second_pair = pairs.next().ok_or("comparison needs second pair")?;
                    let third_pair = pairs.next().ok_or("comparison needs third pair")?;

                    let left_expression = self.build_ast(first_pair.into_inner())?;
                    let comparison = match second_pair.as_rule() {
                        Rule::equals => BooleanComparator::Equal,
                        Rule::not_equals => BooleanComparator::NotEqual,
                        Rule::smaller => BooleanComparator::LessThan,
                        Rule::smaller_equals => BooleanComparator::LessThanOrEqual,
                        Rule::larger => BooleanComparator::GreaterThan,
                        Rule::larger_equals => BooleanComparator::GreaterThanOrEqual,
                        _ => {
                            return Err(format!(
                                "unexpected comparator: {:?}",
                                second_pair.as_rule()
                            ))
                        }
                    };
                    let right_expression = self.build_ast(third_pair.into_inner())?;

                    Ok(BooleanExpression::Comparison {
                        left: Box::new(left_expression),
                        op: comparison,
                        right: Box::new(right_expression),
                    })
                }
                Rule::boolean_expression => self.build_boolean_expression(pair.into_inner()),
                _ => Err(format!("unexpected boolean rule: {:?}", pair.as_rule())),
            },
            |left, op, right| {
                let (left, right) = (left?, right?);

                // dbg!("merge", &left, &op, &right);
                let boolean_operator = match op.as_rule() {
                    Rule::and => BooleanOperator::And,
                    Rule::or => BooleanOperator::Or,
                    _ => unreachable!("unexpected boolean operator: {:?}", op.as_rule()),
                };

                Ok(BooleanExpression::Operation {
                    left: Box::new(left),
                    op: boolean_operator,
                    right: Box::new(right),
                })
            },
        )
    }
}

impl ToTokens for Ast {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let dtype = format_ident!("{}", "f64");

        for fn_name in self.imports.borrow().iter().map(ToString::to_string) {
            let prefixed_fn_name = format_ident!("import_{}", fn_name);

            tokens.extend(quote! {
                #[inline]
            });

            let fn_tokens = match fn_name.as_str() {
                "max" => quote! {
                    fn #prefixed_fn_name (a: #dtype, b: #dtype) -> #dtype {
                        #dtype::max(a, b)
                    }
                },
                "pow" => quote! {
                    fn #prefixed_fn_name (a: #dtype, b: #dtype) -> #dtype {
                        #dtype::powf(a, b)
                    }
                },
                _ => todo!("{} is not yet supported", fn_name),
            };

            tokens.extend(fn_tokens);
        }

        let fn_name = format_ident!("{}", self.name);
        let params = &self.parameters;
        let content = &self.root;

        tokens.extend(quote! {
            #[no_mangle]
            pub extern "C" fn #fn_name (#(#params : #dtype),*) -> #dtype {
                #content
            }
        });
    }
}

#[derive(Debug)]
pub enum AstNode {
    Constant(f64),
    Variable(Ident),
    Operation {
        left: Box<AstNode>,
        op: AstOperator,
        right: Box<AstNode>,
    },
    Function {
        name: Ident,
        args: Vec<AstNode>,
    },
    Branch {
        condition_branches: Vec<Branch>,
        else_branch: Box<AstNode>,
    },
    AssignmentsAndExpression {
        assignments: Vec<Assignment>,
        expression: Box<AstNode>,
    },
}

impl ToTokens for AstNode {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let new_tokens = match self {
            Self::Constant(n) => quote! { #n },
            Self::Variable(v) => quote! { #v },
            Self::Operation { left, op, right } => {
                quote! { ( #left #op #right ) }
            }
            Self::Function { name, args } => {
                let fn_name = format_ident!("import_{}", name);
                quote! { #fn_name(#(#args),*) }
            }
            AstNode::Branch {
                condition_branches,
                else_branch: default_branch,
            } => {
                let mut new_tokens = TokenStream::new();
                for (i, branch) in condition_branches.iter().enumerate() {
                    let condition = &branch.condition;
                    let body = &branch.body;

                    new_tokens.extend(if i == 0 {
                        // first
                        quote! {
                            if #condition {
                                #body
                            }
                        }
                    } else {
                        // middle
                        quote! {
                            else if #condition {
                                #body
                            }
                        }
                    });
                }

                new_tokens.extend(quote! {
                    else {
                        #default_branch
                    }
                });

                new_tokens
            }
            Self::AssignmentsAndExpression {
                assignments,
                expression,
            } => {
                quote! {
                    #(#assignments)*
                    #expression
                }
            }
        };

        tokens.extend(new_tokens);
    }
}

#[derive(Debug)]
pub enum AstOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl ToTokens for AstOperator {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let new_tokens = match self {
            Self::Add => quote! { + },
            Self::Subtract => quote! { - },
            Self::Multiply => quote! { * },
            Self::Divide => quote! { / },
        };

        tokens.extend(new_tokens);
    }
}

#[derive(Debug)]
pub struct Branch {
    condition: BooleanExpression,
    body: AstNode,
}

#[derive(Debug)]
pub enum BooleanExpression {
    Constant(bool),
    Comparison {
        left: Box<AstNode>,
        op: BooleanComparator,
        right: Box<AstNode>,
    },
    Operation {
        left: Box<BooleanExpression>,
        op: BooleanOperator,
        right: Box<BooleanExpression>,
    },
}

impl ToTokens for BooleanExpression {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let new_tokens = match self {
            Self::Constant(b) => quote! { #b },
            Self::Comparison { left, op, right } => quote! { ( (#left) #op (#right) ) },
            Self::Operation { left, op, right } => quote! { ( (#left) #op (#right) ) },
        };

        tokens.extend(new_tokens);
    }
}

#[derive(Debug)]
pub enum BooleanComparator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

impl ToTokens for BooleanComparator {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let new_tokens = match self {
            Self::Equal => quote! { == },
            Self::NotEqual => quote! { != },
            Self::LessThan => quote! { < },
            Self::LessThanOrEqual => quote! { <= },
            Self::GreaterThan => quote! { > },
            Self::GreaterThanOrEqual => quote! { >= },
        };

        tokens.extend(new_tokens);
    }
}

#[derive(Debug)]
pub enum BooleanOperator {
    And,
    Or,
}

impl ToTokens for BooleanOperator {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let new_tokens = match self {
            Self::And => quote! { && },
            Self::Or => quote! { || },
        };

        tokens.extend(new_tokens);
    }
}

#[derive(Debug)]
pub struct Assignment {
    identifier: Ident,
    expression: AstNode,
}

impl ToTokens for Assignment {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            identifier,
            expression,
        } = self;
        let new_tokens = quote! {
            let #identifier = #expression;
        };

        tokens.extend(new_tokens);
    }
}
