// Copyright 2025 Bloxide, all rights reserved
//! Code generation for convention-based context derivation.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::PathArguments;

use super::analyze::{ContextAnalysis, FieldRole};

/// Generate all impls and constructor from analysis.
pub fn generate(analysis: &ContextAnalysis) -> syn::Result<TokenStream> {
    let mut output = TokenStream::new();

    let struct_name = &analysis.struct_name;
    let generics = &analysis.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Generate deprecation warnings if old annotations were used.
    if analysis.has_deprecated_annotations {
        output.extend(generate_deprecation_warning());
    }

    // Generate HasSelfId impl.
    for field in &analysis.fields {
        if matches!(field.role, FieldRole::SelfId) {
            output.extend(generate_has_self_id_impl(
                struct_name,
                &impl_generics,
                &ty_generics,
                where_clause,
                &field.name,
            ));
        }
    }

    // Generate accessor trait impls.
    for field in &analysis.fields {
        if let FieldRole::Accessor(trait_tokens) = &field.role {
            output.extend(generate_accessor_impl(
                struct_name,
                &impl_generics,
                &ty_generics,
                where_clause,
                &field.name,
                &field.ty,
                trait_tokens,
            ));
        }
    }

    // Generate constructor.
    output.extend(generate_constructor(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &analysis.fields,
    ));

    // Generate delegate macro invocations for #[delegates] fields.
    let where_predicates = generics
        .where_clause
        .as_ref()
        .map(|wc| {
            let preds = &wc.predicates;
            quote! { #preds }
        })
        .unwrap_or_default();

    for field in &analysis.fields {
        if let FieldRole::Delegates(traits) = &field.role {
            let fname = &field.name;
            let fty = &field.ty;
            for trait_path in traits {
                let last_seg = trait_path.segments.last().ok_or_else(|| {
                    syn::Error::new_spanned(trait_path, "empty trait path in delegates")
                })?;
                let macro_name = format_ident!("__delegate_{}", last_seg.ident);

                // Extract trait args from the path segment (e.g., HasPeers<WorkerMsg, R>)
                let trait_args = match &last_seg.arguments {
                    PathArguments::AngleBracketed(args) => {
                        // Extract the arguments as a token stream
                        let args_tokens = &args.args;
                        quote! { #args_tokens }
                    }
                    PathArguments::None => {
                        // No generic arguments
                        quote! {}
                    }
                    PathArguments::Parenthesized(_) => {
                        return Err(syn::Error::new_spanned(
                            trait_path,
                            "delegates: parenthesized arguments not supported",
                        ));
                    }
                };

                output.extend(quote! {
                    #macro_name!(
                        struct_name: #struct_name,
                        field: #fname,
                        field_type: #fty,
                        impl_generics: { #impl_generics },
                        ty_generics: { #ty_generics },
                        where_clause: { #where_predicates },
                        trait_args: { #trait_args }
                    );
                });
            }
        }
    }

    Ok(output)
}

/// Generate a compile-time deprecation warning for old annotations.
fn generate_deprecation_warning() -> TokenStream {
    quote! {
        #[deprecated(
            note = "BloxCtx: using deprecated annotations. Use convention-based fields instead: \
                    `self_id: ActorId` is auto-detected, `_ref` fields auto-generate accessor traits. \
                    Only `#[delegates(...)]` is required for behavior fields to document intent."
        )]
        const _: () = ();
    }
}

/// Generate `impl HasSelfId for Struct`.
fn generate_has_self_id_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_name: &syn::Ident,
) -> TokenStream {
    quote! {
        impl #impl_generics ::bloxide_core::accessor::HasSelfId
            for #struct_name #ty_generics #where_clause
        {
            fn self_id(&self) -> ::bloxide_core::messaging::ActorId {
                self.#field_name
            }
        }
    }
}

/// Generate accessor trait impl for a single field.
fn generate_accessor_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_name: &syn::Ident,
    field_type: &syn::Type,
    trait_tokens: &TokenStream,
) -> TokenStream {
    quote! {
        impl #impl_generics #trait_tokens
            for #struct_name #ty_generics #where_clause
        {
            fn #field_name(&self) -> &#field_type {
                &self.#field_name
            }
        }
    }
}

/// Generate the `new()` constructor.
fn generate_constructor(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    fields: &[super::analyze::FieldAnalysis],
) -> TokenStream {
    // Parameters: SelfId, Ctor, Accessor, Delegates all become parameters.
    // State fields are zero-initialized via Default::default().
    let params: Vec<_> = fields
        .iter()
        .filter(|f| !matches!(f.role, FieldRole::State))
        .map(|f| {
            let n = &f.name;
            let t = &f.ty;
            quote! { #n: #t }
        })
        .collect();

    let field_inits: Vec<_> = fields
        .iter()
        .map(|f| {
            let n = &f.name;
            if matches!(f.role, FieldRole::State) {
                quote! { #n: ::core::default::Default::default() }
            } else {
                quote! { #n }
            }
        })
        .collect();

    quote! {
        impl #impl_generics #struct_name #ty_generics #where_clause {
            pub fn new(#(#params),*) -> Self {
                Self { #(#field_inits),* }
            }
        }
    }
}
