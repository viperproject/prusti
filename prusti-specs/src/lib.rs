#![deny(unused_must_use)]

#[macro_use]
mod parse_quote_spanned;
mod span_overrider;
mod extern_spec_rewriter;
mod rewriter;
mod parse_closure_macro;
mod spec_attribute_kind;
pub mod specifications;

use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned, ToTokens};
use syn::{spanned::Spanned};
use std::{collections::hash_map::DefaultHasher, convert::{TryInto}, hash::{Hasher, Hash}, vec};

use specifications::untyped;
use parse_closure_macro::ClosureWithSpec;
pub use spec_attribute_kind::SpecAttributeKind;

macro_rules! handle_result {
    ($parse_result: expr) => {
        match $parse_result {
            Ok(data) => data,
            Err(err) => return err.to_compile_error(),
        };
    };
}

fn extract_prusti_attributes(
    item: &mut untyped::AnyFnItem
) -> Vec<(SpecAttributeKind, TokenStream)> {
    let mut prusti_attributes = Vec::new();
    let mut regular_attributes = Vec::new();
    for attr in item.attrs_mut().drain(0..) {
        if attr.path.segments.len() == 1 {
            if let Ok(attr_kind) = attr.path.segments[0].ident.to_string().try_into() {
                let tokens = match attr_kind {
                    SpecAttributeKind::Requires
                    | SpecAttributeKind::Ensures
                    | SpecAttributeKind::AfterExpiry
                    | SpecAttributeKind::AfterExpiryIf => {
                        // We need to drop the surrounding parenthesis to make the
                        // tokens identical to the ones passed by the native procedural
                        // macro call.
                        let mut iter = attr.tokens.into_iter();
                        let tokens = if let Some(TokenTree::Group(group)) = iter.next() {
                            group.stream()
                        } else {
                            unreachable!()
                        };
                        assert!(iter.next().is_none(), "Unexpected shape of an attribute.");
                        tokens
                    }
                    // Nothing to do for attributes without arguments.
                    SpecAttributeKind::Pure
                    | SpecAttributeKind::Trusted
                    | SpecAttributeKind::Predicate => {
                        assert!(attr.tokens.is_empty(), "Unexpected shape of an attribute.");
                        attr.tokens
                    }
                };
                prusti_attributes.push((attr_kind, tokens));
            } else {
                regular_attributes.push(attr);
            }
        } else {
            regular_attributes.push(attr);
        }
    }
    *item.attrs_mut() = regular_attributes;
    prusti_attributes
}

/// Rewrite an item as required by *all* its specification attributes.
///
/// The first attribute (the outer one) needs to be passed via `attr_kind` and `attr` because
/// the compiler executes it as as a procedural macro attribute.
pub fn rewrite_prusti_attributes(
    outer_attr_kind: SpecAttributeKind,
    outer_attr_tokens: TokenStream,
    item_tokens: TokenStream,
) -> TokenStream {
    let mut item: untyped::AnyFnItem = handle_result!(syn::parse2(item_tokens));

    // Start with the outer attribute
    let mut prusti_attributes = vec![
        (outer_attr_kind, outer_attr_tokens)
    ];

    // Collect the remaining Prusti attributes, removing them from `item`.
    prusti_attributes.extend(extract_prusti_attributes(&mut item));

    // make sure to also update the check in the predicate! handling method
    if prusti_attributes
        .iter()
        .any(|(ak, _)| ak == &SpecAttributeKind::Predicate)
    {
        return syn::Error::new(
            item.span(),
            "`predicate!` is incompatible with other Prusti attributes",
        ).to_compile_error();
    }

    let (generated_spec_items, generated_attributes) = handle_result!(
        generate_spec_and_assertions(prusti_attributes, &item)
    );

    quote_spanned! {item.span()=>
        #(#generated_spec_items)*
        #(#generated_attributes)*
        #item
    }
}

type GeneratedResult = syn::Result<(Vec<syn::Item>, Vec<syn::Attribute>)>;

