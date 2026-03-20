// Copyright 2025 Bloxide, all rights reserved.
//! Proc macro for generating event types from mailbox specifications.
//!
//! ## Syntax
//!
//! ```ignore
//! // Single mailbox:
//! blox_event!(Ping { Msg: PingPongMsg });
//!
//! // Multi-mailbox with generics:
//! blox_event!(Worker<R: BloxRuntime> {
//!     Peer: PeerCtrl<WorkerMsg, R>,
//!     Msg: WorkerMsg,
//! });
//! ```
//!
//! Generates:
//! - `pub enum {Name}Event<...> { Lifecycle, ... }`
//! - `EventTag` and `LifecycleEvent` impls
//! - Tag constants and accessor methods
//! - Lifecycle convenience constructors

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

/// Parsed input for the `blox_event!` macro.
pub struct BloxEventInput {
    blox_name: syn::Ident,
    generics: syn::Generics,
    mailboxes: Vec<MailboxSpec>,
}

/// A single mailbox specification.
pub struct MailboxSpec {
    variant_ident: syn::Ident,
    msg_type: syn::Type,
}

impl syn::parse::Parse for BloxEventInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Parse: BloxName<generics> { Variant: Type, ... }
        let blox_name: syn::Ident = input.parse()?;

        // Parse optional generics
        let generics: syn::Generics = if input.peek(syn::Token![<]) {
            input.parse()?
        } else {
            syn::Generics::default()
        };

        let content;
        syn::braced!(content in input);

        let mut mailboxes = Vec::new();
        while !content.is_empty() {
            // Parse: VariantName: MsgType,
            let variant_ident: syn::Ident = content.parse()?;
            let _colon: syn::Token![:] = content.parse()?;
            let msg_type: syn::Type = content.parse()?;

            mailboxes.push(MailboxSpec {
                variant_ident,
                msg_type,
            });

            if !content.is_empty() {
                let _comma: syn::Token![,] = content.parse()?;
            }
        }

        // Optional trailing semicolon
        let _semi: Option<syn::Token![;]> = input.parse().ok();

        Ok(BloxEventInput {
            blox_name,
            generics,
            mailboxes,
        })
    }
}

