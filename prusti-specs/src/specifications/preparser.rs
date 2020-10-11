/// The preparser splits a composite Prusti assertion into atomic assertions,
/// parses the resulting Rust expressions, and then assembles the composite
/// Prusti assertion.

use proc_macro2::{Delimiter, Group, Span, TokenStream, TokenTree};
use std::collections::VecDeque;
use std::mem;
use syn::parse::{ParseStream, Parse};
use syn::{self, Token, Error};

use super::common;
use crate::specifications::common::{ForAllVars, TriggerSet, Trigger};
use syn::spanned::Spanned;

pub type AssertionWithoutId = common::Assertion<(), syn::Expr, Arg>;
pub type PledgeWithoutId = common::Pledge<(), syn::Expr, Arg>;
pub type ExpressionWithoutId = common::Expression<(), syn::Expr>;

/// A helper to operate the stream of tokens.
#[derive(Debug, Clone)]
struct ParserStream {
    /// Auxiliary field to store the span related to the just-made method call.
    span: Span,
    /// A queue of tokens the ParserStream operates on.
    tokens: VecDeque<TokenTree>,
}

impl ParserStream {
    fn empty() -> Self {
        Self {
            tokens: VecDeque::new(),
            span: Span::call_site(),
        }
    }
    fn from_token_stream(tokens: TokenStream) -> Self {
        let token_queue: VecDeque<_> = tokens.into_iter().collect();
        Self {
            tokens: token_queue,
            span: Span::call_site(),
        }
    }
    /// Check if there is a subexpression (parenthesized or separated by `==>`)
    /// that contains both `&&` and `||`. If yes, set the span to include both
    /// of those operators and everything in between them. This detects
    /// potentially ambiguous subexpressions.
    fn contains_both_and_or(&mut self) -> bool {
        let mut stream = self.clone();
        let mut contains_and = false;
        let mut contains_or = false;
        let mut and_span: Option<Span> = None;
        let mut or_span: Option<Span> = None;

        while !stream.is_empty() {
            // subexpression contains and
            if stream.peek_operator("&&") {
                contains_and = true;
                and_span = Some(stream.tokens.front().span());
            }
            // subexpression contains or
            else if stream.peek_operator("||") {
                contains_or = true;
                or_span = Some(stream.tokens.front().span());
            }
            // implies met - reset subexpression
            else if stream.peek_operator("==>") {
                contains_and = false;
                contains_or = false;
            }
            // nested expression met - resolve it recursively
            else if stream.peek_parenthesized_block() {
                let tokens = stream.check_and_consume_parenthesized_block().unwrap().stream();
                let mut nested_stream = ParserStream::from_token_stream(tokens);
                if nested_stream.contains_both_and_or() {
                    self.span = nested_stream.span;
                    return true;
                }
            }
            // if a subexpression contains both `&&` and `||`, construct the span and return
            if contains_and && contains_or {
                if let Some(a_s) = and_span {
                    if let Some(o_s) = or_span {
                        self.span = a_s.join(o_s).unwrap();
                    }
                }
                return true;
            }
            stream.pop();
        }
        return false;
    }
    /// Check if the token queue is empty.
    fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
    /// Remove the top token from the queue, return it, and set the span to it.
    fn pop(&mut self) -> Option<TokenTree> {
        if let Some(token) = self.tokens.pop_front() {
            self.span = token.span();
            Some(token)
        } else {
            None
        }
    }
    /// Check if the input starts with the keyword and if yes, consume it
    /// and set the span to it.
    fn check_and_consume_keyword(&mut self, keyword: &str) -> bool {
        if let Some(TokenTree::Ident(ident)) = self.tokens.front() {
            if ident.to_string() == keyword {
                self.pop();
                return true;
            }
        }
        false
    }
    /// Check if the input starts with the operator. Does not set the span.
    fn peek_operator(&self, operator: &str) -> bool {
        for (i, c) in operator.char_indices() {
            if let Some(TokenTree::Punct(punct)) = self.tokens.get(i) {
                if punct.as_char() != c {
                    return false;
                }

                // This code is disabled due to pretty-printing the implies operator and then
                // parsing it as == >. Possibly this gets fixed in the future, for now, we
                // recognize both ==> and == > as implies operators. Related issue:
                // https://github.com/viperproject/prusti-dev/issues/127

                // if i + 1 < operator.len() && punct.spacing() != Spacing::Joint {
                //     return false;
                // }
            } else {
                return false;
            }
        }
        true
    }
    /// Check whether the input starts with an operator. Does not set the span.
    fn peek_any_operator(&self) -> bool {
        self.peek_operator("==>") || self.peek_operator("&&")
    }
    /// Check if the input starts with the operator and if yes, consume it
    /// and set the span to it.
    fn check_and_consume_operator(&mut self, operator: &str) -> bool {
        if !self.peek_operator(operator) {
            return false;
        }
        let mut span: Option<Span> = None;
        for _ in operator.chars() {
            self.pop();
            if let Some(maybe_span) = span {
                span = maybe_span.join(self.span);
            }
            else {
                span = Some(self.span);
            }
        }
        self.span = span.unwrap();
        true
    }
    /// Check if the input starts with a parenthesized block and if yes,
    /// consume it and set the span to it.
    fn check_and_consume_parenthesized_block(&mut self) -> Option<Group> {
        if let Some(TokenTree::Group(group)) = self.tokens.front() {
            if group.delimiter() == Delimiter::Parenthesis {
                if let Some(TokenTree::Group(group)) = self.pop() {
                    self.span = group.span();
                    return Some(group);
                } else {
                    unreachable!();
                }
            }
        }
        None
    }
    /// Check if the input starts with a parenthesized block and if yes,
    /// set the span to it.
    fn peek_parenthesized_block(&mut self) -> bool {
        if let Some(TokenTree::Group(group)) = self.tokens.front() {
            if group.delimiter() == Delimiter::Parenthesis {
                self.span = group.span();
                return true;
            }
            else {
                return false;
            }
        }
        return false;
    }
    /// Check if the input contains an operator
    /// (including parenthesized subexpressions) and if yes,
    /// set the span to the first occurrence of it.
    fn contains_operator(&mut self, operator: &str) -> bool {
        let mut stream = self.clone();
        while !stream.is_empty() {
            if stream.check_and_consume_operator(operator) {
                self.span = stream.span;
                return true;
            }
            if let Some(TokenTree::Group(group)) = self.tokens.front() {
                let mut nested_stream = ParserStream::from_token_stream(group.stream());
                if nested_stream.contains_operator(operator) {
                    self.span = nested_stream.span;
                    return true;
                }
            }
            stream.pop();
        }
        false
    }
    /// Creates a TokenStream until a certain operator is met, or until
    /// the end of the stream (whichever comes first).
    /// The terminating operator is not consumed.
    fn create_stream_until(&mut self, operator: &str) -> TokenStream {
        let mut stream = TokenStream::new();
        let mut t = vec![];
        while !self.peek_operator(operator) && !self.is_empty() {
            t.push(self.pop().unwrap());
        }
        stream.extend(t.into_iter());
        stream
    }
    /// Convert the content into TokenStream.
    fn create_stream(&mut self) -> TokenStream {
        let mut stream = TokenStream::new();
        let mut t = vec![];
        while !self.is_empty() {
            t.push(self.pop().unwrap());
        }
        stream.extend(t.into_iter());
        stream
    }
}

