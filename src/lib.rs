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
    variables: Vec<Ident>,
    imports: Rc<RefCell<Vec<Ident>>>,
    // TODO: dtype Float or Int
}

// TODO: builder pattern
impl Ast {
    pub fn new(name: String, variables: &[String], input: &str) -> Self {
        let mut this = Self {
            name,
            root: AstNode::Constant(0.), // TODO: this is bad
            variables: variables.iter().map(|v| format_ident!("{}", v)).collect(),
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
                        if self.variables.contains(&identifier) {
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
                    _ => unreachable!("unexpected rule: {:?}", op.as_rule()),
                };

                Ok(AstNode::Operation {
                    left: Box::new(left),
                    op: ast_operator,
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
        let vars = &self.variables;
        let content = &self.root;

        tokens.extend(quote! {
            #[no_mangle]
            pub extern "C" fn #fn_name (#(#vars : #dtype),*) -> #dtype {
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
