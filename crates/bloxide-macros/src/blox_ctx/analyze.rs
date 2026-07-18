// Copyright 2025 Bloxide, all rights reserved
//! Field analysis for convention-based context derivation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    punctuated::Punctuated, spanned::Spanned, Data, DeriveInput, Error, Field, Fields, Ident,
    Result, Type, TypePath,
};

// Recognized field annotations for BloxCtx.
const ANNOTATION_PROVIDES: &str = "provides";
const ANNOTATION_PROVIDES_MUT: &str = "provides_mut";
const ANNOTATION_DELEGATES: &str = "delegates";
const ANNOTATION_BLOX_CTX: &str = "blox_ctx";
const ANNOTATION_SKIP: &str = "skip";

// Annotations that are always recognized by BloxCtx.
const ALL_RECOGNIZED_ANNOTATIONS: &[&str] = &[
    ANNOTATION_PROVIDES,
    ANNOTATION_PROVIDES_MUT,
    ANNOTATION_DELEGATES,
    ANNOTATION_BLOX_CTX,
];

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
    /// Optionally carries associated type bindings (e.g., `type Factory = F`).
    Accessor(TokenStream, Vec<AssocTypeBinding>),
    /// Behavior field — delegates trait impls to inner type.
    /// Requires `#[delegates(Trait1, Trait2, ...)]`.
    Delegates(Vec<syn::Path>),
    /// State field — no trait impl, zero-initialized in constructor.
    /// These are deprecated: all state should go in behavior objects.
    State,
}

/// An associated type binding parsed from `#[provides(Trait, type Name = Ty)]`.
#[derive(Clone)]
pub struct AssocTypeBinding {
    pub name: Ident,
    pub ty: Type,
}

/// Analysis result for the entire context struct.
pub struct ContextAnalysis {
    pub struct_name: Ident,
    pub generics: syn::Generics,
    pub fields: Vec<FieldAnalysis>,
}

/// Analysis result for a single field.
pub struct FieldAnalysis {
    pub name: Ident,
    pub ty: Type,
    pub role: FieldRole,
    /// Optional mutable accessor in addition to the primary role.
    /// Set by `#[provides_mut(...)]` when the field also has `#[provides(...)]`
    /// or a convention-based accessor role.
    pub mut_accessor: Option<(TokenStream, Ident)>,
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
    let mut seen_self_id = false;

    for field in fields {
        let analysis = analyze_field(field)?;

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
    })
}

/// Analyze a single field and determine its role.
fn analyze_field(field: &Field) -> Result<FieldAnalysis> {
    let name = field
        .ident
        .as_ref()
        .ok_or_else(|| Error::new(field.span(), "BloxCtx: expected named field"))?
        .clone();
    let ty = field.ty.clone();
    let span = field.span();

    // Check for explicit annotations first.
    let (explicit_role, mut_accessor) = extract_explicit_annotation(field, &name)?;
    if let Some(role) = explicit_role {
        return Ok(FieldAnalysis {
            name,
            ty,
            role,
            mut_accessor,
            span,
        });
    }

    // Apply convention-based inference.
    let role = infer_role_from_convention(&name, &ty)?;
    Ok(FieldAnalysis {
        name,
        ty,
        role,
        mut_accessor: None,
        span,
    })
}

/// Extract explicit annotation if present.
/// Returns (primary role, optional mutable accessor).
/// Returns Ok((None, None)) if no BloxCtx annotation is present.
fn extract_explicit_annotation(
    field: &Field,
    field_name: &Ident,
) -> Result<(Option<FieldRole>, Option<(TokenStream, Ident)>)> {
    let mut result = None;
    let mut mut_accessor = None;

    for attr in &field.attrs {
        let path = attr.path();

        // Reject old annotations that are no longer supported.
        if path.is_ident("self_id") {
            return Err(Error::new_spanned(
                attr,
                "BloxCtx: #[self_id] is no longer supported. Use the naming convention: self_id: ActorId",
            ));
        }
        if path.is_ident("ctor") {
            return Err(Error::new_spanned(
                attr,
                "BloxCtx: #[ctor] is no longer supported. Use naming conventions or #[blox_ctx(skip)] to suppress auto-detection.",
            ));
        }

        // Check if this is a BloxCtx annotation.
        let is_recognized = ALL_RECOGNIZED_ANNOTATIONS.iter().any(|a| path.is_ident(a));

        if !is_recognized {
            continue;
        }

        if path.is_ident(ANNOTATION_PROVIDES) {
            let (trait_tokens, assoc_types) = parse_provides_args(attr)?;
            if result.is_some() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: a field may only have one primary BloxCtx annotation (#[provides] or #[delegates] or #[blox_ctx(skip)])",
                ));
            }
            result = Some(FieldRole::Accessor(trait_tokens, assoc_types));
        } else if path.is_ident(ANNOTATION_PROVIDES_MUT) {
            if mut_accessor.is_some() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: a field may only have one #[provides_mut] annotation",
                ));
            }
            let (trait_tokens, method_name) = parse_provides_mut_args(attr, field_name)?;
            mut_accessor = Some((trait_tokens, method_name));
        } else if path.is_ident(ANNOTATION_DELEGATES) {
            let traits = parse_delegates_list(attr)?;
            if traits.is_empty() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: #[delegates(...)] requires at least one trait",
                ));
            }
            if result.is_some() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: a field may only have one primary BloxCtx annotation (#[provides] or #[delegates] or #[blox_ctx(skip)])",
                ));
            }
            result = Some(FieldRole::Delegates(traits));
        } else if is_blox_ctx_skip(attr) {
            if result.is_some() {
                return Err(Error::new_spanned(
                    attr,
                    "BloxCtx: a field may only have one primary BloxCtx annotation (#[provides] or #[delegates] or #[blox_ctx(skip)])",
                ));
            }
            result = Some(FieldRole::Ctor);
        }
    }

    Ok((result, mut_accessor))
}

