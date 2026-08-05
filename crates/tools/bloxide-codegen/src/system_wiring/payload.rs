// Copyright 2025 Bloxide, all rights reserved
//! Conversion of TOML bootstrap-payload values into Rust literal tokens.

use quote::quote;

pub(super) fn toml_value_to_tokens(
    value: &toml::Value,
) -> anyhow::Result<proc_macro2::TokenStream> {
    use proc_macro2::Span;
    match value {
        toml::Value::Integer(i) => {
            let lit = proc_macro2::Literal::i64_unsuffixed(*i);
            Ok(quote! { #lit })
        }
        toml::Value::String(s) => {
            let lit = syn::LitStr::new(s, Span::call_site());
            Ok(quote! { #lit })
        }
        toml::Value::Float(f) => {
            let lit = proc_macro2::Literal::f64_unsuffixed(*f);
            Ok(quote! { #lit })
        }
        toml::Value::Boolean(true) => Ok(quote! { true }),
        toml::Value::Boolean(false) => Ok(quote! { false }),
        toml::Value::Array(arr) => {
            let elems: Vec<_> = arr
                .iter()
                .map(toml_value_to_tokens)
                .collect::<Result<_, _>>()?;
            Ok(quote! { [#(#elems),*] })
        }
        toml::Value::Datetime(dt) => {
            let lit = syn::LitStr::new(&dt.to_string(), Span::call_site());
            Ok(quote! { #lit })
        }
        toml::Value::Table(_) => {
            anyhow::bail!("nested tables are not supported as bootstrap payload values")
        }
    }
}