pub(crate) fn blox_event_inner(input: &BloxEventInput) -> syn::Result<TokenStream2> {
    // Generate event name: {BloxName}Event
    let event_ident = format_ident!("{}Event", input.blox_name);
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Generate enum variants: Lifecycle, then each mailbox
    let mut enum_variants = Vec::new();
    enum_variants.push(quote! {
        /// Lifecycle command (Start/Reset/Stop/Ping).
        Lifecycle(::bloxide_core::lifecycle::LifecycleCommand)
    });

    for mailbox in &input.mailboxes {
        let variant_ident = &mailbox.variant_ident;
        let msg_type = &mailbox.msg_type;
        enum_variants.push(quote! {
            #variant_ident(::bloxide_core::messaging::Envelope<#msg_type>)
        });
    }

    // Generate the event enum (always pub)
    let enum_def = quote! {
        #[derive(Debug)]
        pub enum #event_ident #ty_generics #where_clause {
            #(#enum_variants),*
        }
    };

    // Generate From<Envelope<M>> for each mailbox type
    let mut from_impls = Vec::new();
    for mailbox in &input.mailboxes {
        let variant_ident = &mailbox.variant_ident;
        let msg_type = &mailbox.msg_type;

        from_impls.push(quote! {
            impl #impl_generics ::core::convert::From<::bloxide_core::messaging::Envelope<#msg_type>> for #event_ident #ty_generics #where_clause {
                fn from(env: ::bloxide_core::messaging::Envelope<#msg_type>) -> Self {
                    #event_ident::#variant_ident(env)
                }
            }
        });
    }

    // Generate From<LifecycleCommand>
    let from_lifecycle = quote! {
        impl #impl_generics ::core::convert::From<::bloxide_core::lifecycle::LifecycleCommand> for #event_ident #ty_generics #where_clause {
            fn from(cmd: ::bloxide_core::lifecycle::LifecycleCommand) -> Self {
                #event_ident::Lifecycle(cmd)
            }
        }
    };

    // Generate EventTag impl
    let mut event_tag_arms = Vec::new();
    event_tag_arms.push(quote! {
        Self::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG
    });

    for (idx, mailbox) in input.mailboxes.iter().enumerate() {
        let variant_ident = &mailbox.variant_ident;
        let tag = idx as u8;
        event_tag_arms.push(quote! {
            Self::#variant_ident(..) => #tag
        });
    }

    let event_tag_impl = quote! {
        impl #impl_generics ::bloxide_core::event_tag::EventTag for #event_ident #ty_generics #where_clause {
            #[inline]
            fn event_tag(&self) -> u8 {
                match self {
                    #(#event_tag_arms,)*
                }
            }
        }
    };

    // Generate LifecycleEvent impl
    let lifecycle_impl = quote! {
        impl #impl_generics ::bloxide_core::event_tag::LifecycleEvent for #event_ident #ty_generics #where_clause {
            fn as_lifecycle_command(&self) -> ::core::option::Option<::bloxide_core::lifecycle::LifecycleCommand> {
                match self {
                    Self::Lifecycle(cmd) => ::core::option::Option::Some(*cmd),
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    // Generate tag constants and accessor methods
    let mut tag_constants = Vec::new();
    let mut accessor_methods = Vec::new();

    for (idx, mailbox) in input.mailboxes.iter().enumerate() {
        let variant_ident = &mailbox.variant_ident;
        let msg_type = &mailbox.msg_type;
        let tag = idx as u8;

        // Tag constant
        let upper_snake = to_upper_snake_case(&variant_ident.to_string());
        let const_name = format_ident!("{}_TAG", upper_snake);
        tag_constants.push(quote! {
            /// Event tag for this variant, used for fast dispatch filtering.
            pub const #const_name: u8 = #tag;
        });

        // Accessor methods
        let snake_name = to_snake_case(&variant_ident.to_string());
        let envelope_method = format_ident!("{}_envelope", snake_name);
        let payload_method = format_ident!("{}_payload", snake_name);

        accessor_methods.push(quote! {
            /// Returns the envelope if this event matches this variant.
            pub fn #envelope_method(&self) -> ::core::option::Option<&::bloxide_core::messaging::Envelope<#msg_type>> {
                match self {
                    Self::#variant_ident(env) => ::core::option::Option::Some(env),
                    _ => ::core::option::Option::None,
                }
            }

            /// Returns the message payload if this event matches this variant.
            pub fn #payload_method(&self) -> ::core::option::Option<&#msg_type> {
                match self {
                    Self::#variant_ident(env) => ::core::option::Option::Some(&env.1),
                    _ => ::core::option::Option::None,
                }
            }
        });
    }

    // Generate lifecycle helpers
    let lifecycle_helpers = quote! {
        /// Create a Start lifecycle event.
        pub fn start() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Start)
        }

        /// Create a Reset lifecycle event.
        pub fn reset() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Reset)
        }

        /// Create a Stop lifecycle event.
        pub fn stop() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Stop)
        }

        /// Create a Ping lifecycle event.
        pub fn ping() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Ping)
        }
    };

    let impl_block = quote! {
        impl #impl_generics #event_ident #ty_generics #where_clause {
            #(#tag_constants)*
            #(#accessor_methods)*
            #lifecycle_helpers
        }
    };

    Ok(quote! {
        #enum_def
        #(#from_impls)*
        #from_lifecycle
        #event_tag_impl
        #lifecycle_impl
        #impl_block
    })
}

/// Convert PascalCase to UPPER_SNAKE_CASE.
fn to_upper_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            let prev_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit();
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if prev_lower || (chars[i - 1].is_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            let prev_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit();
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if prev_lower || (chars[i - 1].is_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_single_mailbox() {
        let input: BloxEventInput = syn::parse2(quote! {
            Ping { Msg: PingPongMsg }
        })
        .unwrap();

        let output = blox_event_inner(&input).unwrap();
        let output_str = output.to_string();

        // Verify event name is auto-generated
        assert!(output_str.contains("enum PingEvent"));
        assert!(output_str.contains("Lifecycle"));
        assert!(output_str.contains("Msg ("));
        assert!(output_str.contains("Envelope < PingPongMsg"));

        // Verify tag constant
        assert!(output_str.contains("MSG_TAG"));

        // Verify accessor
        assert!(output_str.contains("msg_envelope"));
        assert!(output_str.contains("msg_payload"));

        // Verify lifecycle helpers
        assert!(output_str.contains("pub fn start"));
    }

    #[test]
    fn test_multi_mailbox_with_generics() {
        let input: BloxEventInput = syn::parse2(quote! {
            Worker<R: BloxRuntime> {
                Peer: PeerCtrl<WorkerMsg, R>,
                Msg: WorkerMsg,
            }
        })
        .unwrap();

        let output = blox_event_inner(&input).unwrap();
        let output_str = output.to_string();

        // Verify event name
        assert!(output_str.contains("enum WorkerEvent"));
        assert!(output_str.contains("< R : BloxRuntime >"));

        // Verify both mailboxes
        assert!(output_str.contains("Peer"));
        assert!(output_str.contains("Msg ("));

        // Verify tag constants
        assert!(output_str.contains("PEER_TAG"));
        assert!(output_str.contains("MSG_TAG"));
    }
}
