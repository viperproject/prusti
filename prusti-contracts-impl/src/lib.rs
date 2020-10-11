extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn requires(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn ensures(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn pledge(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn pure(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn trusted(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro]
pub fn body_invariant(_tokens: TokenStream) -> TokenStream {
    (quote! { () }).into()
}

#[proc_macro]
pub fn closure(tokens: TokenStream) -> TokenStream {
    prusti_specs::closure(tokens.into(), true).into()
}

#[proc_macro_attribute]
pub fn refine_trait_spec(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn extern_spec(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}
