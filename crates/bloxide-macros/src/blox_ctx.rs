// Copyright 2025 Bloxide, all rights reserved
/// `#[derive(BloxCtx)]` — generate accessor trait impls and constructor for blox context structs.
///
/// # Supported field annotations
///
/// - `#[self_id]` — generates `impl HasSelfId for Struct`
/// - `#[ctor]` — makes the field a constructor parameter without generating any trait impl
/// - `#[provides(TraitName<R>)]` — generates `impl TraitName<R> for Struct` with single accessor method
///   (convention: method name = field name, return type = `&FieldType`)
/// - `#[delegates(Trait1, Trait2, ...)]` — emits `__delegate_TraitName!` companion macro
///   invocations that generate forwarding impls (traits must be `#[delegatable]`)
///
/// # Constructor
///
/// Generates `fn new(...)` where annotated fields are constructor parameters and
/// unannotated fields are zero-initialized via `Default::default()`.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    punctuated::Punctuated, spanned::Spanned, DeriveInput, Error, Fields, Ident, Result, Type,
};

// Recognized field annotations for BloxCtx.
const ANNOTATION_SELF_ID: &str = "self_id";
const ANNOTATION_CTOR: &str = "ctor";
const ANNOTATION_PROVIDES: &str = "provides";
const ANNOTATION_DELEGATES: &str = "delegates";

const ALL_ANNOTATIONS: &[&str] = &[
    ANNOTATION_SELF_ID,
    ANNOTATION_CTOR,
    ANNOTATION_PROVIDES,
    ANNOTATION_DELEGATES,
];

/// Categorization of a field's BloxCtx annotation.
enum FieldAnnotation {
    SelfId,
    /// Constructor parameter only — no trait impl generated.
    Ctor,
    /// The TokenStream is the full trait path (possibly with generics), e.g. `HasPeerRef<R>`.
    Provides(TokenStream),
    /// Each path names a trait whose companion `__delegate_TraitName!` macro will be invoked.
    Delegates(Vec<syn::Path>),
    /// No BloxCtx annotation — will be zero-inited in constructor.
    None,
}

struct FieldInfo<'a> {
    name: &'a Ident,
    ty: &'a Type,
    annotation: FieldAnnotation,
}

