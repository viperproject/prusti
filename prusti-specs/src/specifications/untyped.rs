use super::common::{self, ExpressionIdGenerator};
use proc_macro2::{TokenStream, Group, Span, Spacing, Punct, TokenTree, Delimiter};
use quote::{quote_spanned, ToTokens, TokenStreamExt};
use std::collections::HashMap;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use quote::quote;
pub use common::{ExpressionId, SpecType, SpecificationId};
pub use super::preparser::{Parser, Arg};
use crate::specifications::common::ForAllVars;

/// A specification that has no types associated with it.
pub type Specification = common::Specification<ExpressionId, syn::Expr, Arg>;
/// A set of untyped specifications associated with a single element.
pub type SpecificationSet = common::SpecificationSet<ExpressionId, syn::Expr, Arg>;
/// A set of untyped specifications associated with a loop.
pub type LoopSpecification = common::LoopSpecification<ExpressionId, syn::Expr, Arg>;
/// A set of untyped specifications associated with a procedure.
pub type ProcedureSpecification = common::ProcedureSpecification<ExpressionId, syn::Expr, Arg>;
/// A map of untyped specifications for a specific crate.
pub type SpecificationMap = HashMap<common::SpecificationId, SpecificationSet>;
/// An assertion that has no types associated with it.
pub type Assertion = common::Assertion<ExpressionId, syn::Expr, Arg>;
/// An assertion kind that has no types associated with it.
pub type AssertionKind = common::AssertionKind<ExpressionId, syn::Expr, Arg>;
/// An expression that has no types associated with it.
pub type Expression = common::Expression<ExpressionId, syn::Expr>;
/// A trigger set that has not types associated with it.
pub type TriggerSet = common::TriggerSet<ExpressionId, syn::Expr>;
/// A pledge that has not types associated with it.
pub type Pledge = common::Pledge<ExpressionId, syn::Expr, Arg>;

/// An abstraction over all kinds of function items.
pub enum AnyFnItem {
    Fn(syn::ItemFn),
    TraitMethod(syn::TraitItemMethod),
    ImplMethod(syn::ImplItemMethod),
}

impl syn::parse::Parse for AnyFnItem {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        use syn::parse::discouraged::Speculative;
        let fork = input.fork();
        let item_fn = fork.parse();
        match item_fn {
            Ok(res) => {
                // We have an item Fn.
                input.advance_to(&fork);
                Ok(AnyFnItem::Fn(res))
            },
            Err(_) => {
                // It is not a valid ItemFn.
                let item_method = input.parse()?;
                Ok(AnyFnItem::TraitMethod(item_method))
            }
        }
    }
}

impl AnyFnItem {
    pub fn attrs_mut(&mut self) -> &mut Vec<syn::Attribute> {
        match self {
            AnyFnItem::Fn(item) => &mut item.attrs,
            AnyFnItem::TraitMethod(item) => &mut item.attrs,
            AnyFnItem::ImplMethod(item) => &mut item.attrs,
        }
    }
    pub fn sig(&self) -> &syn::Signature {
        match self {
            AnyFnItem::Fn(item) => &item.sig,
            AnyFnItem::TraitMethod(item) => &item.sig,
            AnyFnItem::ImplMethod(item) => &item.sig,
        }
    }
}

impl ToTokens for AnyFnItem {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            AnyFnItem::Fn(item) => item.to_tokens(tokens),
            AnyFnItem::TraitMethod(item) => item.to_tokens(tokens),
            AnyFnItem::ImplMethod(item) => item.to_tokens(tokens),
        }
    }
}

impl Assertion {
    pub(crate) fn parse(
        tokens: TokenStream,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> syn::Result<Self> {
        let mut parser = Parser::from_token_stream(tokens);
        let assertion = parser.extract_assertion()?;
        Ok(assertion.assign_id(spec_id, id_generator))
    }
}

impl Parse for common::Expression<(), syn::Expr> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            spec_id: SpecificationId::dummy(),
            id: (),
            expr: input.parse()?,
        })
    }
}

impl Parse for common::Assertion<(), syn::Expr, Arg> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // TODO: Implement parsing of the full spec. Some code can be taken from
        // here:
        // https://gitlab.inf.ethz.ch/OU-PMUELLER/prusti-dev/-/commits/new-parser/
        Ok(Self {
            kind: box common::AssertionKind::Expr(input.parse()?),
        })
    }
}