/// Generate spec items and attributes for `item` from the Prusti attributes
fn generate_spec_and_assertions(
    mut prusti_attributes: Vec<(SpecAttributeKind, TokenStream)>,
    item: &untyped::AnyFnItem,
) -> GeneratedResult {
    let mut generated_items = vec![];
    let mut generated_attributes = vec![];

    for (attr_kind, attr_tokens) in prusti_attributes.drain(..) {
        let rewriting_result = match attr_kind {
            SpecAttributeKind::Requires => generate_for_requires(attr_tokens, item),
            SpecAttributeKind::Ensures => generate_for_ensures(attr_tokens, item),
            SpecAttributeKind::AfterExpiry => generate_for_after_expiry(attr_tokens, item),
            SpecAttributeKind::AfterExpiryIf => generate_for_after_expiry_if(attr_tokens, item),
            SpecAttributeKind::Pure => generate_for_pure(attr_tokens, item),
            SpecAttributeKind::Trusted => generate_for_trusted(attr_tokens, item),
            // Predicates are handled separately below; the entry in the SpecAttributeKind enum
            // only exists so we successfully parse it and emit an error in
            // `check_incompatible_attrs`; so we'll never reach here.
            SpecAttributeKind::Predicate => unreachable!(),
        };
        let (new_items, new_attributes) = rewriting_result?;
        generated_items.extend(new_items);
        generated_attributes.extend(new_attributes);
    }

    Ok((generated_items, generated_attributes))
}

/// Generate spec items and attributes to typecheck the and later retrieve "requires" annotations.
fn generate_for_requires(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id = rewriter.generate_spec_id();
    let spec_id_str = spec_id.to_string();
    let assertion = rewriter.parse_assertion(spec_id, attr)?;
    let spec_item = rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Precondition,
        spec_id,
        assertion,
        &item
    )?;
    Ok((
        vec![spec_item],
        vec![parse_quote_spanned! {item.span()=>
            #[prusti::pre_spec_id_ref = #spec_id_str]
        }],
    ))
}

/// Generate spec items and attributes to typecheck the and later retrieve "ensures" annotations.
fn generate_for_ensures(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id = rewriter.generate_spec_id();
    let spec_id_str = spec_id.to_string();
    let assertion = rewriter.parse_assertion(spec_id, attr)?;
    let spec_item = rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Postcondition,
        spec_id,
        assertion,
        &item
    )?;
    Ok((
        vec![spec_item],
        vec![parse_quote_spanned! {item.span()=>
            #[prusti::post_spec_id_ref = #spec_id_str]
        }],
    ))
}