/// The representation of an argument to `forall` (for example `a: i32`)
#[derive(Debug, Clone)]
pub struct Arg {
    pub name: syn::Ident,
    pub typ: syn::Type
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        Ok(Self{
            name,
            typ
        })
    }
}

/// The representation of all arguments to `forall`
/// (for example `a: i32, b: i32, c: i32`)
#[derive(Debug)]
struct ForAllArgs {
    args: syn::punctuated::Punctuated<Arg, Token![,]>
}

impl Parse for ForAllArgs {
    fn parse(input: ParseStream) -> syn::Result<Self>{
        let parsed: syn::punctuated::Punctuated<Arg, Token![,]> = input.parse_terminated(Arg::parse)?;
        Ok(Self{
            args: parsed
        })
    }
}

/// The structure to parse Prusti assertions.
///
/// Check common::AssertionKind to see all types of Prusti assertions.
///
/// Since `syn` can only parse the input as a whole, a preparsing phase
/// that recognizes the custom Prusti operators and keywords is needed. After
/// the input is split according to those, it can be parsed using `syn`,
/// and then is sticked back together to form Prusti assertions.
/// For a more high-level overview, please check `mod.rs`.
pub struct Parser {
    /// The helper to manipulate input.
    input: ParserStream,
    /// Members of the conjunction.
    conjuncts: Vec<AssertionWithoutId>,
    /// Currently being parsed Rust expression.
    expr: Vec<TokenTree>,
    /// A flag to denote whether the previous expression is already resolved
    /// (parsed into a conjunct).
    previous_expression_resolved: bool,
    /// A flag to denote that once the current expression finishes, an operator
    /// will be expected.
    expected_operator: bool,
    /// A flag to denote that the next token must be an operator.
    expected_only_operator: bool,
    /// A flag to denote that the parser is currently parsing a pledge
    /// containing a lhs. This is important so that the parser stops at the
    /// comma in between lhs and rhs.
    parsing_pledge_with_lhs: bool,
}