impl Pledge {
    pub(crate) fn parse(
        tokens: TokenStream,
        spec_id_rhs: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> syn::Result<Self> {
        let tokens = Self::replace_special_syntax_ts(tokens);
        let mut parser = Parser::from_token_stream(tokens);
        let pledge = parser.extract_pledge_rhs_only()?;
        let pledge = Pledge {
            rhs: pledge.rhs.assign_id(spec_id_rhs, id_generator),
        };
        Ok(pledge)
    }

    /// Pledges can use the `before_expiry[ref](expr)` and `after_unblocked[ref](expr)` syntax,
    /// which is not proper Rust syntax. This function takes a token stream and returns a new
    /// token stream with the following replacements:
    ///  - `before_expiry[ref](expr)` -> `before_expiry(&ref, expr)`
    ///  - `after_unblocked[ref](expr)` -> `after_unblocked(&ref, expr)`
    ///  - `before_expiry(expr)` -> `before_expiry(0, expr)`
    ///  - `after_unblocked(expr)` -> `after_unblocked(0, expr)`.
    fn replace_special_syntax_ts(ts: TokenStream) -> TokenStream {
        enum State { A, B, C { reference: TokenStream } }
        use State::*;
        fn extend(
            mut ts: TokenStream,
            newts: impl Into<TokenStream>
        ) -> TokenStream {
            ts.extend(newts.into()); ts
        }

        let (_, ts) = ts.into_iter().fold(
            (A, TokenStream::new()),
            |(state, ts), tt| match state {
                A => match &tt {
                    TokenTree::Ident(ident) => match ident.to_string().as_str() {
                        "before_expiry" => (B, extend(ts, tt)),
                        "after_unblocked" => (B, extend(ts, tt)),
                        _ => (A, extend(ts, tt))
                    },
                    _ => (A, extend(ts, Self::replace_special_syntax_tt(tt)))
                },
                B => match &tt {
                    TokenTree::Group(group) => match group.delimiter() {
                        Delimiter::Bracket => (C { reference: group.stream() }, ts),
                        Delimiter::Parenthesis => {
                            let expression = Self::replace_special_syntax_ts(group.stream());
                            (A, extend(ts, quote_spanned! { group.span() => (0, #expression) }))
                        }
                        _ => (A, extend(ts, tt)),
                    },
                    _ => (A, extend(ts, tt))
                },
                C { reference } => match &tt {
                    TokenTree::Group(group) => match group.delimiter() {
                        Delimiter::Parenthesis => {
                            let reference = quote_spanned! { reference.span() => &#reference };
                            let expression = group.stream();
                            (A, extend(ts, quote! { (#reference, #expression) }))
                        }
                        _ => (A, extend(ts, tt)),
                    },
                    _ => (A, extend(ts, tt))
                },
            }
        );
        ts
    }

    fn replace_special_syntax_tt(tt: TokenTree) -> TokenTree {
        match &tt {
            TokenTree::Group(group) => {
                let ts = Self::replace_special_syntax_ts(group.stream());
                TokenTree::Group(Group::new(group.delimiter(), ts))
            }
            _ => tt
        }
    }
}

pub(crate) trait AssignExpressionId<Target> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> Target;
}

impl AssignExpressionId<Expression> for common::Expression<(), syn::Expr> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> Expression {
        Expression {
            spec_id,
            id: id_generator.generate(),
            expr: self.expr,
        }
    }
}

impl AssignExpressionId<Option<Assertion>> for Option<common::Assertion<(), syn::Expr, Arg>> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> Option<Assertion> {
        if self.is_none() {
            return None;
        }
        Some(self.unwrap().assign_id(spec_id, id_generator))
    }
}

impl AssignExpressionId<Option<Expression>> for Option<common::Expression<(), syn::Expr>> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> Option<Expression> {
        if self.is_none() {
            return None;
        }
        Some(self.unwrap().assign_id(spec_id, id_generator))
    }
}

impl AssignExpressionId<ForAllVars<ExpressionId, Arg>> for common::ForAllVars<(), Arg> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> ForAllVars<ExpressionId, Arg> {
        ForAllVars {
            spec_id,
            id: id_generator.generate(),
            vars: self.vars
        }
    }
}

impl AssignExpressionId<TriggerSet> for common::TriggerSet<(), syn::Expr> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> TriggerSet {
        TriggerSet {
            0: self.0
                .into_iter()
                .map(|x| common::Trigger {
                    0: x.0
                        .into_iter()
                        .map(|y| y.assign_id(spec_id, id_generator))
                        .collect()
                })
                .collect()
        }
    }
}


impl AssignExpressionId<AssertionKind> for common::AssertionKind<(), syn::Expr, Arg> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> AssertionKind {
        use common::AssertionKind::*;
        match self {
            Expr(expr) => Expr(expr.assign_id(spec_id, id_generator)),
            And(assertions) => And(
                assertions.into_iter()
                          .map(|assertion|
                              Assertion { kind: assertion.kind.assign_id(spec_id, id_generator) })
                          .collect()),
            Implies(lhs, rhs) => Implies(
                lhs.assign_id(spec_id, id_generator),
                rhs.assign_id(spec_id, id_generator)
            ),
            ForAll(vars, triggers, body) => ForAll(
                vars.assign_id(spec_id, id_generator),
                triggers.assign_id(spec_id, id_generator),
                body.assign_id(spec_id, id_generator)
            ),
            x => unimplemented!("{:?}", x),
        }
    }
}

impl AssignExpressionId<Box<AssertionKind>> for Box<common::AssertionKind<(), syn::Expr, Arg>> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> Box<AssertionKind> {
        box (*self).assign_id(spec_id, id_generator)
    }
}

impl AssignExpressionId<Assertion> for common::Assertion<(), syn::Expr, Arg> {
    fn assign_id(
        self,
        spec_id: SpecificationId,
        id_generator: &mut ExpressionIdGenerator,
    ) -> Assertion {
        Assertion {
            kind: self.kind.assign_id(spec_id, id_generator),
        }
    }
}

/// A trait for encoding the statements for type-checking the spec.
pub trait EncodeTypeCheck {
    fn encode_type_check(&self, tokens: &mut TokenStream);
}

impl EncodeTypeCheck for Vec<Specification> {
    fn encode_type_check(&self, tokens: &mut TokenStream) {
        for spec in self {
            spec.encode_type_check(tokens);
        }
    }
}

impl EncodeTypeCheck for Specification {
    fn encode_type_check(&self, tokens: &mut TokenStream) {
        self.assertion.encode_type_check(tokens);
    }
}

impl EncodeTypeCheck for TriggerSet {
    fn encode_type_check(&self, tokens: &mut TokenStream) {
        for trigger_tuple in &self.0 {
            for trigger in &trigger_tuple.0 {
                let span = trigger.expr.span();
                let expr = &trigger.expr;
                let identifier = format!("{}_{}", trigger.spec_id, trigger.id);
                let typeck_call = quote_spanned! { span =>
                    #[prusti::spec_only]
                    #[prusti::expr_id = #identifier]
                    || {
                        #expr;
                    };
                };
                tokens.extend(typeck_call);
            }
        }
    }
}

impl ToTokens for Arg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.name.to_tokens(tokens);
        tokens.append(Punct::new(':', Spacing::Alone));
        self.typ.to_tokens(tokens);
    }
}

