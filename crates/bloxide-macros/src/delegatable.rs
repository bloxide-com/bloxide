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
use proc_macro2::{Group, Ident, Punct, Spacing, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{FnArg, GenericParam, ItemTrait, Pat, Result, TraitItem};

pub fn delegatable_inner(item: TokenStream) -> Result<TokenStream> {
    let trait_def: ItemTrait = syn::parse2(item)?;
    let trait_name = &trait_def.ident;
    let macro_name = format_ident!("__delegate_{}", trait_name);

    // Check if trait has generic parameters
    let has_generics = !trait_def.generics.params.is_empty();

    // Extract generic type param names (e.g., M, R from HasPeers<M, R>)
    let generic_param_names: Vec<Ident> = trait_def
        .generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(tp) => Some(tp.ident.clone()),
            _ => None,
        })
        .collect();

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

                if has_generics {
                    // For generic traits: substitute generic param names in the
                    // method signature with $N:tt macro variables. Each generic
                    // param gets its own numbered capture.
                    //
                    // The macro arms will capture trait_args as $($argN:tt)* and
                    // the method signatures will use $argN instead of the param name.
                    let sig_tokens = quote! { #sig };
                    let substituted = substitute_generic_params(&sig_tokens, &generic_param_names);
                    method_items.push(quote! {
                        #substituted {
                            self.$field.#method_name(#(#arg_names),*)
                        }
                    });
                } else {
                    // Non-generic: signatures are safe to use verbatim
                    method_items.push(quote! {
                        #sig {
                            self.$field.#method_name(#(#arg_names),*)
                        }
                    });
                }
            }
            _ => {}
        }
    }

    // Generate the macro
    let output = if has_generics {
        // Generic trait: macro requires trait_args parameter.
        // We capture trait_args as individual $N:tt variables so that
        // $N can be used as a type in method signatures.
        //
        // For a trait with K type params, we generate K+1 arms:
        // - The first arm captures exactly K args (one per param)
        // - Additional arms handle trailing commas, etc.
        //
        // Actually, macro_rules! can't count, so we use a fixed pattern.
        // For now, we support up to 4 generic params (enough for HasPeers<M, R>).
        let n = generic_param_names.len();
        let macro_def = match n {
            1 => quote! {
                #[macro_export]
                macro_rules! #macro_name {
                    (
                        struct_name: $struct_name:ident,
                        field: $field:ident,
                        field_type: $field_type:ty,
                        impl_generics: { $($impl_generics:tt)* },
                        ty_generics: { $($ty_generics:tt)* },
                        where_clause: { $($where_clause:tt)* },
                        trait_args: { $a0:tt }
                    ) => {
                        impl $($impl_generics)* #trait_name<$a0> for $struct_name $($ty_generics)*
                        where
                            $field_type: #trait_name<$a0>,
                            $($where_clause)*
                        {
                            #(#assoc_type_items)*
                            #(#method_items)*
                        }
                    };
                }
            },
            2 => quote! {
                #[macro_export]
                macro_rules! #macro_name {
                    (
                        struct_name: $struct_name:ident,
                        field: $field:ident,
                        field_type: $field_type:ty,
                        impl_generics: { $($impl_generics:tt)* },
                        ty_generics: { $($ty_generics:tt)* },
                        where_clause: { $($where_clause:tt)* },
                        trait_args: { $a0:tt, $a1:tt }
                    ) => {
                        impl $($impl_generics)* #trait_name<$a0, $a1> for $struct_name $($ty_generics)*
                        where
                            $field_type: #trait_name<$a0, $a1>,
                            $($where_clause)*
                        {
                            #(#assoc_type_items)*
                            #(#method_items)*
                        }
                    };
                    // Also accept without comma
                    (
                        struct_name: $struct_name:ident,
                        field: $field:ident,
                        field_type: $field_type:ty,
                        impl_generics: { $($impl_generics:tt)* },
                        ty_generics: { $($ty_generics:tt)* },
                        where_clause: { $($where_clause:tt)* },
                        trait_args: { $a0:tt $a1:tt }
                    ) => {
                        impl $($impl_generics)* #trait_name<$a0, $a1> for $struct_name $($ty_generics)*
                        where
                            $field_type: #trait_name<$a0, $a1>,
                            $($where_clause)*
                        {
                            #(#assoc_type_items)*
                            #(#method_items)*
                        }
                    };
                }
            },
            3 => quote! {
                #[macro_export]
                macro_rules! #macro_name {
                    (
                        struct_name: $struct_name:ident,
                        field: $field:ident,
                        field_type: $field_type:ty,
                        impl_generics: { $($impl_generics:tt)* },
                        ty_generics: { $($ty_generics:tt)* },
                        where_clause: { $($where_clause:tt)* },
                        trait_args: { $a0:tt, $a1:tt, $a2:tt }
                    ) => {
                        impl $($impl_generics)* #trait_name<$a0, $a1, $a2> for $struct_name $($ty_generics)*
                        where
                            $field_type: #trait_name<$a0, $a1, $a2>,
                            $($where_clause)*
                        {
                            #(#assoc_type_items)*
                            #(#method_items)*
                        }
                    };
                }
            },
            4 => quote! {
                #[macro_export]
                macro_rules! #macro_name {
                    (
                        struct_name: $struct_name:ident,
                        field: $field:ident,
                        field_type: $field_type:ty,
                        impl_generics: { $($impl_generics:tt)* },
                        ty_generics: { $($ty_generics:tt)* },
                        where_clause: { $($where_clause:tt)* },
                        trait_args: { $a0:tt, $a1:tt, $a2:tt, $a3:tt }
                    ) => {
                        impl $($impl_generics)* #trait_name<$a0, $a1, $a2, $a3> for $struct_name $($ty_generics)*
                        where
                            $field_type: #trait_name<$a0, $a1, $a2, $a3>,
                            $($where_clause)*
                        {
                            #(#assoc_type_items)*
                            #(#method_items)*
                        }
                    };
                }
            },
            _ => {
                return Err(syn::Error::new_spanned(
                    &trait_def,
                    "#[delegatable]: traits with more than 4 generic type params are not supported",
                ));
            }
        };

        quote! {
            #trait_def

            #macro_def
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

/// Replace occurrences of generic param names in a token stream with
/// `$N` macro_rules! variables (where N is the index of the param).
///
/// For example, if `param_names = ["M", "R"]`, then every `M` token becomes
/// `$a0` and every `R` token becomes `$a1`. This allows the generated macro to
/// substitute the correct concrete types at expansion time.
fn substitute_generic_params(tokens: &TokenStream, param_names: &[Ident]) -> TokenStream {
    let mut result = TokenStream::new();

    for token in tokens.clone() {
        match &token {
            TokenTree::Ident(ident) => {
                // Check if this ident matches a generic param name
                if let Some(idx) = param_names.iter().position(|p| p == ident) {
                    // Emit $aN (macro_rules! variable reference)
                    let dollar = Punct::new('$', Spacing::Alone);
                    let var = format_ident!("a{}", idx);
                    result.extend(TokenStream::from(TokenTree::Punct(dollar)));
                    result.extend(TokenStream::from(TokenTree::Ident(var)));
                } else {
                    result.extend(TokenStream::from(token));
                }
            }
            TokenTree::Group(group) => {
                // Recursively substitute inside groups (e.g., <M, R>, [ActorRef<M, R>])
                let new_inner = substitute_generic_params(&group.stream(), param_names);
                let new_group = Group::new(group.delimiter(), new_inner);
                result.extend(TokenStream::from(TokenTree::Group(new_group)));
            }
            _ => {
                result.extend(TokenStream::from(token));
            }
        }
    }

    result
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
