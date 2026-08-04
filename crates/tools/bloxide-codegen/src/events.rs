// Copyright 2025 Bloxide, all rights reserved
//! Generate event enums and their trait implementations from `EventConfig`.
//!
//! When any mailbox has `feature = Some(...)`, emits paired `#[cfg]` variants:
//! one under `#[cfg(not(feature = "..."))]` excluding feature-gated mailboxes,
//! and one under `#[cfg(feature = "...")]` including them with different
//! generics.

use quote::{format_ident, quote};

use crate::schema::{EventConfig, MailboxConfig};
use crate::util::{to_snake_case, to_upper_snake_case, HEADER};

/// Parse a `syn::Generics` from an optional string, returning default if None.
fn parse_generics(g: Option<&str>) -> anyhow::Result<syn::Generics> {
    g.map(|s| {
        syn::parse_str::<syn::Generics>(s)
            .map_err(|e| anyhow::anyhow!("invalid generics '{}': {}", s, e))
    })
    .transpose()
    .map(|opt| opt.unwrap_or_default())
}

/// Build the derive attribute.
fn build_derive_attr(config: &EventConfig) -> proc_macro2::TokenStream {
    if let Some(ref derives) = config.derives {
        if derives.is_empty() {
            quote! {}
        } else {
            let derive_paths: Vec<syn::Path> = derives
                .iter()
                .filter_map(|d| syn::parse_str::<syn::Path>(d).ok())
                .collect();
            quote! { #[derive(#(#derive_paths),*)] }
        }
    } else {
        quote! { #[derive(Debug)] }
    }
}

/// Render the message type for a mailbox.
fn msg_type_ts(mb: &MailboxConfig) -> anyhow::Result<proc_macro2::TokenStream> {
    if let Some(ref path) = mb.message_path {
        let path = syn::parse_str::<syn::Path>(path)
            .map_err(|e| anyhow::anyhow!("invalid message_path '{}': {}", path, e))?;
        Ok(quote! { #path })
    } else {
        let ident = format_ident!("{}", mb.message);
        Ok(quote! { #ident })
    }
}

/// Returns true when a mailbox message type reference (e.g.
/// `bloxide_peers::PeerCtrl<pool_messages::WorkerMsg, R>`) mentions any of the
/// declared generic type parameters — as the sole type argument (`Bar<R>`),
/// after a comma (`Bar<T, R>`), nested (`Foo<Bar<T, R>>`), or as a path
/// segment (`R::Assoc`). Unparseable references conservatively count as
/// unused so the phantom marker is still emitted.
fn msg_uses_type_param(msg_ref: &str, type_param_idents: &[String]) -> bool {
    let path = match syn::parse_str::<syn::Path>(msg_ref) {
        Ok(path) => path,
        Err(_) => return false,
    };
    path_uses_type_param(&path, type_param_idents)
}

fn path_uses_type_param(path: &syn::Path, type_param_idents: &[String]) -> bool {
    path.segments.iter().any(|segment| {
        type_param_idents
            .iter()
            .any(|ident| segment.ident == ident.as_str())
            || args_use_type_param(&segment.arguments, type_param_idents)
    })
}

fn args_use_type_param(args: &syn::PathArguments, type_param_idents: &[String]) -> bool {
    let syn::PathArguments::AngleBracketed(args) = args else {
        return false;
    };
    args.args.iter().any(|arg| match arg {
        syn::GenericArgument::Type(ty) => type_uses_type_param(ty, type_param_idents),
        syn::GenericArgument::AssocType(assoc) => {
            type_uses_type_param(&assoc.ty, type_param_idents)
        }
        _ => false,
    })
}

fn type_uses_type_param(ty: &syn::Type, type_param_idents: &[String]) -> bool {
    match ty {
        syn::Type::Path(ty) => {
            ty.qself.as_ref().map_or(false, |qself| {
                type_uses_type_param(&qself.ty, type_param_idents)
            }) || path_uses_type_param(&ty.path, type_param_idents)
        }
        syn::Type::Reference(ty) => type_uses_type_param(&ty.elem, type_param_idents),
        syn::Type::Paren(ty) => type_uses_type_param(&ty.elem, type_param_idents),
        syn::Type::Group(ty) => type_uses_type_param(&ty.elem, type_param_idents),
        syn::Type::Slice(ty) => type_uses_type_param(&ty.elem, type_param_idents),
        syn::Type::Array(ty) => type_uses_type_param(&ty.elem, type_param_idents),
        syn::Type::Ptr(ty) => type_uses_type_param(&ty.elem, type_param_idents),
        syn::Type::Tuple(ty) => ty
            .elems
            .iter()
            .any(|ty| type_uses_type_param(ty, type_param_idents)),
        _ => false,
    }
}

/// Generate a single variant of the event enum + all its impl blocks.
///
/// `mailboxes` is the filtered list of mailboxes for this variant.
/// `generics` and `feature_cfg` control the generics and `#[cfg]` attribute.
fn generate_variant(
    config: &EventConfig,
    mailboxes: &[&MailboxConfig],
    generics: &syn::Generics,
    feature_cfg: Option<&str>,
) -> anyhow::Result<proc_macro2::TokenStream> {
    let event_ident = format_ident!("{}", config.name);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let derive_attr = build_derive_attr(config);

    // Build use statements
    let mut use_statements = vec![quote! {
        use ::bloxide_core::messaging::Envelope;
    }];
    if !generics.params.is_empty() {
        use_statements.push(quote! {
            use ::bloxide_core::capability::BloxRuntime;
        });
    }

    // Enum variants
    let mut enum_variants = vec![quote! {
        /// Lifecycle command (Start/Reset/Stop/Ping).
        Lifecycle(::bloxide_core::lifecycle::LifecycleCommand)
    }];

    for mb in mailboxes {
        let variant_ident = format_ident!("{}", mb.variant);
        let msg_type = msg_type_ts(mb)?;
        enum_variants.push(quote! {
            #variant_ident(Envelope<#msg_type>)
        });
    }

    // If the enum has generic type params but none of the variants use them
    // (e.g. non-dynamic variant where the only R-using mailbox is feature-gated
    // away), add a PhantomData variant to satisfy the borrow checker.
    let type_param_idents: Vec<String> = generics
        .type_params()
        .map(|tp| tp.ident.to_string())
        .collect();
    let any_msg_uses_generic = mailboxes.iter().any(|mb| {
        let msg_ref = mb.message_path.as_deref().unwrap_or(&mb.message);
        msg_uses_type_param(msg_ref, &type_param_idents)
    });
    let phantom_marker = if !type_param_idents.is_empty() && !any_msg_uses_generic {
        let phantom_ty = if type_param_idents.len() == 1 {
            let id = format_ident!("{}", type_param_idents[0]);
            quote! { ::core::marker::PhantomData<#id> }
        } else {
            let fields: Vec<_> = type_param_idents
                .iter()
                .map(|ident| {
                    let id = format_ident!("{}", ident);
                    quote! { #id }
                })
                .collect();
            quote! { ::core::marker::PhantomData<(#(#fields),*)> }
        };
        Some(quote! {
            /// Marker for unused generic type parameters.
            #[doc(hidden)]
            _Phantom(#phantom_ty)
        })
    } else {
        None
    };

    let enum_def = quote! {
        #derive_attr
        pub enum #event_ident #generics {
            #(#enum_variants,)*
            #phantom_marker
        }
    };

    // From<Envelope<M>> impls
    let mut from_impls = Vec::new();
    for mb in mailboxes {
        let variant_ident = format_ident!("{}", mb.variant);
        let msg_type = msg_type_ts(mb)?;
        from_impls.push(quote! {
            impl #impl_generics ::core::convert::From<Envelope<#msg_type>> for #event_ident #ty_generics #where_clause {
                fn from(envelope: Envelope<#msg_type>) -> Self {
                    #event_ident::#variant_ident(envelope)
                }
            }
        });
    }

    // From<LifecycleCommand>
    let from_lifecycle = quote! {
        impl #impl_generics ::core::convert::From<::bloxide_core::lifecycle::LifecycleCommand> for #event_ident #ty_generics #where_clause {
            fn from(cmd: ::bloxide_core::lifecycle::LifecycleCommand) -> Self {
                #event_ident::Lifecycle(cmd)
            }
        }
    };

    // EventTag impl
    let mut event_tag_arms = vec![quote! {
        Self::Lifecycle(..) => ::bloxide_core::event_tag::LIFECYCLE_TAG
    }];
    for (idx, mb) in mailboxes.iter().enumerate() {
        let variant_ident = format_ident!("{}", mb.variant);
        let tag = idx as u8;
        event_tag_arms.push(quote! {
            Self::#variant_ident(..) => #tag
        });
    }
    // Add wildcard arm for _Phantom marker variant if present. Uses
    // WILDCARD_TAG (never matched by a real mailbox) instead of tag 0, which
    // would collide with the first mailbox variant's tag.
    if phantom_marker.is_some() {
        event_tag_arms.push(quote! {
            Self::_Phantom(..) => ::bloxide_core::event_tag::WILDCARD_TAG
        });
    }

    let event_tag_impl = quote! {
        impl #impl_generics ::bloxide_core::event_tag::EventTag for #event_ident #ty_generics #where_clause {
            #[inline]
            fn event_tag(&self) -> u8 {
                match self {
                    #(#event_tag_arms,)*
                }
            }
        }
    };

    // LifecycleEvent impl — wildcard _ arm handles all non-Lifecycle variants
    // (including _Phantom if present)
    let lifecycle_impl = quote! {
        impl #impl_generics ::bloxide_core::event_tag::LifecycleEvent for #event_ident #ty_generics #where_clause {
            fn as_lifecycle_command(&self) -> ::core::option::Option<::bloxide_core::lifecycle::LifecycleCommand> {
                match self {
                    Self::Lifecycle(cmd) => ::core::option::Option::Some(*cmd),
                    _ => ::core::option::Option::None,
                }
            }
        }
    };

    // Tag constants and accessor methods
    let mut tag_constants = Vec::new();
    let mut accessor_methods = Vec::new();

    for (idx, mb) in mailboxes.iter().enumerate() {
        let variant_ident = format_ident!("{}", mb.variant);
        let msg_type = msg_type_ts(mb)?;
        let tag = idx as u8;

        let upper_snake = to_upper_snake_case(&mb.variant);
        let const_name = format_ident!("{}_TAG", upper_snake);

        tag_constants.push(quote! {
            /// Event tag for this variant, used for fast dispatch filtering.
            pub const #const_name: u8 = #tag;
        });

        let snake_name = to_snake_case(&mb.variant);
        let envelope_method = format_ident!("{}_envelope", snake_name);
        let payload_method = format_ident!("{}_payload", snake_name);

        accessor_methods.push(quote! {
            /// Returns the envelope if this event matches this variant.
            pub fn #envelope_method(&self) -> ::core::option::Option<&Envelope<#msg_type>> {
                match self { #event_ident::#variant_ident(ref e) => ::core::option::Option::Some(e), _ => ::core::option::Option::None }
            }
            /// Returns the message payload if this event matches this variant.
            pub fn #payload_method(&self) -> ::core::option::Option<&#msg_type> {
                match self { #event_ident::#variant_ident(ref e) => ::core::option::Option::Some(&e.1), _ => ::core::option::Option::None }
            }
        });
    }

    // Lifecycle helpers
    let lifecycle_helpers = quote! {
        /// Create a Start lifecycle event.
        pub fn start() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Start)
        }

        /// Create a Reset lifecycle event.
        pub fn reset() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Reset)
        }

        /// Create a Stop lifecycle event.
        pub fn stop() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Stop)
        }

        /// Create a Ping lifecycle event.
        pub fn ping() -> Self {
            Self::Lifecycle(::bloxide_core::lifecycle::LifecycleCommand::Ping)
        }
    };

    let impl_block = quote! {
        impl #impl_generics #event_ident #ty_generics #where_clause {
            #(#tag_constants)*
            #(#accessor_methods)*
            #lifecycle_helpers
        }
    };

    // Build a list of top-level items so #[cfg] can be applied to each one.
    // Applying #[cfg] to a combined token stream only attaches it to the first
    // item; we need it on every item (use, enum, impl, etc.).
    let items: Vec<proc_macro2::TokenStream> = use_statements
        .iter()
        .map(|s| quote! { #s })
        .chain(std::iter::once(quote! { #enum_def }))
        .chain(from_impls.iter().map(|f| quote! { #f }))
        .chain(std::iter::once(quote! { #from_lifecycle }))
        .chain(std::iter::once(quote! { #event_tag_impl }))
        .chain(std::iter::once(quote! { #lifecycle_impl }))
        .chain(std::iter::once(quote! { #impl_block }))
        .collect();

    Ok(match feature_cfg {
        Some(feat) => {
            // Feature-gated variant — wrap each item in #[cfg(feature)]
            let gated: Vec<_> = items
                .iter()
                .map(|item| quote! { #[cfg(feature = #feat)] #item })
                .collect();
            quote! { #(#gated)* }
        }
        None if config.feature.is_some() => {
            // Non-feature variant — wrap each item in #[cfg(not(feature))]
            let feat_name = config.feature.as_deref().unwrap_or("dynamic");
            let gated: Vec<_> = items
                .iter()
                .map(|item| quote! { #[cfg(not(feature = #feat_name))] #item })
                .collect();
            quote! { #(#gated)* }
        }
        None => {
            // Single-variant mode — no feature gating
            quote! { #(#items)* }
        }
    })
}

pub fn generate(config: &EventConfig) -> anyhow::Result<String> {
    // Check if any mailbox is feature-gated
    let has_feature = config.mailboxes.iter().any(|mb| mb.feature.is_some());

    if !has_feature {
        // Single-variant mode — no feature gating
        let generics = parse_generics(config.generics.as_deref())?;
        let mailboxes: Vec<&MailboxConfig> = config.mailboxes.iter().collect();
        let tokens = generate_variant(config, &mailboxes, &generics, None)?;

        let raw = tokens.to_string();
        let file = syn::parse_str::<syn::File>(&raw)
            .map_err(|e| anyhow::anyhow!("syn parsing failed: {}", e))?;
        let formatted = prettyplease::unparse(&file);
        return Ok(format!("{}{}", HEADER, formatted));
    }

    // Paired-variant mode
    let feat_name = config.feature.as_deref().ok_or_else(|| {
        anyhow::anyhow!("mailboxes have feature gates but [event].feature is not set")
    })?;

    // Non-feature variant: exclude feature-gated mailboxes
    let base_generics = parse_generics(config.generics.as_deref())?;
    let base_mailboxes: Vec<&MailboxConfig> = config
        .mailboxes
        .iter()
        .filter(|mb| mb.feature.is_none())
        .collect();
    let base_tokens = generate_variant(config, &base_mailboxes, &base_generics, None)?;

    // Feature variant: include all mailboxes, use feature_generics
    let feature_generics = parse_generics(config.feature_generics.as_deref())?;
    let all_mailboxes: Vec<&MailboxConfig> = config.mailboxes.iter().collect();
    let feature_tokens =
        generate_variant(config, &all_mailboxes, &feature_generics, Some(feat_name))?;

    let tokens = quote! {
        #base_tokens
        #feature_tokens
    };

    let raw = tokens.to_string();
    let file = syn::parse_str::<syn::File>(&raw)
        .map_err(|e| anyhow::anyhow!("syn parsing failed: {}", e))?;
    let formatted = prettyplease::unparse(&file);

    Ok(format!("{}{}", HEADER, formatted))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::msg_uses_type_param;

    fn params(idents: &[&str]) -> Vec<String> {
        idents.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_param_as_sole_type_argument() {
        assert!(msg_uses_type_param("foo::Bar<R>", &params(&["R"])));
    }

    #[test]
    fn detects_param_after_comma() {
        assert!(msg_uses_type_param("foo::Bar<T, R>", &params(&["R"])));
        assert!(msg_uses_type_param("foo::Bar< T , R >", &params(&["R"])));
    }

    #[test]
    fn detects_param_in_nested_generics() {
        assert!(msg_uses_type_param("Foo<Bar<T, R>>", &params(&["R"])));
        assert!(msg_uses_type_param(
            "blox_ctx_pool_ref::SpawnedWorker<bloxide_peers::PeerCtrl<pool_messages::WorkerMsg, R>, R>",
            &params(&["R"])
        ));
    }

    #[test]
    fn detects_param_as_path_segment() {
        assert!(msg_uses_type_param("R::Backend", &params(&["R"])));
    }

    #[test]
    fn detects_each_of_multiple_params() {
        let params = params(&["M", "R"]);
        assert!(msg_uses_type_param("foo::Bar<M>", &params));
        assert!(msg_uses_type_param("foo::Bar<R>", &params));
        assert!(!msg_uses_type_param("foo::Bar<u32>", &params));
    }

    #[test]
    fn unused_param_is_not_detected() {
        assert!(!msg_uses_type_param(
            "pool_messages::PoolMsg",
            &params(&["R"])
        ));
        assert!(!msg_uses_type_param("foo::Bar<T>", &params(&["R"])));
        // Identifiers that merely start with the param name must not match.
        assert!(!msg_uses_type_param("foo::Bar<Reply>", &params(&["R"])));
        assert!(!msg_uses_type_param("foo::Bar<R2>", &params(&["R"])));
    }

    #[test]
    fn unparseable_reference_counts_as_unused() {
        assert!(!msg_uses_type_param("not a path <", &params(&["R"])));
    }
}
