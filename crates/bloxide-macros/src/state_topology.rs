// Copyright 2025 Bloxide, all rights reserved
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use std::collections::HashMap;
use syn::{DeriveInput, Error, Ident};

/// Parse the `#[parent(Variant)]` attribute and return the variant name.
fn parse_parent_attr(attr: &syn::Attribute) -> syn::Result<Option<Ident>> {
    if attr.path().is_ident("parent") {
        let ident: Ident = attr.parse_args()?;
        Ok(Some(ident))
    } else {
        Ok(None)
    }
}

/// Returns true if any attribute is `#[composite]`.
fn has_composite_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("composite"))
}

/// Parse `#[handler_fns(FN1, FN2, ...)]` from the enum-level attributes.
/// Returns `Some(Vec<Ident>)` if the attribute is present, `None` otherwise.
fn parse_handler_fns_attr(attrs: &[syn::Attribute]) -> syn::Result<Option<Vec<Ident>>> {
    for attr in attrs {
        if attr.path().is_ident("handler_fns") {
            let idents: syn::punctuated::Punctuated<Ident, syn::Token![,]> =
                attr.parse_args_with(syn::punctuated::Punctuated::parse_terminated)?;
            return Ok(Some(idents.into_iter().collect()));
        }
    }
    Ok(None)
}