/// Check if the given expression is identifier `result`.
fn check_is_result(reference: &Option<untyped::Expression>) -> syn::Result<()> {
    if let Some(untyped::Expression { expr, ..}) = reference {
        if let syn::Expr::Path(syn::ExprPath { qself: None, path, ..}) = expr {
            if path.is_ident("result") {
                return Ok(());
            }
        }
        Err(syn::Error::new(
            expr.span(),
            "currently only `result` is supported".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Generate spec items and attributes to typecheck and later retrieve "after_expiry" annotations.
fn generate_for_after_expiry(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id_rhs = rewriter.generate_spec_id();
    let spec_id_rhs_str = format!(":{}", spec_id_rhs);
    let pledge = rewriter.parse_pledge(None, spec_id_rhs, attr)?;
    check_is_result(&pledge.reference)?;
    assert!(pledge.lhs.is_none(), "after_expiry with lhs?");
    let spec_item_rhs = rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Postcondition,
        spec_id_rhs,
        pledge.rhs,
        &item
    )?;
    Ok((
        vec![spec_item_rhs],
        vec![parse_quote_spanned! {item.span()=>
            #[prusti::pledge_spec_id_ref = #spec_id_rhs_str]
        }],
    ))
}

/// Generate spec items and attributes to typecheck and later retrieve "after_expiry_if"
/// annotations.
fn generate_for_after_expiry_if(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id_lhs = rewriter.generate_spec_id();
    let spec_id_rhs = rewriter.generate_spec_id();
    let spec_id_str = format!("{}:{}", spec_id_lhs, spec_id_rhs);
    let pledge = rewriter.parse_pledge(
        Some(spec_id_lhs),
        spec_id_rhs,
        attr
    )?;
    check_is_result(&pledge.reference)?;
    let spec_item_lhs = rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Postcondition,
        spec_id_lhs,
        pledge.lhs.unwrap(),
        &item
    )?;
    let spec_item_rhs = rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Postcondition,
        spec_id_rhs,
        pledge.rhs,
        &item
    )?;
    Ok((
        vec![spec_item_lhs, spec_item_rhs],
        vec![parse_quote_spanned! {item.span()=>
            #[prusti::pledge_spec_id_ref = #spec_id_str]
        }],
    ))
}

/// Generate spec items and attributes to typecheck and later retrieve "pure" annotations.
fn generate_for_pure(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    if !attr.is_empty() {
        return Err(syn::Error::new(
            attr.span(),
            "the `#[pure]` attribute does not take parameters"
        ));
    }

    Ok((
        vec![],
        vec![parse_quote_spanned! {item.span()=>
            #[prusti::pure]
        }],
    ))
}

/// Generate spec items and attributes to typecheck and later retrieve "trusted" annotations.
fn generate_for_trusted(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    if !attr.is_empty() {
        return Err(syn::Error::new(
            attr.span(),
            "the `#[trusted]` attribute does not take parameters"
        ));
    }

    Ok((
        vec![],
        vec![parse_quote_spanned! {item.span()=>
            #[prusti::trusted]
        }],
    ))
}

pub fn body_invariant(tokens: TokenStream) -> TokenStream {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id = rewriter.generate_spec_id();
    let invariant = handle_result!(rewriter.parse_assertion(spec_id, tokens));
    let check = rewriter.generate_spec_loop(spec_id, invariant);
    let callsite_span = Span::call_site();
    quote_spanned! {callsite_span=>
        #[allow(unused_must_use, unused_variables)]
        if false {
            #check
        }
    }
}

pub fn prusti_use(tokens: TokenStream) -> TokenStream {
    let path: syn::Path = handle_result!(syn::parse2(tokens.clone().into()));
    let path_span = path.span();

    match prusti_use_macro(path) {
        Ok(macro_path) => parse_quote_spanned!(path_span => #macro_path!();),
        Err(e) => e.to_compile_error()
    }
}

fn prusti_use_macro(path: syn::Path) -> syn::Result<syn::Path> {
    let span = path.span();
    let segs = path.segments;
    let mut macro_path = syn::Path {
        leading_colon: None,
        segments: syn::punctuated::Punctuated::new(),
    };
    let mut seg_iter = segs.into_iter();
    if let Some(crate_seg) = seg_iter.next() {
        macro_path.segments.push(crate_seg.to_owned());
        if let Some(last_seg) = seg_iter.clone().last() {
            let mut mod_path = syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            };
            mod_path.segments.extend(seg_iter);
            if let Some(seg) = mod_path.segments.last_mut() {
                if let syn::PathArguments::AngleBracketed(genargs) =  &mut seg.arguments {
                    genargs.colon2_token = Some(syn::token::Colon2::default());
                }
            }
            let path_hash = {
                let mut hasher = DefaultHasher::new();
                mod_path.hash(&mut hasher);
                hasher.finish()
            };
            macro_path.segments.push(syn::PathSegment {
                ident: syn::Ident::new(format!("{}_{}", last_seg.ident, path_hash).as_str(), span),
                arguments: syn::PathArguments::None,
            });
        } else {
            macro_path.segments.push(syn::PathSegment {
                ident: syn::Ident::new("crate_spec", span),
                arguments: syn::PathArguments::None,
            });
        }

        Ok(macro_path)
    } else {
        Err(syn::Error::new(span, "prusti_use must be given a path with non-empty segements"))
    }
}

/// Unlike the functions above, which are only called from
/// prusti-contracts-internal, this function also needs to be called
/// from prusti-contracts-impl, because we still need to parse the
/// macro in order to replace it with the closure definition.
/// Therefore, there is an extra parameter drop_spec here which tells
/// the function whether to keep the specification (for -internal) or
/// drop it (for -impl).
pub fn closure(tokens: TokenStream, drop_spec: bool) -> TokenStream {
    let cl_spec: ClosureWithSpec = handle_result!(syn::parse(tokens.into()));
    let callsite_span = Span::call_site();

    if drop_spec {
        cl_spec.cl.into_token_stream()
    } else {
        let mut rewriter = rewriter::AstRewriter::new();

        let mut preconds: Vec<(untyped::SpecificationId, untyped::Assertion)> = Vec::new();
        let mut postconds: Vec<(untyped::SpecificationId, untyped::Assertion)> = Vec::new();

        let mut cl_annotations = TokenStream::new();

        for r in cl_spec.pres {
            let spec_id = rewriter.generate_spec_id();
            let precond = handle_result!(rewriter.parse_assertion(spec_id, r.to_token_stream()));
            preconds.push((spec_id, precond));
            let spec_id_str = spec_id.to_string();
            cl_annotations.extend(quote_spanned! { callsite_span =>
                #[prusti::pre_spec_id_ref = #spec_id_str]
            });
        }

        for e in cl_spec.posts {
            let spec_id = rewriter.generate_spec_id();
            let postcond = handle_result!(rewriter.parse_assertion(spec_id, e.to_token_stream()));
            postconds.push((spec_id, postcond));
            let spec_id_str = spec_id.to_string();
            cl_annotations.extend(quote_spanned! { callsite_span =>
                #[prusti::post_spec_id_ref = #spec_id_str]
            });
        }

        let syn::ExprClosure {
            attrs, asyncness, movability, capture, or1_token,
            inputs, or2_token, output, body
        } = cl_spec.cl;

        let output_type: syn::Type = match output {
            syn::ReturnType::Default => {
                return syn::Error::new(output.span(), "closure must specify return type")
                    .to_compile_error();
            }
            syn::ReturnType::Type(_, ref ty) => (**ty).clone()
        };

        let (spec_toks_pre, spec_toks_post) = rewriter.generate_cl_spec(
            inputs.clone(), output_type, preconds, postconds);

        let mut attrs_ts = TokenStream::new();
        for a in attrs {
            attrs_ts.extend(a.into_token_stream());
        }

        quote_spanned! {callsite_span=>
            {
                #[allow(unused_variables)]
                #[prusti::closure]
                #cl_annotations #attrs_ts
                let _prusti_closure =
                    #asyncness #movability #capture
                    #or1_token #inputs #or2_token #output
                    {
                        #[allow(unused_must_use)]
                        if false {
                            #spec_toks_pre
                        }
                        let result = #body ;
                        #[allow(unused_must_use)]
                        if false {
                            #spec_toks_post
                        }
                        result
                    };
                _prusti_closure
            }
        }
    }
}

pub fn refine_trait_spec(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    let mut impl_block: syn::ItemImpl = handle_result!(syn::parse2(tokens));
    let mut new_items = Vec::new();
    let mut generated_spec_items = Vec::new();
    for item in impl_block.items {
        match item {
            syn::ImplItem::Method(method) => {
                let mut method_item = untyped::AnyFnItem::ImplMethod(method);
                let prusti_attributes: Vec<_> = extract_prusti_attributes(&mut method_item);
                let (spec_items, generated_attributes) = handle_result!(
                    generate_spec_and_assertions(prusti_attributes, &method_item)
                );
                generated_spec_items.extend(spec_items.into_iter().map(|spec_item| {
                    match spec_item {
                        syn::Item::Fn(spec_item_fn) => {
                            syn::ImplItem::Method(syn::ImplItemMethod {
                                attrs: spec_item_fn.attrs,
                                vis: spec_item_fn.vis,
                                defaultness: None,
                                sig: spec_item_fn.sig,
                                block: *spec_item_fn.block,
                            })
                        }
                        x => unimplemented!("Unexpected variant: {:?}", x),
                    }
                }));
                let new_item = parse_quote_spanned! {method_item.span()=>
                    #(#generated_attributes)*
                    #method_item
                };
                new_items.push(new_item);
            }
            _ => {}
        }
    }
    impl_block.items = new_items;
    let spec_impl_block = syn::ItemImpl {
        attrs: Vec::new(),
        defaultness: impl_block.defaultness,
        unsafety: impl_block.unsafety,
        impl_token: impl_block.impl_token,
        generics: impl_block.generics.clone(),
        trait_: None,
        self_ty: impl_block.self_ty.clone(),
        brace_token: impl_block.brace_token,
        items: generated_spec_items,
    };
    quote_spanned! {impl_block.span()=>
        #spec_impl_block
        #impl_block
    }
}


pub fn extern_spec(_attr: TokenStream, tokens:TokenStream) -> TokenStream {
    let item: syn::Item = handle_result!(syn::parse2(tokens));
    let item_span = item.span();
    let tokens = match item {
        syn::Item::Impl(mut item_impl) => {
            let new_struct = handle_result!(
                extern_spec_rewriter::generate_new_struct(&item_impl)
            );

            let rewritten_item = handle_result!(
                extern_spec_rewriter::rewrite_impl(&mut item_impl, &new_struct)
            );

            quote_spanned! {item_span=>
                #new_struct
                #rewritten_item
            }
        }
        syn::Item::Mod(mut item_mod) => {
            let mut path = syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            };

            let mut rewrite_path = syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            };

            let mut macros = vec![];
            handle_result!(extern_spec_rewriter::rewrite_mod(&mut item_mod, &mut path, &mut rewrite_path, &mut macros));

            let mut macro_tokens = TokenStream::new();
            macro_tokens.extend(macros.into_iter().map(|item_mac| quote!(#item_mac)));

            parse_quote_spanned!{item_span =>
                #macro_tokens
                #item_mod
                
            }
        }
        _ => { unimplemented!() }
    };
    tokens
}
#[derive(Debug)]
struct PredicateFn {
    fn_sig: syn::Signature,
    body: TokenStream,
}

impl syn::parse::Parse for PredicateFn {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let fn_sig = input.parse()?;
        let brace_content;
        let _brace_token = syn::braced!(brace_content in input);
        let body = brace_content.parse()?;

        Ok(PredicateFn { fn_sig, body })
    }
}

pub fn predicate(tokens: TokenStream) -> TokenStream {
    let tokens_span = tokens.span();
    // emit a custom error to the user instead of a parse error
    let pred_fn: PredicateFn = handle_result!(
        syn::parse2(tokens)
            .map_err(|e| syn::Error::new(
                e.span(),
                "`predicate!` can only be used on function definitions. it supports no attributes."
            ))
    );

    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id = rewriter.generate_spec_id();
    let assertion = handle_result!(rewriter.parse_assertion(spec_id, pred_fn.body));

    let sig = pred_fn.fn_sig.to_token_stream();
    let cleaned_fn: untyped::AnyFnItem = parse_quote_spanned! {tokens_span =>
        #sig {
            unimplemented!("predicate")
        }
    };

    let spec_fn = handle_result!(rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Predicate,
        spec_id,
        assertion,
        &cleaned_fn,
    ));

    let spec_id_str = spec_id.to_string();
    parse_quote_spanned! {cleaned_fn.span() =>
        // this is to typecheck the assertion
        #spec_fn

        // this is the assertion's remaining, empty fn
        #[allow(unused_must_use, unused_variables, dead_code)]
        #[prusti::pure]
        #[prusti::trusted]
        #[prusti::pred_spec_id_ref = #spec_id_str]
        #cleaned_fn
    }
}
