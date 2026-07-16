// Copyright 2025 Bloxide, all rights reserved.
//! Declarative macro for generating message types.
//!
//! The `blox_messages!` macro generates:
//! - A named struct for each message variant payload
//! - The message enum that wraps each payload struct
//! - `Debug` and `Clone` derives for all generated types
//! - `Copy` derive only when explicitly requested
//!
//! # Copy opt-in
//!
//! By default the macro derives `Debug, Clone` only. To also derive `Copy`,
//! prefix the enum with `copy,`:
//!
//! ```ignore
//! blox_messages!(copy, pub enum PingPongMsg { ... })
//! ```
//!
//! # Example
//!
//! ```ignore
//! blox_messages! {
//!     pub enum PingPongMsg {
//!         Ping { round: u32 },
//!         Pong { round: u32 },
//!         Resume {},
//!     }
//! }
//! ```
//!
//! Generates:
//! - `pub struct Ping { pub round: u32 }`
//! - `pub struct Pong { pub round: u32 }`
//! - `pub struct Resume {}`
//! - `pub enum PingPongMsg { Ping(Ping), Pong(Pong), Resume(Resume) }`

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// Parsed input for the blox_messages! macro.
pub(crate) struct BloxMessagesInput {
    copy: bool,
    vis: syn::Visibility,
    enum_ident: syn::Ident,
    variants: Vec<MessageVariant>,
}

/// A single message variant specification.
struct MessageVariant {
    ident: syn::Ident,
    fields: Vec<(syn::Ident, syn::Type)>,
}

impl syn::parse::Parse for BloxMessagesInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Optional leading `copy,` to opt-in to Copy derivation.
        let mut copy = false;
        let lookahead = input.lookahead1();
        if lookahead.peek(syn::Ident)
            && input.peek2(syn::Token![,])
            && input
                .cursor()
                .ident()
                .map(|(ident, _)| ident == "copy")
                .unwrap_or(false)
        {
            let _copy_kw: syn::Ident = input.parse()?;
            let _comma: syn::Token![,] = input.parse()?;
            copy = true;
        }

        let vis = input.parse()?;
        let _enum_token: syn::Token![enum] = input.parse()?;
        let enum_ident: syn::Ident = input.parse()?;

        let content;
        syn::braced!(content in input);

        let mut variants = Vec::new();
        while !content.is_empty() {
            let variant_ident: syn::Ident = content.parse()?;
            let fields_content;
            syn::braced!(fields_content in content);

            let mut fields = Vec::new();
            if !fields_content.is_empty() {
                loop {
                    let field_ident: syn::Ident = fields_content.parse()?;
                    let _colon: syn::Token![:] = fields_content.parse()?;
                    let ty: syn::Type = fields_content.parse()?;
                    fields.push((field_ident, ty));

                    if fields_content.is_empty() {
                        break;
                    }
                    let _comma: syn::Token![,] = fields_content.parse()?;
                    if fields_content.is_empty() {
                        break;
                    }
                }
            }

            variants.push(MessageVariant {
                ident: variant_ident,
                fields,
            });

            if !content.is_empty() {
                let _comma: syn::Token![,] = content.parse()?;
            }
        }

        Ok(BloxMessagesInput {
            copy,
            vis,
            enum_ident,
            variants,
        })
    }
}

pub(crate) fn blox_messages_inner(input: &BloxMessagesInput) -> syn::Result<TokenStream2> {
    let vis = &input.vis;
    let enum_ident = &input.enum_ident;
    let copy = input.copy;

    // Build the derive attribute: always Debug + Clone; add Copy only when opted in.
    let derives = if copy {
        quote! { #[derive(Debug, Clone, Copy)] }
    } else {
        quote! { #[derive(Debug, Clone)] }
    };

    // Generate struct definitions and enum variants
    let mut struct_defs = Vec::new();
    let mut enum_variants = Vec::new();
    let mut match_arms = Vec::new();

    for variant in &input.variants {
        let variant_ident = &variant.ident;
        // Use variant name as struct name (matches existing bloxide convention)
        let struct_ident = variant_ident.clone();

        if variant.fields.is_empty() {
            // Unit struct for empty variants
            struct_defs.push(quote! {
                #derives
                #vis struct #struct_ident;
            });
        } else {
            // Struct with named fields
            let field_idents: Vec<_> = variant.fields.iter().map(|(id, _)| id).collect();
            let field_types: Vec<_> = variant.fields.iter().map(|(_, ty)| ty).collect();

            struct_defs.push(quote! {
                #derives
                #vis struct #struct_ident {
                    #(pub #field_idents: #field_types),*
                }
            });
        }

        // Generate enum variant
        enum_variants.push(quote! {
            #variant_ident(#struct_ident)
        });

        // Generate match arm for message_name() method
        let variant_name_str = variant_ident.to_string();
        match_arms.push(quote! {
            #enum_ident::#variant_ident(..) => #variant_name_str
        });
    }

    // Generate the enum with the same derives
    let enum_def = quote! {
        #derives
        #vis enum #enum_ident {
            #(#enum_variants),*
        }
    };

    // Generate impl with message_name() method for debugging
    let impl_block = quote! {
        impl #enum_ident {
            /// Returns the variant name as a string.
            pub fn message_name(&self) -> &'static str {
                match self {
                    #(#match_arms,)*
                }
            }
        }
    };

    Ok(quote! {
        #(#struct_defs)*
        #enum_def
        #impl_block
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_blox_messages_simple() {
        let input: BloxMessagesInput = syn::parse2(quote! {
            pub enum PingPongMsg {
                Ping { round: u32 },
                Pong { round: u32 },
                Resume {},
            }
        })
        .unwrap();

        let output = blox_messages_inner(&input).unwrap();
        let output_str = output.to_string();

        // Verify structs are generated (no Payload suffix, matches convention)
        assert!(output_str.contains("struct Ping"));
        assert!(output_str.contains("struct Pong"));
        assert!(output_str.contains("struct Resume"));

        // Verify enum is generated
        assert!(output_str.contains("enum PingPongMsg"));

        // Verify base derives are present (Debug, Clone) but NOT Copy (opt-in)
        assert!(output_str.contains("Debug"));
        assert!(output_str.contains("Clone"));
        assert!(!output_str.contains("Copy"));
    }

    #[test]
    fn test_blox_messages_copy_opt_in() {
        let input: BloxMessagesInput = syn::parse2(quote! {
            copy, pub enum PingPongMsg {
                Ping { round: u32 },
                Pong { round: u32 },
                Resume {},
            }
        })
        .unwrap();

        let output = blox_messages_inner(&input).unwrap();
        let output_str = output.to_string();

        // Copy should be derived when opted in
        assert!(output_str.contains("Debug"));
        assert!(output_str.contains("Clone"));
        assert!(output_str.contains("Copy"));
    }

    #[test]
    fn test_blox_messages_with_multiple_fields() {
        let input: BloxMessagesInput = syn::parse2(quote! {
            pub enum WorkerMsg {
                DoWork { task_id: u64, payload: Vec<u8> },
                WorkDone { task_id: u64, result: Vec<u8> },
            }
        })
        .unwrap();

        let output = blox_messages_inner(&input).unwrap();
        let output_str = output.to_string();

        assert!(output_str.contains("pub task_id"));
        assert!(output_str.contains("payload"));
        assert!(output_str.contains("result"));

        // Non-Copy message types should compile fine — Copy must not be derived
        assert!(!output_str.contains("Copy"));
    }
}
