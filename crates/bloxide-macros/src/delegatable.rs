/// `#[delegatable]` — attribute macro that generates a companion `macro_rules!`
/// delegation macro alongside a trait definition.
///
/// When applied to a trait, it:
/// 1. Emits the trait definition **unchanged**.
/// 2. Generates a `#[macro_export] macro_rules! __delegate_TraitName` macro that
///    accepts struct/field/generics information and produces a forwarding impl.
///
/// # Limitations (Phase A)
///
/// - Trait must not have type parameters (associated types are fine).
/// - Method arguments must use simple ident patterns.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, FnArg, ItemTrait, Pat, Result, TraitItem};

pub fn delegatable_inner(item: TokenStream) -> Result<TokenStream> {
    let trait_def: ItemTrait = syn::parse2(item)?;
    let trait_name = &trait_def.ident;
    let macro_name = format_ident!("__delegate_{}", trait_name);

    if !trait_def.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &trait_def.generics,
            "#[delegatable] does not yet support generic trait parameters",
        ));
    }

    let mut assoc_type_items = Vec::new();
    let mut method_items = Vec::new();

    for item in &trait_def.items {
        match item {
            TraitItem::Type(assoc) => {
                let type_name = &assoc.ident;
                assoc_type_items.push(quote! {
                    type #type_name = <$field_type as #trait_name>::#type_name;
                });
            }
            TraitItem::Fn(method) => {
                let sig = &method.sig;
                let method_name = &sig.ident;

                let mut arg_names: Vec<&syn::Ident> = Vec::new();
                for input in &sig.inputs {
                    if let FnArg::Typed(pat_type) = input {
                        arg_names.push(extract_arg_ident(&pat_type.pat)?);
                    }
                }

                method_items.push(quote! {
                    #sig {
                        self.$field.#method_name(#(#arg_names),*)
                    }
                });
            }
            _ => {}
        }
    }

    let output = quote! {
        #trait_def

        #[macro_export]
        macro_rules! #macro_name {
            (
                struct_name: $struct_name:ident,
                field: $field:ident,
                field_type: $field_type:ty,
                impl_generics: { $($impl_generics:tt)* },
                ty_generics: { $($ty_generics:tt)* },
                where_clause: { $($where_clause:tt)* }
            ) => {
                impl $($impl_generics)* #trait_name for $struct_name $($ty_generics)*
                where
                    $field_type: #trait_name,
                    $($where_clause)*
                {
                    #(#assoc_type_items)*
                    #(#method_items)*
                }
            };
        }
    };

    Ok(output)
}

fn extract_arg_ident(pat: &Pat) -> Result<&syn::Ident> {
    match pat {
        Pat::Ident(pat_ident) => Ok(&pat_ident.ident),
        _ => Err(Error::new_spanned(
            pat,
            "#[delegatable]: method arguments must use simple ident patterns",
        )),
    }
}
