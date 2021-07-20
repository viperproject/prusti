extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote_spanned;

#[proc_macro_attribute]
pub fn requires(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn ensures(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn after_expiry(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}

#[proc_macro_attribute]
pub fn after_expiry_if(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
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
    let callsite_span = Span::call_site();
    (quote_spanned!(callsite_span=> ())).into()
}

#[proc_macro]
pub fn prusti_use(_tokens: TokenStream) -> TokenStream {
    let callsite_span = Span::call_site();
    (quote_spanned!(callsite_span => )).into()
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
pub fn extern_spec(_attr: TokenStream, _tokens: TokenStream) -> TokenStream {
    let callsite_span = Span::call_site();
    (quote_spanned!(callsite_span => )).into()
}

#[proc_macro_attribute]
pub fn predicate(_attr: TokenStream, tokens: TokenStream) -> TokenStream {
    tokens
}
