use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::parse_quote_spanned;
use crate::span_overrider::SpanOverrider;
use crate::specifications::common::NameGenerator;
use proc_macro2::{Group, TokenStream, TokenTree};
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{ImplItemMethod, ItemUse};

/// Process external specifications in Rust modules marked with the
/// #[extern_spec] attribute. Nested modules are processed recursively.
/// Specifications are collected from functions and function stubs.
///
/// Modules are rewritten so that their name does not clash with the module
/// they are specifying.

/// To export the spec and code in one crate, macros for preserving the tokens are generated
pub fn rewrite_mod(item_mod: &mut syn::ItemMod, path: &mut syn::Path, rewrite_path: &mut syn::Path, macros: &mut Vec<syn::ItemMacro>) -> syn::Result<()>{
    if item_mod.content.is_none() {
        return Ok(());
    }

    path.segments.push(syn::PathSegment {
        ident: item_mod.ident.clone(),
        arguments: syn::PathArguments::None,
    });
    let mod_ident = item_mod.ident.clone();
    let name_generator = NameGenerator::new();
    let ident = syn::Ident::new(
        &name_generator.generate_mod_name(&item_mod.ident),
        item_mod.span(),
    );

    item_mod.ident = ident.clone();

    rewrite_path.segments.push(syn::PathSegment {
        ident: ident.clone(),
        arguments: syn::PathArguments::None,
    });

    // Record all the uses in this module, this is for preserving uses in function macro
    let mut uses = vec![];

    // Record every items' tokens for generating macro
    let mut macro_content_tokens = TokenStream::new();
    let span = item_mod.span();

    for item in item_mod.content.as_mut().unwrap().1.iter_mut() {
        let mut path = path.to_owned();
        let mut rewrite_path = rewrite_path.to_owned();
        match item {
            syn::Item::Fn(item_fn) => {
                rewrite_fn(item_fn, &mut path, &mut rewrite_path, macros, &uses)?;
                macro_content_tokens.extend(quote!(#item_fn));
            }
            syn::Item::Mod(inner_mod) => {
                rewrite_mod(inner_mod, &mut path, &mut rewrite_path, macros)?;
                macro_content_tokens.extend(quote!(#inner_mod));
            }
            syn::Item::Verbatim(tokens) => {
                // Transforms function stubs (functions with a `;` after the
                // signature instead of the body) into functions, then
                // processes them.
                let mut new_tokens = TokenStream::new();
                for mut token in tokens.clone().into_iter() {
                    if let TokenTree::Punct(punct) = &mut token {
                        if punct.as_char() == ';' {
                            new_tokens.extend(
                                Group::new(proc_macro2::Delimiter::Brace, TokenStream::new())
                                    .to_token_stream(),
                            );
                            continue;
                        }
                    }
                    new_tokens.extend(token.to_token_stream());
                }
                let res: Result<syn::Item, _> = syn::parse2(new_tokens);
                if res.is_err() {
                    return Err(syn::Error::new(item.span(), "invalid function signature"));
                }

                let mut item = res.unwrap();
                if let syn::Item::Fn(item_fn) = &mut item {
                    rewrite_fn(item_fn, &mut path, &mut rewrite_path, macros, &uses)?;
                    macro_content_tokens.extend(quote!(#item_fn));
                }
                *tokens = quote!(#item)
            }
            syn::Item::Use(item_use) => {
                uses.push(item_use.to_owned());
                macro_content_tokens.extend(quote!(#item_use));
            }
            _ => return Err(syn::Error::new(item.span(), "unexpected item")),
        }
    }

    let macro_content_tokens = expand_segements_as_mod(rewrite_path, macro_content_tokens);

    let macro_ident = hash_path_ident(mod_ident, path.to_owned());

    let final_item: syn::ItemMacro = syn::parse2(parse_quote_spanned!(span => 
            #[macro_export]
            macro_rules! #macro_ident {
                () => {
                    #macro_content_tokens
                };
            }
    ))?;

    macros.push(final_item);
    Ok(())
}

/// expand each segement of path as a mod declaration
fn expand_segements_as_mod(path: &syn::Path, content: TokenStream) -> TokenStream {
    let mut tokens = content;
    let segs = path.segments.iter().rev();
    for seg in segs {
        tokens = quote!(
            mod #seg {
                #tokens
            }
        )
    }
    tokens
}

/// Rewrite a specification function to a call to the specified function.
/// The result of this rewriting is then parsed in `ExternSpecResolver`.
/// macro that expands to the specification and function tokens are generated
fn rewrite_fn(item_fn: &mut syn::ItemFn, path: &mut syn::Path, rewrite_path: &mut syn::Path, macros: &mut Vec<syn::ItemMacro>, uses: &Vec<ItemUse>) -> syn::Result<()> {
    let ident = &item_fn.sig.ident;
    let args = &item_fn.sig.inputs;
    let item_fn_span = item_fn.span();
    path.segments.push(syn::PathSegment {
        ident: ident.to_owned(),
        arguments: syn::PathArguments::None,
    });
    item_fn.block = parse_quote_spanned! {item_fn_span=>
        {
            #path (#args);
            unimplemented!()
        }
    };

    item_fn
        .attrs
        .push(parse_quote_spanned!(item_fn_span=> #[prusti::extern_spec]));
    item_fn
        .attrs
        .push(parse_quote_spanned!(item_fn_span=> #[trusted]));
    let mut uses_tokens = TokenStream::new();
    uses_tokens.extend(uses.into_iter().map(|item_use| quote!(#item_use)));
    let macro_content_tokens = expand_segements_as_mod(rewrite_path, quote!(
        #uses_tokens
        #item_fn
    ));
    let macro_ident = hash_path_ident(ident.to_owned(), path.to_owned());
    macros.push(parse_quote_spanned!(item_fn_span =>
        #[macro_export] 
        macro_rules! #macro_ident {
            () => {
                #macro_content_tokens
            };
        }
    ));

    Ok(())

}

fn hash_path_ident<T: Hash>(ident: syn::Ident, path: T) -> syn::Ident {
    let path_hash = {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    };

    let span = ident.span();
    syn::Ident::new(format!("{}_{}", ident , path_hash).as_str(), span)
}

/// Rewrite all methods in an impl block to calls to the specified methods.
/// The result of this rewriting is then parsed in `ExternSpecResolver`.
pub fn rewrite_impl(
    impl_item: &mut syn::ItemImpl,
    new_struct: &syn::ItemStruct,
) -> syn::Result<TokenStream> {
    let item_ty = &mut impl_item.self_ty;
    if let syn::Type::Path(type_path) = item_ty.as_mut() {
        for seg in type_path.path.segments.iter_mut() {
            if let syn::PathArguments::AngleBracketed(genargs) = &mut seg.arguments {
                genargs.colon2_token = Some(syn::token::Colon2::default());
            }
        }
    }

    for item in impl_item.items.iter_mut() {
        let item_span = item.span();
        match item {
            syn::ImplItem::Method(method) => {
                for attr in method.attrs.iter_mut() {
                    attr.tokens = rewrite_self(attr.tokens.clone());
                }

                let args = rewrite_method_inputs(item_ty, method);
                let ident = &method.sig.ident;

                method
                    .attrs
                    .push(parse_quote_spanned!(item_span=> #[prusti::extern_spec]));
                method
                    .attrs
                    .push(parse_quote_spanned!(item_span=> #[trusted]));

                let mut method_path: syn::ExprPath = parse_quote_spanned! {ident.span()=>
                    #item_ty :: #ident
                };

                // Fix the span
                syn::visit_mut::visit_expr_path_mut(
                    &mut SpanOverrider::new(ident.span()),
                    &mut method_path,
                );

                method.block = parse_quote_spanned! {item_span=>
                    {
                        #method_path (#args);
                        unimplemented!()
                    }
                };
            }
            _ => {
                return Err(syn::Error::new(
                    item.span(),
                    "expected a method".to_string(),
                ));
            }
        }
    }
    let macro_tokens = generate_macro_from_impl(impl_item, new_struct)?;
    Ok(quote! {
        #macro_tokens
        #impl_item
    })
}

fn generate_macro_from_impl(impl_item: &mut syn::ItemImpl,  new_struct: &syn::ItemStruct) -> syn::Result<TokenStream> {
    let item_ty = &impl_item.self_ty;
    let item_span = impl_item.span();
    let struct_ident = &new_struct.ident;
    let generics = &new_struct.generics;

    let new_struct_ty: syn::Type = parse_quote_spanned! {item_span=>
        #struct_ident #generics
    };
    if let syn::Type::Path(type_path) = item_ty.as_ref() {
        if let Some(impl_ident_seg) = type_path.path.segments.last() {
            
            let mut method_tokens = TokenStream::new();
            for item in impl_item.items.iter() {
                match &item {
                    &syn::ImplItem::Method(method) => {
                        method_tokens.extend(generate_macro_from_method(&method, new_struct, item_ty));
                    },
                    _ => {
                        return Err(syn::Error::new(
                            item.span(),
                            "expected a method".to_string(),
                        ));
                    }
                }
            }

            let impl_ident = hash_path_ident(impl_ident_seg.to_owned().ident, type_path.path.to_owned());
            impl_item.self_ty = Box::from(new_struct_ty);
            return Ok(quote_spanned! {item_span => 
                #method_tokens
                #[macro_export]
                macro_rules! #impl_ident {
                    () => {
                        #new_struct
                        #impl_item
                    };
                }
            });
        }
    }
    
    return Err(syn::Error::new(item_span, "invalid struct".to_string()));
}

fn generate_macro_from_method(method: &ImplItemMethod, new_struct: &syn::ItemStruct, impl_type: &Box<syn::Type>) -> syn::Result<TokenStream> {
    let ident = &method.sig.ident;
    let item_span = method.span();
    let method_path: syn::Path = parse_quote_spanned! {ident.span()=>
        #impl_type :: #ident
    };
    let struct_ident = &new_struct.ident;
    let generics = &new_struct.generics;
    let new_struct_ty: syn::Type = parse_quote_spanned! {item_span=>
        #struct_ident #generics
    };
    let macro_ident = hash_path_ident(ident.to_owned(), method_path);

    Ok(quote_spanned! {item_span => 
        #[macro_export]
        macro_rules! #macro_ident {
            () => {
                #new_struct
                impl #new_struct_ty {
                    #method
                }
            };
        }
    })
}

fn rewrite_self(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let mut new_tokens = proc_macro2::TokenStream::new();
    for token in tokens.into_iter() {
        match token {
            proc_macro2::TokenTree::Group(group) => {
                let new_group =
                    proc_macro2::Group::new(group.delimiter(), rewrite_self(group.stream()));
                new_tokens.extend(new_group.to_token_stream());
            }
            proc_macro2::TokenTree::Ident(mut ident) => {
                if ident == "self" {
                    ident = proc_macro2::Ident::new("_self", ident.span());
                }
                new_tokens.extend(ident.into_token_stream());
            }
            _ => {
                new_tokens.extend(token.into_token_stream());
            }
        }
    }
    new_tokens
}

fn rewrite_method_inputs(
    item_ty: &Box<syn::Type>,
    method: &mut ImplItemMethod,
) -> syn::punctuated::Punctuated<syn::Expr, syn::token::Comma> {
    let mut args: syn::punctuated::Punctuated<syn::Expr, syn::token::Comma> =
        syn::punctuated::Punctuated::new();

    for input in method.sig.inputs.iter_mut() {
        let input_span = input.span();
        match input {
            syn::FnArg::Receiver(receiver) => {
                let and = if receiver.reference.is_some() {
                    // TODO: do lifetimes need to be specified here?
                    quote_spanned! {input_span=> &}
                } else {
                    quote! {}
                };
                let mutability = &receiver.mutability;
                let fn_arg: syn::FnArg = parse_quote_spanned! {input_span=>
                    _self : #and #mutability #item_ty
                };
                *input = fn_arg;
                let expr: syn::Expr = parse_quote_spanned!(input_span=> _self);
                args.push_value(expr);
            }
            syn::FnArg::Typed(typed) => {
                if let syn::Pat::Ident(ident) = &*typed.pat {
                    let arg = &ident.ident;
                    let expr: syn::Expr = syn::parse_quote!(#arg);
                    args.push_value(expr);
                }
            }
        }
        args.push_punct(syn::token::Comma::default());
    }
    args
}

/// Generate an empty struct to be able to define impl blocks (in
/// `rewrite_impl`) on it for its specification functions.
pub fn generate_new_struct(item: &syn::ItemImpl) -> syn::Result<syn::ItemStruct> {
    let name_generator = NameGenerator::new();
    let struct_name = match name_generator.generate_struct_name(item) {
        Ok(name) => name,
        Err(msg) => return Err(syn::Error::new(item.span(), msg)),
    };
    let struct_ident = syn::Ident::new(&struct_name, item.span());

    let mut new_struct: syn::ItemStruct = parse_quote_spanned! {item.span()=>
        struct #struct_ident {}
    };

    new_struct.generics = item.generics.clone();
    let generics = &new_struct.generics;

    let mut fields_str: String = String::new();

    // Add `PhantomData` markers for each type parameter to silence errors
    // about unused type parameters.
    for param in generics.params.iter() {
        let field = format!(
            "std::marker::PhantomData<{}>,",
            param.to_token_stream().to_string()
        );
        fields_str.push_str(&field);
    }

    let fields: syn::FieldsUnnamed = syn::parse_str(&format!("({})", fields_str)).unwrap();

    new_struct.fields = syn::Fields::Unnamed(fields);
    Ok(new_struct)
}