/// Check whether an attribute is `#[blox_ctx(skip)]`.
fn is_blox_ctx_skip(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    if !path.is_ident(ANNOTATION_BLOX_CTX) {
        return false;
    }
    match &attr.meta {
        syn::Meta::List(list) => list
            .parse_args::<syn::Ident>()
            .map(|ident| ident == ANNOTATION_SKIP)
            .unwrap_or(false),
        _ => false,
    }
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
        return Ok(FieldRole::Accessor(trait_tokens, Vec::new()));
    }

    // Rule 3: `foo_factory: fn(...) -> ...` → Accessor for HasFooFactory
    // Pattern: field name ends with `_factory` and type is fn pointer
    if name_str.ends_with("_factory") {
        if is_fn_type(ty) {
            let trait_name = format!("Has{}", to_pascal_case(&name_str));
            let trait_ident = syn::Ident::new(&trait_name, name.span());
            // Factory traits typically don't have runtime generics, but check
            let runtime_generic = extract_runtime_generic(ty);
            let trait_tokens = if let Some(rg) = runtime_generic {
                quote!( #trait_ident<#rg> )
            } else {
                quote!( #trait_ident )
            };
            return Ok(FieldRole::Accessor(trait_tokens, Vec::new()));
        } else {
            // Type alias like WorkerSpawnFn<R> — treat as constructor parameter
            return Ok(FieldRole::Ctor);
        }
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
    match ty {
        Type::BareFn(_) => true,
        Type::Path(TypePath { path, .. }) => path.segments.iter().any(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "fn" | "Fn" | "FnMut" | "FnOnce"
            )
        }),
        _ => false,
    }
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

/// Parse `#[provides(TraitPath, type Name = Ty, ...)]` into trait tokens and
/// any associated type bindings.
///
/// The first positional argument is the trait path. Subsequent `type X = Y`
/// clauses are associated type bindings emitted before the method in the impl.
fn parse_provides_args(attr: &syn::Attribute) -> Result<(TokenStream, Vec<AssocTypeBinding>)> {
    let tokens = parse_paren_tokens(attr)?;

    // Custom parser: first item is a trait path, subsequent items are
    // `type Name = Ty` bindings, all comma-separated.
    let (trait_tokens, assoc_types) = syn::parse::Parser::parse2(
        |input: syn::parse::ParseStream| {
            // First: trait path (e.g. HasSpawnFactory<R>)
            let trait_path: syn::Path = input.parse()?;
            let trait_tokens = quote!( #trait_path );

            let mut assoc_types = Vec::new();
            while input.peek(syn::Token![,]) {
                let _comma: syn::Token![,] = input.parse()?;
                let _type_kw: syn::Token![type] = input.parse()?;
                let name: Ident = input.parse()?;
                let _eq: syn::Token![=] = input.parse()?;
                let ty: Type = input.parse()?;
                assoc_types.push(AssocTypeBinding { name, ty });
            }

            Ok((trait_tokens, assoc_types))
        },
        tokens,
    )?;

    Ok((trait_tokens, assoc_types))
}

/// Parse `#[provides_mut(TraitPath)]` or `#[provides_mut(TraitPath, method_name)]`.
///
/// The first argument is the trait path. The optional second argument is the
/// method name; if omitted, it defaults to `{field_name}_mut`.
fn parse_provides_mut_args(
    attr: &syn::Attribute,
    field_name: &Ident,
) -> Result<(TokenStream, Ident)> {
    let tokens = parse_paren_tokens(attr)?;

    let (trait_tokens, method_name) = syn::parse::Parser::parse2(
        |input: syn::parse::ParseStream| {
            let trait_path: syn::Path = input.parse()?;
            let trait_tokens = quote!( #trait_path );

            // Optional second arg: method name ident.
            let method_name = if input.peek(syn::Token![,]) {
                let _comma: syn::Token![,] = input.parse()?;
                input.parse::<Ident>()?
            } else {
                // Default: {field_name}_mut
                let mut name = field_name.to_string();
                name.push_str("_mut");
                Ident::new(&name, field_name.span())
            };

            Ok((trait_tokens, method_name))
        },
        tokens,
    )?;

    Ok((trait_tokens, method_name))
}

/// Parse the tokens inside `#[attr(TOKENS)]`, returning `TOKENS` as a `TokenStream`.
fn parse_paren_tokens(attr: &syn::Attribute) -> Result<TokenStream> {
    match &attr.meta {
        syn::Meta::List(list) => Ok(list.tokens.clone()),
        _ => Err(Error::new_spanned(
            attr,
            "BloxCtx: expected parenthesized argument, e.g., #[delegates(TraitName)] or field name following convention (foo_ref: ActorRef<M, R>)",
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
