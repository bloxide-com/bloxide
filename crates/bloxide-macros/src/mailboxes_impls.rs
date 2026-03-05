use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

pub(crate) fn mailboxes_impls_inner(input: TokenStream) -> TokenStream {
    let n_lit = syn::parse_macro_input!(input as syn::LitInt);
    let n: usize = match n_lit.base10_parse() {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let mut output = TokenStream2::new();

    for arity in 1..=n {
        let type_params: Vec<proc_macro2::Ident> =
            (1..=arity).map(|i| format_ident!("S{}", i)).collect();
        let indices: Vec<syn::Index> = (0..arity).map(syn::Index::from).collect();

        let from_bounds: Vec<TokenStream2> = type_params
            .iter()
            .map(|tp| quote! { ::core::convert::From<#tp::Item> })
            .collect();
        let stream_bounds: Vec<TokenStream2> = type_params
            .iter()
            .map(|tp| quote! { #tp: ::futures_core::Stream + Unpin + Send + 'static })
            .collect();
        let item_bounds: Vec<TokenStream2> = type_params
            .iter()
            .map(|tp| quote! { #tp::Item: Send + 'static })
            .collect();

        // Each match arm is pre-built so the repetition body is simple.
        let match_arms: Vec<TokenStream2> = type_params
            .iter()
            .zip(indices.iter())
            .enumerate()
            .map(|(i, (_tp, idx))| {
                let msg = format!("mailbox stream {i} closed — self-sender invariant violated");
                quote! {
                    match ::core::pin::Pin::new(&mut self.#idx).poll_next(cx) {
                        ::core::task::Poll::Ready(::core::option::Option::Some(item)) => {
                            return ::core::task::Poll::Ready(E::from(item));
                        }
                        ::core::task::Poll::Ready(::core::option::Option::None) => {
                            debug_assert!(false, #msg);
                        }
                        ::core::task::Poll::Pending => {}
                    }
                }
            })
            .collect();

        output.extend(quote! {
            impl<E, #(#type_params),*> Mailboxes<E> for (#(#type_params,)*)
            where
                E: Send + 'static #(+ #from_bounds)*,
                #(#stream_bounds,)*
                #(#item_bounds,)*
            {
                fn poll_next(
                    &mut self,
                    cx: &mut ::core::task::Context<'_>,
                ) -> ::core::task::Poll<E> {
                    use ::futures_core::Stream;
                    #(#match_arms)*
                    ::core::task::Poll::Pending
                }
            }
        });
    }

    output.into()
}
