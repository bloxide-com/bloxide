// Copyright 2025 Bloxide, all rights reserved
use core::sync::atomic::Ordering;
use proc_macro::TokenStream;
use quote::{format_ident, quote};

use crate::channels::{ChannelsInput, NEXT_ACTOR_ID};

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

    // Bake the ID from the same compile-time counter used by `channels!` and
    // `next_actor_id!` — static wiring never touches the runtime's dynamic
    // `alloc_actor_id` counter (reserved for dynamically spawned actors).
    let actor_id = NEXT_ACTOR_ID.fetch_add(1, Ordering::Relaxed);
    quote! {
        {
            // Compile-time guard: statically wired actor IDs live below
            // `DYNAMIC_ACTOR_ID_BASE` (hard limit: 255 statically wired
            // actors per system). Exceeding the limit fails compilation here.
            const _: () = assert!(
                #actor_id < ::bloxide_core::capability::DYNAMIC_ACTOR_ID_BASE,
                "statically wired actor limit exceeded: compile-time actor IDs must stay below DYNAMIC_ACTOR_ID_BASE"
            );
            #(
                let (#ref_idents, #stream_idents) =
                    <#runtime as ::bloxide_core::capability::DynamicChannelCap>
                        ::channel::<#msg_types>(#actor_id, #capacities);
            )*
            ((#(#ref_idents,)*), (#(#stream_idents,)*))
        }
    }
    .into()
}
