// Copyright 2025 Bloxide, all rights reserved
use proc_macro::TokenStream;
use quote::{format_ident, quote};

use crate::channels::ChannelsInput;

pub(crate) fn dyn_channels_inner(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as ChannelsInput);

    let n = parsed.entries.len();
    if n == 0 {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "dyn_channels! requires at least one message type",
        )
        .to_compile_error()
        .into();
    }

    let runtime = &parsed.runtime;
    let ref_idents: Vec<proc_macro2::Ident> = (1..=n).map(|i| format_ident!("r{}", i)).collect();
    let stream_idents: Vec<proc_macro2::Ident> = (1..=n).map(|i| format_ident!("s{}", i)).collect();
    let msg_types: Vec<&syn::Type> = parsed.entries.iter().map(|e| &e.msg_type).collect();
    let capacities: Vec<&syn::LitInt> = parsed.entries.iter().map(|e| &e.capacity).collect();

    quote! {
        {
            let __actor_id = <#runtime as ::bloxide_core::capability::DynamicChannelCap>::alloc_actor_id();
            #(
                let (#ref_idents, #stream_idents) =
                    <#runtime as ::bloxide_core::capability::DynamicChannelCap>
                        ::channel::<#msg_types>(__actor_id, #capacities);
            )*
            ((#(#ref_idents,)*), (#(#stream_idents,)*))
        }
    }
    .into()
}
