use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DeriveInput, Error};

/// Convert a PascalCase identifier to UPPER_SNAKE_CASE.
/// E.g. "Lifecycle" -> "LIFECYCLE", "GoB" -> "GO_B", "SelfLoop" -> "SELF_LOOP".
pub(crate) fn to_upper_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            // Insert underscore before uppercase that follows lowercase or another uppercase
            // that is followed by lowercase (e.g. "GoB" -> "GO_B", not "G_O_B")
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

pub(crate) fn derive_event_tag_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let enum_name = &input.ident;

    let variants = match &input.data {
        syn::Data::Enum(e) => &e.variants,
        _ => {
            return Err(Error::new_spanned(
                input,
                "#[derive(EventTag)] only works on enums",
            ))
        }
    };

    let count = variants.len();
    if count > 254 {
        return Err(Error::new_spanned(
            input,
            "#[derive(EventTag)] supports at most 254 variants (255 is reserved for the wildcard sentinel)",
        ));
    }

    // Generate the match arms for event_tag()
    let tag_match_arms: Vec<TokenStream2> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            let tag = i as u8;
            // Handle unit, tuple, and struct variants generically
            let pattern = match &variant.fields {
                syn::Fields::Unit => quote! { Self::#variant_name },
                syn::Fields::Unnamed(_) => quote! { Self::#variant_name(..) },
                syn::Fields::Named(_) => quote! { Self::#variant_name { .. } },
            };
            quote! { #pattern => #tag }
        })
        .collect();

    // Generate UPPER_SNAKE_CASE tag constants
    let tag_constants: Vec<TokenStream2> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            let upper_snake = to_upper_snake_case(&variant_name.to_string());
            let const_name = format_ident!("{}_TAG", upper_snake);
            let tag = i as u8;
            quote! {
                pub const #const_name: u8 = #tag;
            }
        })
        .collect();

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::bloxide_core::event_tag::EventTag for #enum_name #ty_generics #where_clause {
            #[inline]
            fn event_tag(&self) -> u8 {
                match self {
                    #(#tag_match_arms,)*
                }
            }
        }

        impl #impl_generics #enum_name #ty_generics #where_clause {
            #(#tag_constants)*
        }
    })
}

#[cfg(test)]
mod tests {
    use super::to_upper_snake_case;

    #[test]
    fn case_conversion() {
        assert_eq!(to_upper_snake_case("Lifecycle"), "LIFECYCLE");
        assert_eq!(to_upper_snake_case("Msg"), "MSG");
        assert_eq!(to_upper_snake_case("GoB"), "GO_B");
        assert_eq!(to_upper_snake_case("SelfLoop"), "SELF_LOOP");
        assert_eq!(to_upper_snake_case("UnhandledDeep"), "UNHANDLED_DEEP");
        assert_eq!(to_upper_snake_case("Start"), "START");
        assert_eq!(to_upper_snake_case("Reset"), "RESET");
    }
}
