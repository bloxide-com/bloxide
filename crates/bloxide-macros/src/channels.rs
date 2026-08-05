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
            // Compile-time guard: statically wired actor IDs live below
            // `DYNAMIC_ACTOR_ID_BASE` (hard limit: 255 statically wired
            // actors per system). Exceeding the limit fails compilation here.
            const _: () = assert!(
                #actor_id < ::bloxide_core::capability::DYNAMIC_ACTOR_ID_BASE,
                "statically wired actor limit exceeded: compile-time actor IDs must stay below DYNAMIC_ACTOR_ID_BASE"
            );
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

#[cfg(test)]
mod tests {
    use super::ChannelsInput;
    use quote::ToTokens;

    /// Render a parsed type back to a compact string for comparison.
    fn type_string(ty: &syn::Type) -> String {
        ty.to_token_stream().to_string().replace(' ', "")
    }

    /// Grammar: `channels!(Runtime; Msg(CAP), ...)` — the leading runtime
    /// type is mandatory, so empty input is rejected at the first parse step.
    #[test]
    fn empty_input_is_error() {
        assert!(syn::parse_str::<ChannelsInput>("").is_err());
    }

    /// The runtime argument is parsed as `syn::Type`; a non-type token is
    /// rejected before any channel entries are considered.
    #[test]
    fn malformed_runtime_is_error() {
        assert!(syn::parse_str::<ChannelsInput>("123; Msg(16)").is_err());
    }

    /// Capacity is a mandatory parenthesized `LitInt` after each message
    /// type: both `Msg` (no parens at all) and `Msg()` (empty parens) are
    /// errors.
    #[test]
    fn missing_capacity_is_error() {
        assert!(syn::parse_str::<ChannelsInput>("MockRuntime; Msg").is_err());
        assert!(syn::parse_str::<ChannelsInput>("MockRuntime; Msg()").is_err());
    }

    /// A comma after the final entry is accepted: the parser consumes an
    /// optional comma after every entry and stops once the stream is empty.
    /// This locks in the current lenient trailing-comma behavior.
    #[test]
    fn trailing_comma_is_accepted() {
        let parsed =
            syn::parse_str::<ChannelsInput>("MockRuntime; Msg(16),").expect("should parse");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(
            parsed.entries[0].capacity.base10_parse::<usize>().unwrap(),
            16
        );
    }

    /// Valid multi-channel input parses into the runtime type plus one entry
    /// per `Msg(CAP)` pair, preserving order.
    #[test]
    fn multi_channel_input_parses() {
        let parsed = syn::parse_str::<ChannelsInput>("MockRuntime; PingPongMsg(16), SomeMsg(8)")
            .expect("valid input should parse");
        assert_eq!(type_string(&parsed.runtime), "MockRuntime");
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(type_string(&parsed.entries[0].msg_type), "PingPongMsg");
        assert_eq!(
            parsed.entries[0].capacity.base10_parse::<usize>().unwrap(),
            16
        );
        assert_eq!(type_string(&parsed.entries[1].msg_type), "SomeMsg");
        assert_eq!(
            parsed.entries[1].capacity.base10_parse::<usize>().unwrap(),
            8
        );
    }

    /// The parser itself does not require any entries; rejecting an empty
    /// entry list is `channels_inner`'s job ("channels! requires at least
    /// one message type"), which runs after parsing.
    #[test]
    fn runtime_without_entries_parses_to_empty_list() {
        let parsed = syn::parse_str::<ChannelsInput>("MockRuntime;").expect("should parse");
        assert_eq!(type_string(&parsed.runtime), "MockRuntime");
        assert!(parsed.entries.is_empty());
    }
}
