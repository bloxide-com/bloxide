// Copyright 2025 Bloxide, all rights reserved
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Fields, ItemEnum};

use crate::event_tag::to_upper_snake_case;
use crate::event_tag::MAX_EVENT_VARIANTS;

/// Input for the function-like `blox_event!` macro.
#[allow(dead_code)]
pub struct BloxEventInput {
    vis: syn::Visibility,
    event_ident: syn::Ident,
    msg_type: syn::Type,
    generics: Option<syn::Generics>,
}

impl syn::parse::Parse for BloxEventInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Parse: pub enum EventName<R: BloxRuntime> uses MsgType;
        let vis = input.parse()?;
        let _enum_token: syn::Token![enum] = input.parse()?;
        let event_ident: syn::Ident = input.parse()?;

        // Parse optional generics
        let generics: Option<syn::Generics> = if input.peek(syn::Token![<]) {
            Some(input.parse()?)
        } else {
            None
        };

        let uses_token: Option<syn::Ident> = input.parse().ok();
        if let Some(uses_tok) = uses_token {
            if uses_tok != "uses" {
                return Err(syn::Error::new_spanned(uses_tok, "expected `uses` keyword"));
            }
        } else {
            return Err(syn::Error::new(
                input.span(),
                "expected `uses` keyword after event name",
            ));
        }

        let msg_type: syn::Type = input.parse()?;
        let _semi: syn::Token![;] = input.parse()?;

        Ok(BloxEventInput {
            vis,
            event_ident,
            msg_type,
            generics,
        })
    }
}

/// Generate a blox event from the function-like macro syntax.
#[allow(dead_code)]
pub(crate) fn blox_event_simple_inner(input: &BloxEventInput) -> syn::Result<TokenStream2> {
    let vis = &input.vis;
    let event_ident = &input.event_ident;
    let msg_type = &input.msg_type;

    // Handle generics - use default Generics when None
    let dummy_generics = syn::Generics::default();
    let generics = input.generics.as_ref().unwrap_or(&dummy_generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate the event enum
    let enum_def = quote! {
        #[derive(Debug)]
        #vis enum #event_ident #ty_generics {
            Lifecycle(::bloxide_core::lifecycle::LifecycleCommand),
            Msg(::bloxide_core::messaging::Envelope<#msg_type>),
        }
    };

    // Generate From<Envelope<M>> impl
    let from_envelope = quote! {
        impl #impl_generics ::core::convert::From<::bloxide_core::messaging::Envelope<#msg_type>> for #event_ident #ty_generics #where_clause {
            fn from(env: ::bloxide_core::messaging::Envelope<#msg_type>) -> Self {
                #event_ident::Msg(env)
            }
        }
    };

    // Generate From<LifecycleCommand> impl
    let from_lifecycle = quote! {
        impl #impl_generics ::core::convert::From<::bloxide_core::lifecycle::LifecycleCommand> for #event_ident #ty_generics #where_clause {
            fn from(cmd: ::bloxide_core::lifecycle::LifecycleCommand) -> Self {
                #event_ident::Lifecycle(cmd)
            }
        }
    };

    // Generate EventTag impl
    let event_tag_impl = quote! {
        impl #impl_generics ::bloxide_core::event_tag::EventTag for #event_ident #ty_generics #where_clause {
            #[inline]
            fn event_tag(&self) -> u8 {
                match self {
                    Self::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG,
                    Self::Msg(..) => 0,
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

    // Generate helper methods
    let impl_block = quote! {
        impl #impl_generics #event_ident #ty_generics #where_clause {
            /// Tag for the Msg variant.
            pub const MSG_TAG: u8 = 0;

            /// Get the message envelope if this is a Msg variant.
            pub fn msg(&self) -> ::core::option::Option<&::bloxide_core::messaging::Envelope<#msg_type>> {
                match self {
                    Self::Msg(env) => ::core::option::Option::Some(env),
                    _ => ::core::option::Option::None,
                }
            }

            /// Get the message payload if this is a Msg variant.
            pub fn msg_payload(&self) -> ::core::option::Option<&#msg_type> {
                match self {
                    Self::Msg(env) => ::core::option::Option::Some(&env.1),
                    _ => ::core::option::Option::None,
                }
            }

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
        }
    };

    Ok(quote! {
        #enum_def
        #from_envelope
        #from_lifecycle
        #event_tag_impl
        #lifecycle_impl
        #impl_block
    })
}

