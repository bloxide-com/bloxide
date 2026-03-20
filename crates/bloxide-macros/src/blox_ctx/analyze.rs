// Copyright 2025 Bloxide, all rights reserved
//! Field analysis for convention-based context derivation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    punctuated::Punctuated, spanned::Spanned, Data, DeriveInput, Error, Field, Fields, Ident,
    Result, Type, TypePath,
};

// Recognized field annotations for BloxCtx.
const ANNOTATION_SELF_ID: &str = "self_id";
const ANNOTATION_CTOR: &str = "ctor";
const ANNOTATION_PROVIDES: &str = "provides";
const ANNOTATION_DELEGATES: &str = "delegates";

const ALL_DEPRECATED_ANNOTATIONS: &[&str] =
    &[ANNOTATION_SELF_ID, ANNOTATION_CTOR, ANNOTATION_PROVIDES];

/// Categorization of a field's role in the context.
#[derive(Clone)]
pub enum FieldRole {
    /// `self_id: ActorId` field — auto-generates `impl HasSelfId`.
    SelfId,
    /// Constructor parameter — passed to `new()`, no trait impl.
    /// Used for factory closures and runtime-injected values.
    Ctor,
    /// Accessor field — generates trait impl with single accessor method.
    /// The trait must have a method matching the field name.
    /// Contains the trait path tokens (e.g., `quote!( HasPeerRef<R> )`).
    Accessor(TokenStream),
    /// Behavior field — delegates trait impls to inner type.
    /// Requires `#[delegates(Trait1, Trait2, ...)]`.
    Delegates(Vec<syn::Path>),
    /// State field — no trait impl, zero-initialized in constructor.
    /// These are deprecated: all state should go in behavior objects.
    State,
}

/// Analysis result for the entire context struct.
pub struct ContextAnalysis {
    pub struct_name: Ident,
    pub generics: syn::Generics,
    pub fields: Vec<FieldAnalysis>,
    /// Whether any deprecated annotations were used.
    pub has_deprecated_annotations: bool,
}

/// Analysis result for a single field.
pub struct FieldAnalysis {
    pub name: Ident,
    pub ty: Type,
    pub role: FieldRole,
    /// Span for error reporting.
    pub span: proc_macro2::Span,
}

/// Analyze a derive input and produce context analysis.
pub fn analyze(input: &DeriveInput) -> Result<ContextAnalysis> {
    let struct_name = input.ident.clone();
    let generics = input.generics.clone();

    // Only support named-field structs.
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
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

    let mut field_analyses = Vec::new();
    let mut has_deprecated_annotations = false;
    let mut seen_self_id = false;

    for field in fields {
        let analysis = analyze_field(field, &mut has_deprecated_annotations)?;

        // Check for duplicate self_id fields.
        if matches!(analysis.role, FieldRole::SelfId) {
            if seen_self_id {
                return Err(Error::new(
                    analysis.span,
                    "BloxCtx: multiple `self_id` fields found; only one is allowed",
                ));
            }
            seen_self_id = true;
        }

        field_analyses.push(analysis);
    }

    Ok(ContextAnalysis {
        struct_name,
        generics,
        fields: field_analyses,
        has_deprecated_annotations,
    })
}

/// Analyze a single field and determine its role.
fn analyze_field(field: &Field, has_deprecated_annotations: &mut bool) -> Result<FieldAnalysis> {
    let name = field
        .ident
        .as_ref()
        .ok_or_else(|| Error::new(field.span(), "BloxCtx: expected named field"))?
        .clone();
    let ty = field.ty.clone();
    let span = field.span();

    // Check for explicit annotations first.
    let explicit_role = extract_explicit_annotation(field, has_deprecated_annotations)?;
    if let Some(role) = explicit_role {
        return Ok(FieldAnalysis {
            name,
            ty,
            role,
            span,
        });
    }

    // Apply convention-based inference.
    let role = infer_role_from_convention(&name, &ty)?;
    Ok(FieldAnalysis {
        name,
        ty,
        role,
        span,
    })
}

/// Extract explicit annotation if present.
/// Returns Ok(None) if no BloxCtx annotation is present.
fn extract_explicit_annotation(
    field: &Field,
    has_deprecated_annotations: &mut bool,
) -> Result<Option<FieldRole>> {
    let mut result = None;

    for attr in &field.attrs {
        let path = attr.path();

        // Check if this is a BloxCtx annotation.
        let is_deprecated = ALL_DEPRECATED_ANNOTATIONS.iter().any(|a| path.is_ident(a));
        let is_delegates = path.is_ident(ANNOTATION_DELEGATES);

        if !is_deprecated && !is_delegates {
            continue;
        }

        if result.is_some() {
            return Err(Error::new_spanned(
                attr,
                "BloxCtx: a field may only have one BloxCtx annotation",
            ));
        }

        if is_deprecated {
            *has_deprecated_annotations = true;
        }

        if path.is_ident(ANNOTATION_SELF_ID) {
            result = Some(FieldRole::SelfId);
        } else if path.is_ident(ANNOTATION_CTOR) {
            result = Some(FieldRole::Ctor);
        } else if path.is_ident(ANNOTATION_PROVIDES) {
            let tokens = parse_paren_tokens(attr)?;
            result = Some(FieldRole::Accessor(tokens));
        } else if path.is_ident(ANNOTATION_DELEGATES) {
            let traits = parse_delegates_list(attr)?;
            if traits.is_empty() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: #[delegates(...)] requires at least one trait",
                ));
            }
            result = Some(FieldRole::Delegates(traits));
        }
    }

    Ok(result)
}

