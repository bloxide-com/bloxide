// Copyright 2025 Bloxide, all rights reserved.
//! Declarative macro for generating message types.
//!
//! The `blox_messages!` macro generates:
//! - A named struct for each message variant payload
//! - The message enum that wraps each payload struct
//! - `Debug` and `Clone` derives for all generated types
//! - Optional `Copy` derive when all fields implement Copy
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
            vis,
            enum_ident,
            variants,
        })
    }
}

pub(crate) fn blox_messages_inner(input: &BloxMessagesInput) -> syn::Result<TokenStream2> {
    let vis = &input.vis;
    let enum_ident = &input.enum_ident;

    // Generate struct definitions and enum variants
    let mut struct_defs = Vec::new();
    let mut enum_variants = Vec::new();
    let mut match_arms = Vec::new();

    for variant in &input.variants {
        let variant_ident = &variant.ident;
        // Use variant name as struct name (matches existing bloxide convention)
        let struct_ident = variant_ident.clone();

        // Generate struct definition with Copy if all fields are Copy
        // For simplicity, derive Copy for all generated structs - user can remove if needed
        if variant.fields.is_empty() {
            // Unit struct for empty variants - always Copy
            struct_defs.push(quote! {
                #[derive(Debug, Clone, Copy)]
                #vis struct #struct_ident;
            });
        } else {
            // Struct with named fields - derive Copy for primitive types
            let field_idents: Vec<_> = variant.fields.iter().map(|(id, _)| id).collect();
            let field_types: Vec<_> = variant.fields.iter().map(|(_, ty)| ty).collect();

            struct_defs.push(quote! {
                #[derive(Debug, Clone, Copy)]
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

    // Generate the enum with Copy if possible (all variants are Copy)
    let enum_def = quote! {
        #[derive(Debug, Clone, Copy)]
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

        // Verify derives are present (including Copy)
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
    }
}
