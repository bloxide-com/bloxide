// Copyright 2025 Bloxide, all rights reserved
use core::sync::atomic::{AtomicUsize, Ordering};
use proc_macro::TokenStream;
use quote::{format_ident, quote};

pub(crate) static NEXT_ACTOR_ID: AtomicUsize = AtomicUsize::new(1);

pub(crate) struct ChannelEntry {
    pub msg_type: syn::Type,
    pub capacity: syn::LitInt,
}

pub(crate) struct ChannelsInput {
    pub runtime: syn::Type,
    pub entries: Vec<ChannelEntry>,
}

impl syn::parse::Parse for ChannelsInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let runtime: syn::Type = input.parse()?;
        let _: syn::Token![;] = input.parse()?;

        let mut entries = Vec::new();
        while !input.is_empty() {
            let msg_type: syn::Type = input.parse()?;
            let content;
            syn::parenthesized!(content in input);
            let capacity: syn::LitInt = content.parse()?;
            entries.push(ChannelEntry { msg_type, capacity });
            if input.peek(syn::Token![,]) {
                let _: syn::Token![,] = input.parse()?;
            }
        }

        Ok(ChannelsInput { runtime, entries })
    }
}

pub(crate) fn channels_inner(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as ChannelsInput);

    let n = parsed.entries.len();
    if n == 0 {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "channels! requires at least one message type",
        )
        .to_compile_error()
        .into();
    }

    let runtime = &parsed.runtime;
    let ref_idents: Vec<proc_macro2::Ident> = (1..=n).map(|i| format_ident!("r{}", i)).collect();
    let stream_idents: Vec<proc_macro2::Ident> = (1..=n).map(|i| format_ident!("s{}", i)).collect();
    let msg_types: Vec<&syn::Type> = parsed.entries.iter().map(|e| &e.msg_type).collect();
    let capacities: Vec<&syn::LitInt> = parsed.entries.iter().map(|e| &e.capacity).collect();

    let actor_id = NEXT_ACTOR_ID.fetch_add(1, Ordering::Relaxed);
    quote! {
        {
            #(
                let (#ref_idents, #stream_idents) =
                    <#runtime as ::bloxide_core::capability::StaticChannelCap>
                        ::channel::<#msg_types, #capacities>(#actor_id);
            )*
            ((#(#ref_idents,)*), (#(#stream_idents,)*))
        }
    }
    .into()
}