/// Infer field role from naming conventions.
fn infer_role_from_convention(name: &Ident, ty: &Type) -> Result<FieldRole> {
    let name_str = name.to_string();

    // Rule 1: `self_id: ActorId` → SelfId
    if name_str == "self_id" && is_actor_id_type(ty) {
        return Ok(FieldRole::SelfId);
    }

    // Rule 2: `foo_ref: ActorRef<M, R>` → Accessor for HasFooRef<R>
    // Pattern: field name ends with `_ref` and type is ActorRef<...>
    if name_str.ends_with("_ref") && is_actor_ref_type(ty) {
        let trait_name = format!("Has{}", to_pascal_case(&name_str));
        let trait_ident = syn::Ident::new(&trait_name, name.span());
        // Extract the runtime generic R from ActorRef<M, R>
        let runtime_generic = extract_runtime_generic(ty);
        let trait_tokens = if let Some(rg) = runtime_generic {
            quote!( #trait_ident<#rg> )
        } else {
            quote!( #trait_ident )
        };
        return Ok(FieldRole::Accessor(trait_tokens));
    }

    // Rule 3: `foo_factory: fn(...) -> ...` → Accessor for HasFooFactory
    // Pattern: field name ends with `_factory` and type is fn pointer
    if name_str.ends_with("_factory") && is_fn_type(ty) {
        let trait_name = format!("Has{}", to_pascal_case(&name_str));
        let trait_ident = syn::Ident::new(&trait_name, name.span());
        // Factory traits typically don't have runtime generics, but check
        let runtime_generic = extract_runtime_generic(ty);
        let trait_tokens = if let Some(rg) = runtime_generic {
            quote!( #trait_ident<#rg> )
        } else {
            quote!( #trait_ident )
        };
        return Ok(FieldRole::Accessor(trait_tokens));
    }

    // Rule 4: Any other field type → treat as state (deprecated)
    // Users should move state to behavior objects.
    // For backward compatibility, we treat this as a constructor parameter
    // if it's an ActorRef (for self_ref patterns) or factory, otherwise state.
    if is_actor_ref_type(ty) {
        // e.g., `self_ref` without annotation → constructor param
        return Ok(FieldRole::Ctor);
    }

    // Default: treat as state field (zero-initialized)
    // This is deprecated but supported for backward compatibility.
    Ok(FieldRole::State)
}

/// Check if type is `ActorId`.
fn is_actor_id_type(ty: &Type) -> bool {
    let ty_str = type_to_string(ty);
    ty_str == "ActorId" || ty_str.ends_with("::ActorId")
}

/// Check if type is `ActorRef<...>`.
fn is_actor_ref_type(ty: &Type) -> bool {
    let ty_str = type_to_string(ty);
    ty_str.starts_with("ActorRef<") || ty_str.contains("::ActorRef<")
}

/// Check if type is a function pointer `fn(...) -> ...`.
fn is_fn_type(ty: &Type) -> bool {
    matches!(ty, Type::Path(TypePath { path, .. }) if path.segments.iter().any(|s| {
        matches!(s.ident.to_string().as_str(), "fn" | "Fn" | "FnMut" | "FnOnce")
    }))
}

/// Extract the runtime generic parameter R from a type like `ActorRef<M, R>`.
fn extract_runtime_generic(ty: &Type) -> Option<syn::GenericArgument> {
    if let Type::Path(TypePath { path, .. }) = ty {
        let last_seg = path.segments.last()?;
        if let syn::PathArguments::AngleBracketed(args) = &last_seg.arguments {
            // For ActorRef<M, R>, the last argument is typically R
            args.args.last().cloned()
        } else {
            None
        }
    } else {
        None
    }
}

/// Convert type to string for pattern matching.
fn type_to_string(ty: &Type) -> String {
    // Use quote to get a reliable string representation
    let tokens = quote!(#ty);
    tokens.to_string().replace(' ', "")
}

/// Convert snake_case to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// Parse the tokens inside `#[attr(TOKENS)]`, returning `TOKENS` as a `TokenStream`.
fn parse_paren_tokens(attr: &syn::Attribute) -> Result<TokenStream> {
    match &attr.meta {
        syn::Meta::List(list) => Ok(list.tokens.clone()),
        _ => Err(Error::new_spanned(
            attr,
            "BloxCtx: expected parenthesized argument, e.g., #[provides(HasPeerRef<R>)]",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("peer_ref"), "PeerRef");
        assert_eq!(to_pascal_case("self_ref"), "SelfRef");
        assert_eq!(to_pascal_case("worker_factory"), "WorkerFactory");
    }
}