impl Parser {
    pub fn from_token_stream(tokens: TokenStream) -> Self {
        let input = ParserStream::from_token_stream(tokens);
        Self {
            input,
            conjuncts: Vec::new(),
            expr: Vec::new(),
            previous_expression_resolved: false,
            expected_operator: false,
            expected_only_operator: false,
            parsing_pledge_with_lhs: false,
        }
    }
    fn from_parser_stream(input: ParserStream) -> Self {
        Self {
            input,
            conjuncts: Vec::new(),
            expr: Vec::new(),
            previous_expression_resolved: false,
            expected_operator: false,
            expected_only_operator: false,
            parsing_pledge_with_lhs: false,
        }
    }
    fn resolve_and(&mut self) -> syn::Result<()>{
        // handles the case when there is no lhs of the && operator
        if !self.expected_operator {
            return Err(self.error_expected_assertion());
        }

        // convert the currently being parsed expression into a conjunct if not already
        // done so
        if self.previous_expression_resolved {
            self.previous_expression_resolved = false;
        }
        else {
            if self.expr.is_empty() {
                return Err(self.error_expected_assertion());
            }
            if let Err(err) = self.convert_expr_into_conjunct() {
                return Err(err);
            }
        }

        // handles the case when there is no rhs of the && operator
        if self.input.is_empty() {
            return Err(self.error_expected_assertion());
        }

        self.expected_operator = false;
        self.expected_only_operator = false;
        Ok(())
    }
    fn resolve_implies(&mut self) -> syn::Result<AssertionWithoutId>{
        // handles the case when there is no lhs of the ==> operator
        if !self.expected_operator {
            return Err(self.error_expected_assertion());
        }

        // convert the currently being parsed expression into a conjunct if not already
        // done so
        if !self.previous_expression_resolved {
            if self.expr.is_empty() {
                return Err(self.error_expected_assertion());
            }
            if let Err(err) = self.convert_expr_into_conjunct() {
                return Err(err);
            }
        }

        // handles the case when there is no rhs of the ==> operator
        if self.input.is_empty() {
            return Err(self.error_expected_assertion());
        }

        // recursively parse the rhs assertion; note that this automatically handles the
        // operator precedence: implication will be then weaker than conjunction
        let mut parser = Parser::from_parser_stream(
            mem::replace(&mut self.input, ParserStream::empty())
        );

        let lhs = self.conjuncts_to_assertion();
        if let Err(err) = lhs {
            return Err(err);
        }

        let rhs = parser.extract_assertion();
        if let Err(err) = rhs {
            return Err(err);
        }

        return Ok(AssertionWithoutId{
            kind: box common::AssertionKind::Implies(lhs.unwrap(), rhs.unwrap())
        });
    }
    fn resolve_forall(&mut self) -> syn::Result<()> {
        if self.expected_operator {
            return Err(self.error_expected_operator());
        }

        // check whether there is a parenthesized block after forall
        if let Some(group) = self.input.check_and_consume_parenthesized_block() {

            // construct a ParserStream off of the parenthesized block for further parsing
            let mut stream = ParserStream::from_token_stream(group.stream());

            // parse vars
            if !stream.check_and_consume_operator("|") {
                return Err(self.error_expected_or());
            }
            let token_stream = stream.create_stream_until("|");
            if token_stream.is_empty() {
                return Err(self.error_no_quantifier_arguments());
            }
            let all_args: ForAllArgs = syn::parse2(token_stream)?;
            if !stream.check_and_consume_operator("|") {
                return Err(self.error_expected_or());
            }
            let mut vars = vec![];
            for var in all_args.args {
                vars.push(Arg {
                    typ: var.typ,
                    name: var.name
                })
            }

            // parse body
            let token_stream = stream.create_stream_until(",");
            let mut parser = Parser::from_token_stream(token_stream);
            let body = parser.extract_assertion()?;

            // create triggers in case they are not present
            let mut trigger_set = TriggerSet(vec![]);

            // parse triggers (check if they are present at all)
            if stream.peek_operator(",") {
                stream.check_and_consume_operator(",");
                if !stream.check_and_consume_keyword("triggers") {
                    return Err(self.error_expected_triggers());
                }
                if !stream.check_and_consume_operator("=") {
                    return Err(self.error_expected_equals());
                }
                let token_stream = stream.create_stream();

                let maybe_arr: Result<syn::ExprArray, Error> = syn::parse2(token_stream);
                if let Err(err) = maybe_arr {
                    self.input.span = err.span();
                    return Err(self.error_expected_tuple());
                }
                let arr = maybe_arr.unwrap();
                self.input.span = arr.span();

                let mut vec_of_triggers = vec![];
                for item in arr.elems {
                    if let syn::Expr::Tuple(tuple) = item {
                        vec_of_triggers.push(
                            Trigger(tuple.elems
                                .into_iter()
                                .map(|x| ExpressionWithoutId {
                                    id: (),
                                    spec_id: common::SpecificationId::dummy(),
                                    expr: x })
                                .collect()
                            )
                        );
                    }
                    else {
                        self.input.span = item.span();
                        return Err(self.error_expected_tuple());
                    }
                }

                trigger_set = TriggerSet(vec_of_triggers);
            }

            let conjunct = AssertionWithoutId {
                kind: box common::AssertionKind::ForAll(
                    ForAllVars {
                        spec_id: common::SpecificationId::dummy(),
                        id: (),
                        vars
                    },
                    trigger_set,
                    body,
                )
            };

            self.conjuncts.push(conjunct);
            self.previous_expression_resolved = true;
            self.expected_only_operator = true;
            self.expected_operator = true;
            return Ok(());
        }
        else {
            return Err(self.error_expected_parenthesis());
        }
    }
    fn resolve_parenthesized_block(&mut self, group: Group) -> syn::Result<()>{
        // handling a parenthesized block
        if self.expected_only_operator {
            return Err(self.error_expected_operator());
        }

        if self.expr.is_empty() && (self.input.peek_any_operator() || self.input.is_empty()) {
            // if there is no expression currently being parsed (in which case the
            // parenthesized block would be a part of it) and there is a Prusti operator
            // or nothing at all after the parenthesized block, then we might parser
            // this as a Prusti assertion

            if self.expected_operator {
                return Err(self.error_expected_operator());
            }

            let mut parser = Parser::from_token_stream(group.stream());
            let conjunct = parser.extract_assertion();

            if let Err(err) = conjunct {
                return Err(err);
            }

            let conjunct = conjunct.unwrap();
            self.conjuncts.push(conjunct);
            self.previous_expression_resolved = true;
            self.expected_operator = true;
        }
        else{
            // if this was not the case, we just parse as a Rust expression and stick
            // it to any possible already being-parsed expression
            self.expr.push(TokenTree::Group(group));
        }
        Ok(())
    }
    fn resolve_rust_expr(&mut self) -> syn::Result<()> {
        // if nothing of the above happened, we just continue parsing as a Rust expression
        if self.expected_only_operator {
            self.input.pop();
            return Err(self.error_expected_operator());
        }

        let token = self.input.pop().unwrap();
        self.expr.push(token);
        self.expected_operator = true;
        Ok(())
    }
    /// Creates a single Prusti assertion from the input and returns it.
    pub fn extract_assertion(&mut self) -> syn::Result<AssertionWithoutId> {
        // detect possibly ambiguous input
        if self.input.contains_both_and_or() {
            return Err(self.error_ambiguous_expression());
        }

        // preparse the input into atomic Prusti assertions
        while !self.input.is_empty() {
            if self.input.check_and_consume_operator("&&") {
                if let Err(err) = self.resolve_and() {
                    return Err(err);
                }
            }
            else if self.input.check_and_consume_operator("==>") {
                return self.resolve_implies();
            }
            else if self.input.check_and_consume_keyword("forall") {
                if let Err(err) = self.resolve_forall() {
                    return Err(err);
                }
            }
            else if let Some(group) = self.input.check_and_consume_parenthesized_block() {
                if let Err(err) = self.resolve_parenthesized_block(group) {
                    return Err(err);
                }
            }
            else if self.parsing_pledge_with_lhs && self.input.peek_operator(",") {
                break;
            }
            else{
                if let Err(err) = self.resolve_rust_expr() {
                    return Err(err);
                }
            }
        }

        // when the input is parsed, convert the trailing expression into a conjunct (if any)
        if !self.expr.is_empty() {
            if let Err(err) = self.convert_expr_into_conjunct() {
                return Err(err);
            }
        }

        // build a conjunction off of the assertions parsed
        self.conjuncts_to_assertion()
    }
    fn parse_rust_expression(&mut self, tokens: TokenStream) -> syn::Result<syn::Expr> {
        let maybe_expr = syn::parse2(tokens.clone());
        if let Err(err) = maybe_expr {
            let mut stream = ParserStream::from_token_stream(tokens);
            // raise a better error when seeing implication as part of a Rust expression
            if stream.contains_operator("==>") {
                self.input.span = stream.span;
                return Err(self.error_expected_expr_without_implication());
            }
            return Err(err);
        }
        maybe_expr
    }
    pub fn extract_pledge(&mut self) -> syn::Result<PledgeWithoutId> {
        self.parsing_pledge_with_lhs = true;
        let mut pledge = self.extract_pledge_rhs_only()?;
        self.parsing_pledge_with_lhs = false;
        if !self.input.peek_operator(",") {
            return Err(self.error_expected_comma());
        }
        self.input.check_and_consume_operator(",");
        pledge.rhs = self.extract_assertion()?;
        Ok(pledge)
    }
    pub fn extract_pledge_rhs_only(&mut self) -> syn::Result<PledgeWithoutId> {
        let assertion = self.extract_assertion()?;
        Ok(PledgeWithoutId { rhs: assertion })
    }
    /// Convert all conjuncts into And assertion.
    fn conjuncts_to_assertion(&mut self) -> syn::Result<AssertionWithoutId> {
        let mut conjuncts = mem::replace(&mut self.conjuncts, Vec::new());

        // if there is just one conjunction, don't construct a conjunction and just return it
        // as a single assertion
        if conjuncts.len() == 1 {
            Ok(conjuncts.pop().unwrap())
        }
        else{
            Ok(AssertionWithoutId{
                kind: box common::AssertionKind::And(conjuncts)
            })
        }
    }
    /// Convert parsed Rust expression into a Prusti conjunct.
    fn convert_expr_into_conjunct(&mut self) -> syn::Result<()> {
        let expr = self.expr.clone();
        let mut token_stream = TokenStream::new();
        token_stream.extend(expr.into_iter());
        self.expr.clear();

        let parsed_expr = self.parse_rust_expression(token_stream.clone())?;

        let expr = ExpressionWithoutId {
            spec_id: common::SpecificationId::dummy(),
            id: (),
            expr: parsed_expr,
        };
        self.conjuncts.push(AssertionWithoutId{
            kind: box common::AssertionKind::Expr(expr)
        });
        Ok(())
    }
    fn error_expected_expr_without_implication(&self) -> syn::Error {
        syn::Error::new(self.input.span,
                        "`==>` cannot be part of Rust expression")
    }
    fn error_expected_assertion(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected Prusti assertion")
    }
    fn error_expected_operator(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected `&&` or `==>`")
    }
    fn error_expected_parenthesis(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected `(`")
    }
    fn error_expected_comma(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected `,`")
    }
    fn error_expected_or(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected `|`")
    }
    fn error_expected_triggers(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected `triggers`")
    }
    fn error_expected_equals(&self) -> syn::Error {
        syn::Error::new(self.input.span, "expected `=`")
    }
    fn error_expected_tuple(&self) -> syn::Error {
        syn::Error::new(self.input.span, "`triggers` must be an array of tuples containing Rust expressions")
    }
    fn error_ambiguous_expression(&self) -> syn::Error {
        syn::Error::new(
            self.input.span,
            "found `||` and `&&` in the same subexpression. \
            Hint: add parentheses to clarify the evaluation order.")
    }
    fn error_no_quantifier_arguments(&self) -> syn::Error {
        syn::Error::new(self.input.span, "a quantifier must have at least one argument")
    }
}