pub fn derive_blox_ctx_inner(input: &DeriveInput) -> Result<TokenStream> {
    let struct_name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Only support named-field structs.
    let fields = match &input.data {
        syn::Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return Err(Error::new_spanned(
                    input,
                    "#[derive(BloxCtx)] requires named fields",
                ))
            }
        },
        _ => {
            return Err(Error::new_spanned(
                input,
                "#[derive(BloxCtx)] only supports structs",
            ))
        }
    };

    // Parse each field's annotation.
    let mut field_infos: Vec<FieldInfo<'_>> = Vec::new();

    for field in fields {
        let name = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "BloxCtx: expected named field"))?;
        let ty = &field.ty;
        let annotation = extract_annotation(field)?;
        field_infos.push(FieldInfo {
            name,
            ty,
            annotation,
        });
    }

    let mut output = TokenStream::new();

    // ── HasSelfId impl ─────────────────────────────────────────────────────────

    for fi in &field_infos {
        if matches!(fi.annotation, FieldAnnotation::SelfId) {
            let fname = fi.name;
            output.extend(quote! {
                impl #impl_generics ::bloxide_core::accessor::HasSelfId
                    for #struct_name #ty_generics #where_clause
                {
                    fn self_id(&self) -> ::bloxide_core::messaging::ActorId {
                        self.#fname
                    }
                }
            });
        }
    }

    // ── #[provides(Trait<R>)] impls ────────────────────────────────────────────
    // Convention: single accessor method whose name = field name, returning &FieldType.

    for fi in &field_infos {
        if let FieldAnnotation::Provides(trait_tokens) = &fi.annotation {
            let fname = fi.name;
            let fty = fi.ty;
            output.extend(quote! {
                impl #impl_generics #trait_tokens
                    for #struct_name #ty_generics #where_clause
                {
                    fn #fname(&self) -> &#fty {
                        &self.#fname
                    }
                }
            });
        }
    }

    // ── Constructor ────────────────────────────────────────────────────────────
    // Annotated fields (self_id, provides, delegates) become parameters.
    // Unannotated fields are initialized via Default::default().

    let params: Vec<_> = field_infos
        .iter()
        .filter(|fi| !matches!(fi.annotation, FieldAnnotation::None))
        .map(|fi| {
            let n = fi.name;
            let t = fi.ty;
            quote! { #n: #t }
        })
        .collect();

    let field_inits: Vec<_> = field_infos
        .iter()
        .map(|fi| {
            let n = fi.name;
            if matches!(fi.annotation, FieldAnnotation::None) {
                quote! { #n: ::core::default::Default::default() }
            } else {
                quote! { #n }
            }
        })
        .collect();

    output.extend(quote! {
        impl #impl_generics #struct_name #ty_generics #where_clause {
            pub fn new(#(#params),*) -> Self {
                Self { #(#field_inits),* }
            }
        }
    });

    // ── #[delegates] companion macro invocations ────────────────────────────

    let where_predicates = generics
        .where_clause
        .as_ref()
        .map(|wc| {
            let preds = &wc.predicates;
            quote! { #preds }
        })
        .unwrap_or_default();

    for fi in &field_infos {
        if let FieldAnnotation::Delegates(traits) = &fi.annotation {
            let fname = fi.name;
            let fty = fi.ty;
            for trait_path in traits {
                let last_seg = &trait_path
                    .segments
                    .last()
                    .ok_or_else(|| Error::new_spanned(trait_path, "empty trait path"))?
                    .ident;
                let macro_name = format_ident!("__delegate_{}", last_seg);

                output.extend(quote! {
                    #macro_name!(
                        struct_name: #struct_name,
                        field: #fname,
                        field_type: #fty,
                        impl_generics: { #impl_generics },
                        ty_generics: { #ty_generics },
                        where_clause: { #where_predicates }
                    );
                });
            }
        }
    }

    Ok(output)
}

/// Extract the BloxCtx annotation from a field, returning `FieldAnnotation::None`
/// if the field has no recognized annotation. Errors on malformed annotations.
fn extract_annotation(field: &syn::Field) -> Result<FieldAnnotation> {
    let mut result = FieldAnnotation::None;
    let mut found = false;

    for attr in &field.attrs {
        let path = attr.path();
        let is_known = ALL_ANNOTATIONS.iter().any(|a| path.is_ident(a));
        if !is_known {
            continue;
        }

        if found {
            return Err(Error::new_spanned(
                attr,
                "BloxCtx: a field may only have one BloxCtx annotation",
            ));
        }
        found = true;

        if path.is_ident(ANNOTATION_SELF_ID) {
            result = FieldAnnotation::SelfId;
        } else if path.is_ident(ANNOTATION_CTOR) {
            result = FieldAnnotation::Ctor;
        } else if path.is_ident(ANNOTATION_PROVIDES) {
            let tokens = parse_paren_tokens(attr)?;
            result = FieldAnnotation::Provides(tokens);
        } else if path.is_ident(ANNOTATION_DELEGATES) {
            let traits = parse_delegates_list(attr)?;
            if traits.is_empty() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: #[delegates(...)] requires at least one trait",
                ));
            }
            result = FieldAnnotation::Delegates(traits);
        }
    }

    Ok(result)
}

/// Parse the tokens inside `#[attr(TOKENS)]`, returning `TOKENS` as a `TokenStream`.
fn parse_paren_tokens(attr: &syn::Attribute) -> Result<TokenStream> {
    match &attr.meta {
        syn::Meta::List(list) => Ok(list.tokens.clone()),
        _ => Err(Error::new_spanned(
            attr,
            "BloxCtx: expected parenthesized argument, e.g. #[provides(HasPeerRef<R>)]",
        )),
    }
}

/// Parse `#[delegates(Trait1, Trait2, ...)]` into a list of trait paths.
fn parse_delegates_list(attr: &syn::Attribute) -> Result<Vec<syn::Path>> {
    let tokens = parse_paren_tokens(attr)?;
    let parsed: Punctuated<syn::Path, syn::Token![,]> =
        syn::parse::Parser::parse2(Punctuated::parse_terminated, tokens)?;
    Ok(parsed.into_iter().collect())
}