impl EncodeTypeCheck for Assertion {
    fn encode_type_check(&self, tokens: &mut TokenStream) {
        match &*self.kind {
            AssertionKind::Expr(expression) => {
                expression.encode_type_check(tokens);
            }
            AssertionKind::And(assertions) => {
                for assertion in assertions {
                    assertion.encode_type_check(tokens);
                }
            }
            AssertionKind::Implies(lhs, rhs) => {
                lhs.encode_type_check(tokens);
                rhs.encode_type_check(tokens);
            }
            AssertionKind::ForAll(vars, triggers, body) => {
                let vec_of_vars = &vars.vars;
                let span = Span::call_site();
                let identifier = format!("{}_{}", vars.spec_id, vars.id);

                let mut nested_assertion = TokenStream::new();
                body.encode_type_check(&mut nested_assertion);
                triggers.encode_type_check(&mut nested_assertion);

                let typeck_call = quote_spanned! { span =>
                    #[prusti::spec_only]
                    #[prusti::expr_id = #identifier]
                    |#(#vec_of_vars),*| {
                        #nested_assertion
                    };
                };
                tokens.extend(typeck_call);
            }
            x => {
                unimplemented!("{:?}", x);
            }
        }
    }
}

impl EncodeTypeCheck for Expression {
    fn encode_type_check(&self, tokens: &mut TokenStream) {
        let span = self.expr.span();
        let expr = &self.expr;
        let identifier = format!("{}_{}", self.spec_id, self.id);
        let typeck_call = quote_spanned! { span =>
            #[prusti::spec_only]
            #[prusti::expr_id = #identifier]
            || -> bool {
                #expr
            };
        };
        tokens.extend(typeck_call);
    }
}
