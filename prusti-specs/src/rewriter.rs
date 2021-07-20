use crate::specifications::common::SpecificationIdGenerator;
use crate::specifications::untyped;
use proc_macro2::{TokenStream, Span};
use syn::{Type, punctuated::Punctuated, Pat, Token};
use syn::spanned::Spanned;
use quote::{quote_spanned, format_ident};
use crate::specifications::preparser::Parser;

pub(crate) struct AstRewriter {
    spec_id_generator: SpecificationIdGenerator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecItemType {
    Precondition,
    Postcondition,
    Pledge,
    Predicate,
}

impl std::fmt::Display for SpecItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecItemType::Precondition => write!(f, "pre"),
            SpecItemType::Postcondition => write!(f, "post"),
            SpecItemType::Pledge => write!(f, "pledge"),
            SpecItemType::Predicate => write!(f, "pred"),
        }
    }
}

impl AstRewriter {
    pub(crate) fn new() -> Self {
        Self {
            spec_id_generator: SpecificationIdGenerator::new(),
        }
    }

    pub fn generate_spec_id(&mut self) -> untyped::SpecificationId {
        self.spec_id_generator.generate()
    }

    /// Check whether function `item` contains a parameter called `keyword`. If
    /// yes, return its span.
    fn check_contains_keyword_in_params(&self, item: &untyped::AnyFnItem, keyword: &str) -> Option<Span> {
        for param in &item.sig().inputs {
            match param {
                syn::FnArg::Typed(syn::PatType {
                    pat,
                    ..
                }) => {
                    if let syn::Pat::Ident(syn::PatIdent { ident, .. }) = &**pat {
                        if ident == keyword {
                            return Some(param.span());
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    fn generate_result_arg(&self, item: &untyped::AnyFnItem) -> syn::FnArg {
        let item_span = item.span();
        let output_ty = match &item.sig().output {
            syn::ReturnType::Default => parse_quote_spanned!(item_span=> ()),
            syn::ReturnType::Type(_, ty) => ty.clone(),
        };
        let fn_arg = syn::FnArg::Typed(
            syn::PatType {
                attrs: Vec::new(),
                pat: Box::new(parse_quote_spanned!(item_span=> result)),
                colon_token: syn::Token![:](item.sig().output.span()),
                ty: output_ty,
            }
        );
        fn_arg
    }

    /// Turn an expression into the appropriate function
    pub fn generate_spec_item_fn(
        &mut self,
        spec_type: SpecItemType,
        spec_id: untyped::SpecificationId,
        expr: syn::Expr,
        item: &untyped::AnyFnItem,
    ) -> syn::Result<syn::Item> {
        if let Some(span) = self.check_contains_keyword_in_params(item, "result") {
            return Err(syn::Error::new(
                span,
                "it is not allowed to use the keyword `result` as a function argument".to_string(),
            ));
        }
        let item_span = item.span();
        let item_name = syn::Ident::new(
            &format!("prusti_{}_item_{}_{}", spec_type, item.sig().ident, spec_id),
            item_span,
        );
        let spec_id_str = spec_id.to_string();

        let mut spec_item: syn::ItemFn = parse_quote_spanned! {item_span =>
            #[allow(unused_must_use, unused_variables, dead_code)]
            #[prusti::spec_only]
            #[prusti::spec_id = #spec_id_str]
            fn #item_name() -> bool {
                #expr
            }
        };
        spec_item.sig.generics = item.sig().generics.clone();
        spec_item.sig.inputs = item.sig().inputs.clone();
        if spec_type != SpecItemType::Precondition {
            let fn_arg = self.generate_result_arg(item);
            spec_item.sig.inputs.push(fn_arg);
        }
        Ok(syn::Item::Fn(spec_item))
    }

    /// Parse an assertion into a Rust expression
    pub fn process_assertion(
        &mut self,
        spec_type: SpecItemType,
        spec_id: untyped::SpecificationId,
        tokens: TokenStream,
        item: &untyped::AnyFnItem,
    ) -> syn::Result<syn::Item> {
        self.generate_spec_item_fn(
            spec_type,
            spec_id,
            Parser::from_token_stream(tokens).extract_assertion_expr()?,
            item
        )
    }

    /// Parse a pledge without lhs into a Rust expression
    pub fn process_pledge_rhs_only(
        &mut self,
        spec_id: untyped::SpecificationId,
        tokens: TokenStream,
        item: &untyped::AnyFnItem,
    ) -> syn::Result<syn::Item> {
        self.generate_spec_item_fn(
            SpecItemType::Pledge,
            spec_id,
            Parser::from_token_stream(tokens).extract_pledge_rhs_only_expr()?,
            &item
        )
    }

    /// Parse a pledge with lhs into a Rust expression
    pub fn process_pledge(
        &mut self,
        spec_id: untyped::SpecificationId,
        tokens: TokenStream,
        item: &untyped::AnyFnItem,
    ) -> syn::Result<syn::Item> {
        self.generate_spec_item_fn(
            SpecItemType::Pledge,
            spec_id,
            Parser::from_token_stream(tokens).extract_pledge_expr()?,
            &item
        )
    }

    /// Parse a loop invariant into a Rust expression
    pub fn process_loop(
        &mut self,
        spec_id: untyped::SpecificationId,
        tokens: TokenStream,
    ) -> syn::Result<TokenStream> {
        let expr = Parser::from_token_stream(tokens).extract_assertion_expr()?;
        let spec_id_str = spec_id.to_string();
        let callsite_span = Span::call_site();
        Ok(quote_spanned! {callsite_span=>
            #[allow(unused_must_use, unused_variables)]
            {
                #[prusti::spec_only]
                #[prusti::loop_body_invariant_spec]
                #[prusti::spec_id = #spec_id_str]
                || -> bool {
                    #expr
                };
            }
        })
    }

    /// Parse a closure with specifications into a Rust expression
    pub fn process_closure(
        &mut self,
        inputs: Punctuated<Pat, Token![,]>,
        output: Type,
        preconds: Vec<(untyped::SpecificationId, syn::Item)>,
        postconds: Vec<(untyped::SpecificationId, syn::Item)>,
    ) -> syn::Result<(TokenStream, TokenStream)> {
        let process_cond = |is_post: bool, id: &untyped::SpecificationId,
                            assertion: &syn::Item| -> TokenStream
        {
            let spec_id_str = id.to_string();
            let name = format_ident!("prusti_{}_closure_{}", if is_post { "post" } else { "pre" }, spec_id_str);
            let callsite_span = Span::call_site();
            let result = if is_post && !inputs.empty_or_trailing() {
                quote_spanned! { callsite_span => , result: #output }
            } else if is_post {
                quote_spanned! { callsite_span => result: #output }
            } else {
                TokenStream::new()
            };
            quote_spanned! { callsite_span =>
                #[prusti::spec_only]
                #[prusti::spec_id = #spec_id_str]
                fn #name(#inputs #result) {
                    #assertion
                }
            }
        };

        let mut pre_ts = TokenStream::new();
        for (id, precond) in preconds {
            pre_ts.extend(process_cond(false, &id, &precond));
        }

        let mut post_ts = TokenStream::new();
        for (id, postcond) in postconds {
            post_ts.extend(process_cond(true, &id, &postcond));
        }

        Ok((pre_ts, post_ts))
    }
    /// Parse an assertion into a Rust expression
    pub fn process_closure_assertion(
        &mut self,
        spec_id: untyped::SpecificationId,
        tokens: TokenStream,
    ) -> syn::Result<syn::Item> {
        let expr = Parser::from_token_stream(tokens).extract_assertion_expr()?;
        let spec_id_str = spec_id.to_string();
        let callsite_span = Span::call_site();
        Ok(parse_quote_spanned! {callsite_span=>
            #[allow(unused_must_use, unused_variables)]
            {
                #[prusti::spec_only]
                #[prusti::spec_id = #spec_id_str]
                || -> bool {
                    #expr
                };
            }
        })
    }
}

pub fn translate_empty_assertion() -> syn::Expr {
    parse_quote_spanned! {
        Span::call_site() =>
        true
    }
}

pub fn translate_pledge_rhs_only(reference: Option<syn::Expr>, rhs: syn::Expr) -> syn::Expr {
    if let Some(reference) = reference {
        parse_quote_spanned! {
            // TODO: figure out span
            Span::call_site() =>
            pledge_rhs(#reference, #rhs)
        }
    } else {
        parse_quote_spanned! {
            // TODO: figure out span
            Span::call_site() =>
            pledge_rhs(result, #rhs)
        }
    }
}

pub fn translate_pledge(reference: Option<syn::Expr>, lhs: syn::Expr, rhs: syn::Expr) -> syn::Expr {
    if let Some(reference) = reference {
        parse_quote_spanned! {
            // TODO: figure out span
            Span::call_site() =>
            pledge(#reference, #lhs, #rhs)
        }
    } else {
        parse_quote_spanned! {
            // TODO: figure out span
            Span::call_site() =>
            pledge(result, #lhs, #rhs)
        }
    }
}

pub fn translate_implication(lhs: syn::Expr, rhs: syn::Expr) -> syn::Expr {
    parse_quote_spanned! {
        lhs.span().join(rhs.span()).unwrap() =>
        implication(#lhs, #rhs)
    }
}

pub fn translate_conjunction(mut conjuncts: Vec<syn::Expr>) -> syn::Expr {
    debug_assert!(conjuncts.len() != 0, "empty conjuncts given");
    if conjuncts.len() == 1 {
        conjuncts.pop().unwrap()
    } else {
        let last = conjuncts.pop().unwrap();
        let rest = translate_conjunction(conjuncts);
        parse_quote_spanned! {
            rest.span().join(last.span()).unwrap() =>
            #rest && #last
        }
    }
}

fn args_to_tokens(mut args: Vec<(syn::Ident, syn::Type)>) -> TokenStream {
    if args.len() == 0 {
        TokenStream::new()
    } else if args.len() == 1 {
        let (ident, typ) = args.pop().unwrap();
        quote_spanned! {
            ident.span().join(typ.span()).unwrap() =>
            #ident: #typ
        }
    } else {
        let (ident, typ) = args.pop().unwrap();
        let rest = args_to_tokens(args);
        quote_spanned! {
            rest.span().join(typ.span()).unwrap() =>
            #rest, #ident: #typ
        }
    }
}

pub fn translate_spec_entailment(closure: syn::Expr, args: Vec<(syn::Ident, syn::Type)>, pres: Vec<syn::Expr>, posts: Vec<syn::Expr>) -> syn::Expr {
    let arg_tokens = args_to_tokens(args);
    let pre_conjuncts = translate_conjunction(pres);
    let post_conjuncts = translate_conjunction(posts);
    parse_quote_spanned! {
        // TODO: get the right span
        Span::call_site() =>
        entailment(#closure, |#arg_tokens| {
            {
                #pre_conjuncts
            } &&
            {
                #post_conjuncts
            }
        })
    }
}

fn tuple_to_tokens(mut exprs: Vec<syn::Expr>) -> TokenStream {
    if exprs.len() == 0 {
        quote_spanned! {
            // TODO: get the right span
            Span::call_site() =>
            ,
        }
    } else if exprs.len() == 1 {
        let expr = exprs.pop().unwrap();
        quote_spanned! {
            expr.span() =>
            #expr,
        }
    } else {
        let last = exprs.pop().unwrap();
        let rest = tuple_to_tokens(exprs);
        quote_spanned! {
            rest.span().join(last.span()).unwrap() =>
            #rest #last,
        }
    }
}

pub fn translate_forall(vars: Vec<(syn::Ident, syn::Type)>, trigger_set: Vec<syn::Expr>, body: syn::Expr) -> syn::Expr {
    let arg_tokens = args_to_tokens(vars);
    let trigger_tuple = tuple_to_tokens(trigger_set);
    parse_quote_spanned! {
        // TODO: get the right span
        Span::call_site() =>
        forall((#trigger_tuple),
               |#arg_tokens| -> bool {
                   #body
               })
    }
}