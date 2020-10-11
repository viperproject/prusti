#![feature(box_syntax)]
#![feature(box_patterns)]
#![feature(drain_filter)]

mod extern_spec_rewriter;
mod rewriter;
mod parse_closure_macro;
mod spec_attribute_kind;
pub mod specifications;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::spanned::Spanned;
use syn::parse_quote;
use std::convert::{TryFrom, TryInto};

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

fn extract_prusti_attributes<'a>(item: &'a mut untyped::AnyFnItem) -> impl Iterator<Item=(SpecAttributeKind, TokenStream)> + 'a {
    item.attrs_mut().drain_filter(
        |attr|
            attr.path.segments.len() == 1
                && SpecAttributeKind::try_from(attr.path.segments[0].ident.to_string()).is_ok()
    ).map(
        |attr| attr.path.segments[0].ident.to_string().try_into().ok().map(
            |attr_kind| (attr_kind, attr.tokens)
        )
    ).flatten()
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

    let (generated_spec_items, generated_attributes) = handle_result!(
        generate_spec_and_assertions(prusti_attributes, &item)
    );

    quote!{
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
            SpecAttributeKind::Pledge => generate_for_pledge(attr_tokens, item),
            SpecAttributeKind::Pure => generate_for_pure(attr_tokens, item),
            SpecAttributeKind::Trusted => generate_for_trusted(attr_tokens, item),
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
        vec![parse_quote!(#[prusti::pre_spec_id_ref = #spec_id_str])],
    ))
}

/// Generate spec items and attributes to typecheck th and later retrieve "ensures" annotations.
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
        vec![parse_quote!(#[prusti::post_spec_id_ref = #spec_id_str])],
    ))
}

/// Generate spec items and attributes to typecheck and later retrieve "after_expiry" annotations.
fn generate_for_pledge(attr: TokenStream, item: &untyped::AnyFnItem) -> GeneratedResult {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id_rhs = rewriter.generate_spec_id();
    let spec_id_rhs_str = format!(":{}", spec_id_rhs);
    let pledge = rewriter.parse_pledge(spec_id_rhs, attr)?;
    let spec_item_rhs = rewriter.generate_spec_item_fn(
        rewriter::SpecItemType::Postcondition,
        spec_id_rhs,
        pledge.rhs,
        &item
    )?;
    Ok((
        vec![spec_item_rhs],
        vec![parse_quote!(#[prusti::pledge_spec_id_ref = #spec_id_rhs_str])],
    ))
}

/// Generate spec items and attributes to typecheck and later retrieve "pure" annotations.
fn generate_for_pure(_attr: TokenStream, _item: &untyped::AnyFnItem) -> GeneratedResult {
    Ok((
        vec![],
        vec![parse_quote!(#[prusti::pure])],
    ))
}

/// Generate spec items and attributes to typecheck and later retrieve "trusted" annotations.
fn generate_for_trusted(_attr: TokenStream, _item: &untyped::AnyFnItem) -> GeneratedResult {
    Ok((
        vec![],
        vec![parse_quote!(#[prusti::trusted])],
    ))
}

pub fn body_invariant(tokens: TokenStream) -> TokenStream {
    let mut rewriter = rewriter::AstRewriter::new();
    let spec_id = rewriter.generate_spec_id();
    let invariant = handle_result!(rewriter.parse_assertion(spec_id, tokens));
    let check = rewriter.generate_spec_loop(spec_id, invariant);
    quote! {
        if false {
            #check
        }
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
    let cl_spec = syn::parse::<ClosureWithSpec>(tokens.into());
    if let Err(err) = cl_spec {
        return err.to_compile_error();
    }
    let cl_spec = cl_spec.unwrap();

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
            cl_annotations.extend(quote! {
                #[prusti::pre_spec_id_ref = #spec_id_str]
            });
        }

        for e in cl_spec.posts {
            let spec_id = rewriter.generate_spec_id();
            let postcond = handle_result!(rewriter.parse_assertion(spec_id, e.to_token_stream()));
            postconds.push((spec_id, postcond));
            let spec_id_str = spec_id.to_string();
            cl_annotations.extend(quote! {
                #[prusti::post_spec_id_ref = #spec_id_str]
            });
        }

        let (spec_toks_pre, spec_toks_post) = rewriter.generate_cl_spec(preconds, postconds);
        let syn::ExprClosure {
            attrs, asyncness, movability, capture, or1_token,
            inputs, or2_token, output, body
        } = cl_spec.cl;

        let mut attrs_ts = TokenStream::new();
        for a in attrs {
            attrs_ts.extend(a.into_token_stream());
        }

        quote! {
            {
                #cl_annotations #attrs_ts
                let _prusti_closure =
                    #asyncness #movability #capture
                    #or1_token #inputs #or2_token #output
                    {
                        if false {
                            #spec_toks_pre
                        }
                        let result = #body ;
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
                let prusti_attributes: Vec<_> = extract_prusti_attributes(&mut method_item).collect();
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
                let new_item = parse_quote!{
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
    quote! {
        #spec_impl_block
        #impl_block
    }
}

pub fn extern_spec(_attr: TokenStream, tokens:TokenStream) -> TokenStream {
    let item: syn::Item = handle_result!(syn::parse2(tokens));
    match item {
        syn::Item::Impl(mut item_impl) => {
            let new_struct = handle_result!(extern_spec_rewriter::generate_new_struct(&mut item_impl));

            let struct_ident = &new_struct.ident;
            let generics = &new_struct.generics;

            let struct_ty: syn::Type = syn::parse_quote! {
                #struct_ident #generics
            };

            let rewritten_item =
                handle_result!(extern_spec_rewriter::rewrite_impl(&mut item_impl, Box::from(struct_ty)));

            quote! {
                #new_struct
                #rewritten_item
            }
        }
        syn::Item::Mod(mut item_mod) => {
            let mut path = syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            };
            handle_result!(extern_spec_rewriter::rewrite_mod(&mut item_mod, &mut path));
            quote! {
                #item_mod
            }
        }
        _ => { unimplemented!() }
    }
}
