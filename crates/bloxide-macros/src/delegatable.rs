// Copyright 2025 Bloxide, all rights reserved
/// `#[delegatable]` — attribute macro that generates a companion `macro_rules!`
/// delegation macro alongside a trait definition.
///
/// When applied to a trait, it:
/// 1. Emits the trait definition **unchanged**.
/// 2. Generates a `#[macro_export] macro_rules! __delegate_TraitName` macro that
///    accepts struct/field/generics information and produces a forwarding impl.
///
/// # Generic Trait Support
///
/// For generic traits like `HasPeers<M, R>`, the generated macro accepts a `trait_args`
/// parameter to specify the concrete types:
///
/// ```ignore
/// __delegate_HasPeers!(
///     struct_name: MyCtx,
///     field: behavior,
///     field_type: B,
///     impl_generics: { impl<R, B> },
///     ty_generics: { <R, B> },
///     where_clause: { ... },
///     trait_args: { WorkerMsg, R }
/// );
/// ```
///
/// For non-generic traits, `trait_args` can be empty.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemTrait, Pat, Result, TraitItem};

pub fn delegatable_inner(item: TokenStream) -> Result<TokenStream> {
    let trait_def: ItemTrait = syn::parse2(item)?;
    let trait_name = &trait_def.ident;
    let macro_name = format_ident!("__delegate_{}", trait_name);

    // Check if trait has generic parameters
    let has_generics = !trait_def.generics.params.is_empty();

    let mut assoc_type_items = Vec::new();
    let mut method_items = Vec::new();

    for item in &trait_def.items {
        match item {
            TraitItem::Type(assoc) => {
                let type_name = &assoc.ident;
                if has_generics {
                    // For generic traits, use the trait with trait_args from macro
                    assoc_type_items.push(quote! {
                        type #type_name = <$field_type as #trait_name<$($trait_args)*>>::#type_name;
                    });
                } else {
                    // For non-generic traits, use the trait without angle brackets
                    assoc_type_items.push(quote! {
                        type #type_name = <$field_type as #trait_name>::#type_name;
                    });
                }
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

    // Generate the macro
    let output = if has_generics {
        // Generic trait: macro requires trait_args parameter
        quote! {
            #trait_def

            #[macro_export]
            macro_rules! #macro_name {
                (
                    struct_name: $struct_name:ident,
                    field: $field:ident,
                    field_type: $field_type:ty,
                    impl_generics: { $($impl_generics:tt)* },
                    ty_generics: { $($ty_generics:tt)* },
                    where_clause: { $($where_clause:tt)* },
                    trait_args: { $($trait_args:tt)* }
                ) => {
                    impl $($impl_generics)* #trait_name<$($trait_args)*> for $struct_name $($ty_generics)*
                    where
                        $field_type: #trait_name<$($trait_args)*>,
                        $($where_clause)*
                    {
                        #(#assoc_type_items)*
                        #(#method_items)*
                    }
                };
            }
        }
    } else {
        // Non-generic trait: trait_args is accepted but ignored
        quote! {
            #trait_def

            #[macro_export]
            macro_rules! #macro_name {
                (
                    struct_name: $struct_name:ident,
                    field: $field:ident,
                    field_type: $field_type:ty,
                    impl_generics: { $($impl_generics:tt)* },
                    ty_generics: { $($ty_generics:tt)* },
                    where_clause: { $($where_clause:tt)* },
                    trait_args: { $($trait_args:tt)* }
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
                // Backward compatibility: allow omitting trait_args
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
        }
    };

    Ok(output)
}

fn extract_arg_ident(pat: &Pat) -> Result<&syn::Ident> {
    match pat {
        Pat::Ident(pat_ident) => Ok(&pat_ident.ident),
        _ => Err(syn::Error::new_spanned(
            pat,
            "#[delegatable]: method arguments must use simple ident patterns",
        )),
    }
}