pub(crate) fn blox_event_inner(input: &ItemEnum) -> syn::Result<TokenStream2> {
    let enum_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let variant_count = input.variants.len();
    if variant_count > MAX_EVENT_VARIANTS {
        return Err(syn::Error::new_spanned(
            input,
            "#[blox_event] supports at most 254 variants (255 is reserved for the wildcard sentinel)",
        ));
    }

    // Detect Lifecycle(LifecycleCommand) variant
    let lifecycle_variant = input.variants.iter().find(|variant| {
        if variant.ident != "Lifecycle" {
            return false;
        }
        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                is_lifecycle_command(&fields.unnamed[0].ty)
            }
            _ => false,
        }
    });

    // Generate From<Envelope<M>> impls
    let from_impls: Vec<TokenStream2> = input
        .variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let field_ty = &fields.unnamed[0].ty;
                    Ok(quote! {
                        impl #impl_generics ::core::convert::From<#field_ty> for #enum_name #ty_generics #where_clause {
                            fn from(env: #field_ty) -> Self {
                                #enum_name::#variant_name(env)
                            }
                        }
                    })
                }
                _ => Err(syn::Error::new_spanned(
                    variant,
                    "#[blox_event] variants must each have exactly one unnamed field \
                     (the Envelope<M> wrapper)",
                )),
            }
        })
        .collect::<syn::Result<_>>()?;

    // Generate EventTag impl: match self { Self::Variant(..) => index, ... }
    // Lifecycle variant gets LIFECYCLE_TAG, others get sequential tags starting from 0
    let mut next_tag: u8 = 0;
    let event_tag_arms: Vec<TokenStream2> = input
        .variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            // Check if this is the Lifecycle variant
            let tag = if lifecycle_variant.is_some()
                && variant.ident == "Lifecycle"
                && is_variant_lifecycle_command(variant)
            {
                quote! { ::bloxide_core::event_tag::LIFECYCLE_TAG }
            } else {
                let t = next_tag;
                next_tag += 1;
                quote! { #t }
            };
            quote! { Self::#variant_name(..) => #tag }
        })
        .collect();

    // Generate UPPER_SNAKE_CASE tag constants
    // Skip Lifecycle variant (LIFECYCLE_TAG is in bloxide-core)
    let mut next_const_tag: u8 = 0;
    let tag_constants: Vec<TokenStream2> = input
        .variants
        .iter()
        .filter(|variant| {
            // Skip Lifecycle(LifecycleCommand) - uses LIFECYCLE_TAG constant
            !(lifecycle_variant.is_some()
                && variant.ident == "Lifecycle"
                && is_variant_lifecycle_command(variant))
        })
        .map(|variant| {
            let variant_name = &variant.ident;
            let upper_snake = to_upper_snake_case(&variant_name.to_string());
            let const_name = format_ident!("{}_TAG", upper_snake);
            let tag = next_const_tag;
            next_const_tag += 1;
            quote! { pub const #const_name: u8 = #tag; }
        })
        .collect();

    // Generate payload accessor methods: variant_payload() -> Option<&InnerType>
    // Extract inner type from Envelope<T> if possible, otherwise use the field type.
    let accessor_methods: Vec<TokenStream2> = input
        .variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let snake_name = to_snake_case(&variant_name.to_string());
            let payload_fn_name = format_ident!("{}_payload", snake_name);
            let envelope_fn_name = format_ident!("{}_envelope", snake_name);

            match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let field_ty = &fields.unnamed[0].ty;
                    // Try to extract the inner type from Envelope<T>
                    if let Some(inner_ty) = extract_envelope_inner(field_ty) {
                        quote! {
                            pub fn #payload_fn_name(&self) -> ::core::option::Option<&#inner_ty> {
                                match self {
                                    Self::#variant_name(env) => ::core::option::Option::Some(&env.1),
                                    _ => ::core::option::Option::None,
                                }
                            }
                            pub fn #envelope_fn_name(&self) -> ::core::option::Option<&#field_ty> {
                                match self {
                                    Self::#variant_name(env) => ::core::option::Option::Some(env),
                                    _ => ::core::option::Option::None,
                                }
                            }
                        }
                    } else {
                        quote! {
                            pub fn #payload_fn_name(&self) -> ::core::option::Option<&#field_ty> {
                                match self {
                                    Self::#variant_name(inner) => ::core::option::Option::Some(inner),
                                    _ => ::core::option::Option::None,
                                }
                            }
                        }
                    }
                }
                _ => quote! {},
            }
        })
        .collect();

    // Generate LifecycleEvent impl if Lifecycle(LifecycleCommand) variant exists
    let lifecycle_impl = if lifecycle_variant.is_some() {
        quote! {
            impl #impl_generics ::bloxide_core::event_tag::LifecycleEvent for #enum_name #ty_generics #where_clause {
                fn as_lifecycle_command(&self) -> ::core::option::Option<::bloxide_core::lifecycle::LifecycleCommand> {
                    match self {
                        Self::Lifecycle(cmd) => ::core::option::Option::Some(*cmd),
                        _ => ::core::option::Option::None,
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #input

        #(#from_impls)*

        impl #impl_generics ::bloxide_core::event_tag::EventTag for #enum_name #ty_generics #where_clause {
            #[inline]
            fn event_tag(&self) -> u8 {
                match self {
                    #(#event_tag_arms,)*
                }
            }
        }

        #lifecycle_impl

        impl #impl_generics #enum_name #ty_generics #where_clause {
            #(#tag_constants)*
            #(#accessor_methods)*
        }
    })
}

/// Check if a type is `LifecycleCommand`.
fn is_lifecycle_command(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        if let Some(last) = segments.last() {
            return last.ident == "LifecycleCommand";
        }
    }
    false
}

/// Check if a variant is Lifecycle(LifecycleCommand).
fn is_variant_lifecycle_command(variant: &syn::Variant) -> bool {
    if variant.ident != "Lifecycle" {
        return false;
    }
    match &variant.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            is_lifecycle_command(&fields.unnamed[0].ty)
        }
        _ => false,
    }
}

/// Convert PascalCase to snake_case for method names.
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

/// Try to extract `T` from `Envelope<T>`.
fn extract_envelope_inner(ty: &syn::Type) -> Option<syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        let last = segments.last()?;
        if last.ident != "Envelope" {
            return None;
        }
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
            if args.args.len() == 1 {
                if let syn::GenericArgument::Type(inner) = &args.args[0] {
                    return Some(inner.clone());
                }
            }
        }
    }
    None
}
