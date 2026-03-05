use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Fields, ItemEnum};

use crate::event_tag::to_upper_snake_case;

pub(crate) fn blox_event_inner(input: &ItemEnum) -> syn::Result<TokenStream2> {
    let enum_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let variant_count = input.variants.len();
    if variant_count > 254 {
        return Err(syn::Error::new_spanned(
            input,
            "#[blox_event] supports at most 254 variants (255 is reserved for the wildcard sentinel)",
        ));
    }

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
    let event_tag_arms: Vec<TokenStream2> = input
        .variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            let tag = i as u8;
            quote! { Self::#variant_name(..) => #tag }
        })
        .collect();

    // Generate UPPER_SNAKE_CASE tag constants
    let tag_constants: Vec<TokenStream2> = input
        .variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            let upper_snake = to_upper_snake_case(&variant_name.to_string());
            let const_name = format_ident!("{}_TAG", upper_snake);
            let tag = i as u8;
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

        impl #impl_generics #enum_name #ty_generics #where_clause {
            #(#tag_constants)*
            #(#accessor_methods)*
        }
    })
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