/// Convert an enum name like `PingState` to a snake_case macro name like `ping_state`.
fn to_snake_case_ident(s: &str, span: Span) -> Ident {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            let prev_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit();
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if prev_lower || (chars[i - 1].is_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    Ident::new(&out, span)
}

pub(crate) fn derive_state_topology_inner(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let enum_name = &input.ident;

    // Validate that the enum has #[repr(u8)] or #[repr(usize)]
    let has_repr = input.attrs.iter().any(|a| {
        if a.path().is_ident("repr") {
            let result: syn::Result<syn::Ident> = a.parse_args();
            if let Ok(ident) = result {
                return ident == "u8" || ident == "usize";
            }
        }
        false
    });
    if !has_repr {
        return Err(Error::new_spanned(
            input,
            "#[derive(StateTopology)] requires #[repr(u8)] (or #[repr(usize)]) on the enum",
        ));
    }

    // Parse optional #[handler_fns(FN1, FN2, ...)] enum-level attribute
    let handler_fns = parse_handler_fns_attr(&input.attrs)?;

    let variants = match &input.data {
        syn::Data::Enum(e) => &e.variants,
        _ => {
            return Err(Error::new_spanned(
                input,
                "#[derive(StateTopology)] only works on enums",
            ))
        }
    };

    let state_count = variants.len();

    // Validate handler_fns length matches variant count if provided
    if let Some(ref fns) = handler_fns {
        if fns.len() != state_count {
            return Err(Error::new_spanned(
                input,
                format!(
                    "#[handler_fns(...)] must list exactly {} entries (one per variant), got {}",
                    state_count,
                    fns.len()
                ),
            ));
        }
    }

    // Build variant info: name, index, parent_name, is_composite
    struct VariantInfo {
        name: Ident,
        index: usize,
        parent_name: Option<Ident>,
        is_composite: bool,
    }

    let mut variant_info: Vec<VariantInfo> = Vec::new();
    for (i, variant) in variants.iter().enumerate() {
        // Validate: variants must be unit variants
        if !matches!(variant.fields, syn::Fields::Unit) {
            return Err(Error::new_spanned(
                variant,
                "#[derive(StateTopology)] requires all variants to be unit variants",
            ));
        }

        let mut parent_name = None;
        for attr in &variant.attrs {
            if let Some(p) = parse_parent_attr(attr)? {
                if parent_name.is_some() {
                    return Err(Error::new_spanned(
                        attr,
                        "duplicate #[parent(...)] attribute",
                    ));
                }
                parent_name = Some(p);
            }
        }
        let is_composite = has_composite_attr(&variant.attrs);

        variant_info.push(VariantInfo {
            name: variant.ident.clone(),
            index: i,
            parent_name,
            is_composite,
        });
    }

    // Build a name -> index map for validation and path computation
    let name_to_index: HashMap<String, usize> = variant_info
        .iter()
        .map(|v| (v.name.to_string(), v.index))
        .collect();

    // Validate: parent names must exist in the enum
    for v in &variant_info {
        if let Some(ref pname) = v.parent_name {
            if !name_to_index.contains_key(&pname.to_string()) {
                return Err(Error::new_spanned(
                    pname,
                    format!(
                        "#[parent({pname})] refers to variant `{pname}` which doesn't exist in `{enum_name}`",
                    ),
                ));
            }
        }
    }

    // Validate: cycle detection — walk up the parent chain for each variant
    for v in &variant_info {
        let mut visited = std::collections::HashSet::new();
        let mut cursor = v.parent_name.as_ref().map(|n| n.to_string());
        while let Some(ref cur_name) = cursor {
            if !visited.insert(cur_name.clone()) {
                return Err(Error::new_spanned(
                    &v.name,
                    format!("cycle detected in #[parent(...)] chain for `{}`", v.name),
                ));
            }
            let parent_idx = name_to_index[cur_name];
            cursor = variant_info[parent_idx]
                .parent_name
                .as_ref()
                .map(|n| n.to_string());
        }
    }

    // Compute paths for each variant (root-first, ending at self)
    // path = [ancestors...] + [self]
    let paths: Vec<Vec<usize>> = variant_info
        .iter()
        .map(|v| {
            let mut chain = Vec::new();
            chain.push(v.index);
            let mut cursor = v.parent_name.as_ref().map(|n| n.to_string());
            while let Some(ref cur_name) = cursor {
                let parent_idx = name_to_index[cur_name];
                chain.push(parent_idx);
                cursor = variant_info[parent_idx]
                    .parent_name
                    .as_ref()
                    .map(|n| n.to_string());
            }
            chain.reverse(); // root-first
            chain
        })
        .collect();

    // Generate parent() match arms
    let parent_arms: Vec<TokenStream2> = variant_info
        .iter()
        .map(|v| {
            let vname = &v.name;
            match &v.parent_name {
                None => quote! { Self::#vname => ::core::option::Option::None },
                Some(pname) => {
                    quote! { Self::#vname => ::core::option::Option::Some(Self::#pname) }
                }
            }
        })
        .collect();

    // Generate is_leaf() match arms
    let is_leaf_arms: Vec<TokenStream2> = variant_info
        .iter()
        .map(|v| {
            let vname = &v.name;
            let is_leaf = !v.is_composite;
            quote! { Self::#vname => #is_leaf }
        })
        .collect();

    // Generate path() match arms: static arrays + match returning slice refs
    let path_statics: Vec<TokenStream2> = variant_info
        .iter()
        .zip(paths.iter())
        .map(|(v, path)| {
            let vname = &v.name;
            let path_idents: Vec<TokenStream2> = path
                .iter()
                .map(|&idx| {
                    let name = &variant_info[idx].name;
                    quote! { #enum_name::#name }
                })
                .collect();
            let const_name = Ident::new(
                &format!("__PATH_{}", vname.to_string().to_uppercase()),
                Span::call_site(),
            );
            let len = path.len();
            quote! {
                static #const_name: [#enum_name; #len] = [#(#path_idents),*];
            }
        })
        .collect();

    let path_arms: Vec<TokenStream2> = variant_info
        .iter()
        .map(|v| {
            let vname = &v.name;
            let const_name = Ident::new(
                &format!("__PATH_{}", vname.to_string().to_uppercase()),
                Span::call_site(),
            );
            quote! { Self::#vname => &#const_name }
        })
        .collect();

    // Generate as_index() — just cast discriminant to usize
    let as_index_arms: Vec<TokenStream2> = variant_info
        .iter()
        .map(|v| {
            let vname = &v.name;
            let idx = v.index;
            quote! { Self::#vname => #idx }
        })
        .collect();

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let topology_impl = quote! {
        impl #impl_generics ::bloxide_core::topology::StateTopology for #enum_name #ty_generics #where_clause {
            const STATE_COUNT: usize = #state_count;

            #[inline]
            fn parent(self) -> ::core::option::Option<Self> {
                match self {
                    #(#parent_arms,)*
                }
            }

            #[inline]
            fn is_leaf(self) -> bool {
                match self {
                    #(#is_leaf_arms,)*
                }
            }

            fn path(self) -> &'static [Self] {
                #(#path_statics)*
                match self {
                    #(#path_arms,)*
                }
            }

            #[inline]
            fn as_index(self) -> usize {
                match self {
                    #(#as_index_arms,)*
                }
            }
        }
    };

    // If #[handler_fns(FN1, FN2, ...)] was provided, emit a helper macro
    // `{snake_case_enum}_handler_table!(SpecType)` that generates the
    // correctly-ordered HANDLER_TABLE slice.
    //
    // Usage in MachineSpec impl:
    //   const HANDLER_TABLE: &'static [&'static StateFns<Self>] =
    //       ping_state_handler_table!(Self);
    let helper_macro = if let Some(ref fns) = handler_fns {
        let macro_name = to_snake_case_ident(&enum_name.to_string(), Span::call_site());
        let macro_name = Ident::new(&format!("{}_handler_table", macro_name), Span::call_site());
        quote! {
            #[doc(hidden)]
            #[macro_export]
            macro_rules! #macro_name {
                ($ty:ty) => {
                    &[ #( &<$ty>::#fns ),* ]
                };
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #topology_impl
        #helper_macro
    })
}
