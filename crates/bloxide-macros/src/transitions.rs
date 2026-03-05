// Copyright 2025 Bloxide, all rights reserved
use proc_macro2::{Delimiter, Ident, TokenStream as TokenStream2, TokenTree};
use quote::quote;

// ── Shorthand pattern detection ───────────────────────────────────────────────

/// How a transition arm's pattern should be interpreted.
///
/// The project convention is:
/// - **message enums** end with `Msg`  → `MsgShorthand`  (access via `.msg_payload()`)
/// - **ctrl enums**    end with `Ctrl` → `CtrlShorthand` (access via `.ctrl_payload()`)
/// - **event enums**   end with `Event`, or wildcard `_` → `FullEvent` (direct pattern match)
#[derive(Copy, Clone)]
enum PatternKind {
    FullEvent,
    MsgShorthand,
    CtrlShorthand,
}

fn classify_pattern(pat: &TokenStream2) -> PatternKind {
    for tt in pat.clone().into_iter() {
        if let TokenTree::Ident(id) = tt {
            let s = id.to_string();
            if s.ends_with("Msg") {
                return PatternKind::MsgShorthand;
            }
            if s.ends_with("Ctrl") {
                return PatternKind::CtrlShorthand;
            }
            return PatternKind::FullEvent;
        }
    }
    PatternKind::FullEvent
}

/// Returns `true` if the pattern contains a `::` path separator, indicating a
/// variant-specific pattern like `PeerCtrl::AddPeer(...)` vs a catch-all `PeerCtrl(...)`.
fn has_path_separator(pat: &TokenStream2) -> bool {
    let tokens: Vec<TokenTree> = pat.clone().into_iter().collect();
    for i in 0..tokens.len().saturating_sub(1) {
        if let (TokenTree::Punct(p1), TokenTree::Punct(p2)) = (&tokens[i], &tokens[i + 1]) {
            if p1.as_char() == ':' && p2.as_char() == ':' {
                return true;
            }
        }
    }
    false
}

/// For a shorthand pattern like `MsgEnum::Variant(binding)` or `MsgEnum::Variant(_)`,
/// extract the inner group contents (the binding pattern, e.g. `binding` or `_`).
fn extract_shorthand_inner(pat: &TokenStream2) -> TokenStream2 {
    let tokens: Vec<TokenTree> = pat.clone().into_iter().collect();
    // Find the last Group with parenthesis delimiter — that's the binding list.
    for tt in tokens.iter().rev() {
        if let TokenTree::Group(g) = tt {
            if g.delimiter() == Delimiter::Parenthesis {
                return g.stream();
            }
        }
    }
    // No group found — wildcard binding
    quote! { _ }
}

/// Parse the comma-separated arms and generate a `&[Rule { ... }]` array.
pub(crate) fn transitions_inner(
    input: proc_macro::TokenStream,
    is_root: bool,
) -> proc_macro::TokenStream {
    let input2 = TokenStream2::from(input);
    match generate_rules(input2, is_root) {
        Ok(ts) => ts.into(),
        Err(msg) => {
            // Emit a compile error
            let msg_lit = proc_macro2::Literal::string(&msg);
            quote! { compile_error!(#msg_lit) }.into()
        }
    }
}

fn generate_rules(input: TokenStream2, is_root: bool) -> Result<TokenStream2, String> {
    let arms = split_arms(input)?;

    let rule_type = quote! { ::bloxide_core::transition::StateRule };

    let rule_tokens: Vec<TokenStream2> = arms
        .into_iter()
        .map(|(pat, body)| generate_rule(&pat, &body, is_root, &rule_type))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(quote! {
        &[ #(#rule_tokens),* ]
    })
}

/// Split the top-level comma-separated `PAT => BODY, PAT => BODY, ...` arms.
/// Each arm is split into (pattern tokens, body tokens).
/// Commas inside delimiters (braces, parens, brackets) are not treated as separators.
fn split_arms(input: TokenStream2) -> Result<Vec<(TokenStream2, TokenStream2)>, String> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut arms = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        // Skip leading commas (trailing commas between arms)
        if let TokenTree::Punct(ref p) = tokens[i] {
            if p.as_char() == ',' {
                i += 1;
                continue;
            }
        }
        if i >= tokens.len() {
            break;
        }

        // Collect pattern tokens up to `=>`.
        //
        // In proc_macro2, all `(...)`, `{...}`, and `[...]` content is wrapped into
        // opaque Group tokens. So `=>` at the top level is always a literal separator
        // and can never appear INSIDE a Group at this level. No depth tracking needed.
        let mut pat_tokens: Vec<TokenTree> = Vec::new();
        loop {
            if i >= tokens.len() {
                return Err("expected `=>` after pattern, but reached end of input".to_string());
            }
            // Detect `=>` as two adjacent Punct tokens: `=` (Joint) followed by `>`
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == '=' && i + 1 < tokens.len() {
                    if let TokenTree::Punct(ref p2) = tokens[i + 1] {
                        if p2.as_char() == '>' {
                            i += 2; // consume `=>`
                            break;
                        }
                    }
                }
            }
            pat_tokens.push(tokens[i].clone());
            i += 1;
        }

        // Collect body tokens up to the next top-level comma (or end).
        //
        // In proc_macro2, all `(...)`, `{...}`, and `[...]` content is wrapped
        // into opaque Group tokens. A `,` inside a Group is not visible at this
        // level, so no depth tracking is needed — the first `,` we see is always
        // the arm separator.
        let mut body_tokens: Vec<TokenTree> = Vec::new();
        loop {
            if i >= tokens.len() {
                break;
            }
            if let TokenTree::Punct(p) = &tokens[i] {
                if p.as_char() == ',' {
                    i += 1; // consume the comma
                    break;
                }
            }
            body_tokens.push(tokens[i].clone());
            i += 1;
        }

        let pat: TokenStream2 = pat_tokens.into_iter().collect();
        let body: TokenStream2 = body_tokens.into_iter().collect();
        if !pat.is_empty() {
            arms.push((pat, body));
        }
    }
    Ok(arms)
}

/// Extract the event tag expression from a pattern.
///
/// Looks for a path like `SomeEvent::VariantName(...)` and returns
/// `Some(TokenStream2 for "SomeEvent::VARIANT_NAME_TAG")`.
/// Returns `None` (wildcard) if the pattern is `_` or unrecognizable.
fn extract_event_tag(pat: &TokenStream2) -> TokenStream2 {
    let tokens: Vec<TokenTree> = pat.clone().into_iter().collect();

    // Try to find a path `A :: B` or `A :: B :: C` at the top level
    // The last `::` segment before `(` or end is the variant name.
    // E.g. `PingEvent::Msg(Envelope { ... })` -> enum=PingEvent, variant=Msg
    // E.g. `TEvent::GoB` -> enum=TEvent, variant=GoB

    let mut path_segments: Vec<Ident> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Ident(ident) => {
                // Check if this is `_` (wildcard)
                if ident == "_" && path_segments.is_empty() {
                    return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
                }
                path_segments.push(ident.clone());
                i += 1;
                // Look for `::`
                if i + 1 < tokens.len() {
                    if let (TokenTree::Punct(p1), TokenTree::Punct(p2)) =
                        (&tokens[i], &tokens[i + 1])
                    {
                        if p1.as_char() == ':' && p2.as_char() == ':' {
                            i += 2; // consume `::`
                            continue;
                        }
                    }
                }
                // End of path
                break;
            }
            TokenTree::Group(_) => break,
            TokenTree::Punct(p) if p.as_char() == '_' => {
                return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
            }
            _ => break,
        }
    }

    if path_segments.len() < 2 {
        // Not a qualified path — use wildcard
        return quote! { ::bloxide_core::event_tag::WILDCARD_TAG };
    }

    // path_segments = [EnumName, ..., VariantName]
    // enum_path = path up to (not including) the last segment
    // variant_name = last segment
    let variant_ident = path_segments.last().unwrap();
    let enum_path: Vec<_> = path_segments[..path_segments.len() - 1].to_vec();

    // Convert variant name to UPPER_SNAKE_CASE
    let upper_snake = crate::event_tag::to_upper_snake_case(&variant_ident.to_string());
    let tag_const = quote::format_ident!("{}_TAG", upper_snake);

    // Build the qualified path: EnumPath::VARIANT_TAG
    let enum_path_ts: TokenStream2 = enum_path
        .iter()
        .enumerate()
        .map(|(i, seg)| {
            let mut ts = quote! { #seg };
            if i + 1 < enum_path.len() {
                ts = quote! { #ts :: };
            }
            ts
        })
        .collect();

    quote! { #enum_path_ts :: #tag_const }
}

/// Parse the body of a rule arm and generate the rule struct literal.
fn generate_rule(
    pat: &TokenStream2,
    body: &TokenStream2,
    is_root: bool,
    rule_type: &TokenStream2,
) -> Result<TokenStream2, String> {
    let kind = classify_pattern(pat);

    // For full-event patterns, use the pattern directly for tag extraction and matches.
    // For Msg/Ctrl shorthand patterns, generate matches via the appropriate payload accessor
    // and use WILDCARD_TAG (matches any event_tag).
    let (event_tag_expr, matches_expr) = match kind {
        PatternKind::FullEvent => {
            let tag = extract_event_tag(pat);
            let matches = quote! { |__ev| ::core::matches!(__ev, #pat) };
            (tag, matches)
        }
        PatternKind::MsgShorthand => {
            // Strip bindings for the `matches` predicate to avoid "cannot move out of" errors.
            let pat_no_bindings = strip_bindings_from_shorthand(pat);
            let matches = quote! {
                |__ev| __ev.msg_payload().map_or(false, |__m| ::core::matches!(__m, #pat_no_bindings))
            };
            (quote! { ::bloxide_core::event_tag::WILDCARD_TAG }, matches)
        }
        PatternKind::CtrlShorthand => {
            // `PeerCtrl(_)` (no `::`) — catch-all: any ctrl payload.
            // `PeerCtrl::AddPeer(_)` (has `::`) — match a specific variant.
            let matches = if has_path_separator(pat) {
                let pat_no_bindings = strip_bindings_from_shorthand(pat);
                quote! {
                    |__ev| __ev.ctrl_payload().map_or(false, |__m| ::core::matches!(__m, #pat_no_bindings))
                }
            } else {
                quote! { |__ev| __ev.ctrl_payload().is_some() }
            };
            (quote! { ::bloxide_core::event_tag::WILDCARD_TAG }, matches)
        }
    };

    let body_tokens: Vec<TokenTree> = body.clone().into_iter().collect();

    // Detect body form:
    // 1. `stay` (single ident)
    // 2. `reset` (single ident)
    // 3. `{ transition STATE }` (brace group starting with `transition`)
    // 4. `{ actions [...] stay }` or `{ actions [...] reset }` or
    //    `{ actions [...] transition STATE }`  or
    //    `{ actions [...] guard(ctx, res) { ... } }`

    // Check for simple keyword body
    if body_tokens.len() == 1 {
        if let TokenTree::Ident(ref kw) = body_tokens[0] {
            if kw == "stay" {
                return Ok(quote! {
                    #rule_type {
                        event_tag: #event_tag_expr,
                        matches: #matches_expr,
                        actions: &[],
                        guard: |_, _, _| ::bloxide_core::transition::Guard::Stay,
                    }
                });
            }
            if kw == "reset" {
                return Ok(quote! {
                    #rule_type {
                        event_tag: #event_tag_expr,
                        matches: #matches_expr,
                        actions: &[],
                        guard: |_, _, _| ::bloxide_core::transition::Guard::Reset,
                    }
                });
            }
            // Single-token body that isn't a keyword: treat as a state expression.
            // e.g. `PingEvent::Foo => SomeState::Bar` (no braces, no keyword).
            if !is_root {
                let state = kw.clone();
                return Ok(quote! {
                    #rule_type {
                        event_tag: #event_tag_expr,
                        matches: #matches_expr,
                        actions: &[],
                        guard: |_, _, _| ::bloxide_core::transition::Guard::Transition(
                            ::bloxide_core::topology::LeafState::new(#state)
                        ),
                    }
                });
            }
        }
    }

    // Check for brace-block body
    if body_tokens.len() == 1 {
        if let TokenTree::Group(ref g) = body_tokens[0] {
            if g.delimiter() == Delimiter::Brace {
                return parse_brace_body(
                    g.stream(),
                    pat,
                    &event_tag_expr,
                    &matches_expr,
                    is_root,
                    kind,
                    rule_type,
                );
            }
        }
    }

    Err(format!("transitions!: unrecognized arm body: `{body}`"))
}

/// For a shorthand pattern like `PingPongMsg::Pong(binding)`, strip the binding
/// names and replace with `_` to produce a pattern safe for `matches!`.
/// E.g. `PingPongMsg::Pong(pong)` → `PingPongMsg::Pong(_)`.
fn strip_bindings_from_shorthand(pat: &TokenStream2) -> TokenStream2 {
    let tokens: Vec<TokenTree> = pat.clone().into_iter().collect();
    let mut out = Vec::new();
    for tt in tokens {
        match tt {
            TokenTree::Group(g) if g.delimiter() == Delimiter::Parenthesis => {
                // Replace the entire group contents with `_`
                let new_stream = quote! { _ };
                let new_group = proc_macro2::Group::new(Delimiter::Parenthesis, new_stream);
                out.push(TokenTree::Group(new_group));
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// Parse the contents of a `{ ... }` body:
/// - `transition STATE`
/// - `actions [...] stay`
/// - `actions [...] reset`
/// - `actions [...] transition STATE`
/// - `actions [...] guard(ctx, res) { ... }`
fn parse_brace_body(
    body: TokenStream2,
    pat: &TokenStream2,
    event_tag_expr: &TokenStream2,
    matches_expr: &TokenStream2,
    is_root: bool,
    kind: PatternKind,
    rule_type: &TokenStream2,
) -> Result<TokenStream2, String> {
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    let mut i = 0;

    // Consume optional leading `actions [...]`
    let mut actions_ts = quote! { &[] };
    if i < tokens.len() {
        if let TokenTree::Ident(ref kw) = tokens[i] {
            if kw == "actions" {
                i += 1;
                // Expect a bracket group `[fn1, fn2, ...]`
                if i >= tokens.len() {
                    return Err("transitions!: expected `[...]` after `actions`".to_string());
                }
                if let TokenTree::Group(ref g) = tokens[i] {
                    if g.delimiter() == Delimiter::Bracket {
                        let inner = g.stream();
                        actions_ts = quote! { &[#inner] };
                        i += 1;
                    } else {
                        return Err("transitions!: expected `[...]` after `actions`".to_string());
                    }
                } else {
                    return Err("transitions!: expected `[...]` after `actions`".to_string());
                }
            }
        }
    }

    // Next keyword determines the guard
    if i >= tokens.len() {
        return Err("transitions!: unexpected end of arm body".to_string());
    }

    match &tokens[i].clone() {
        TokenTree::Ident(kw) if kw == "stay" => Ok(quote! {
            #rule_type {
                event_tag: #event_tag_expr,
                matches: #matches_expr,
                actions: #actions_ts,
                guard: |_, _, _| ::bloxide_core::transition::Guard::Stay,
            }
        }),
        TokenTree::Ident(kw) if kw == "reset" => Ok(quote! {
            #rule_type {
                event_tag: #event_tag_expr,
                matches: #matches_expr,
                actions: #actions_ts,
                guard: |_, _, _| ::bloxide_core::transition::Guard::Reset,
            }
        }),
        TokenTree::Ident(kw) if kw == "transition" => {
            i += 1;
            let state_tokens: TokenStream2 = tokens[i..].iter().cloned().collect();
            Ok(quote! {
                #rule_type {
                    event_tag: #event_tag_expr,
                    matches: #matches_expr,
                    actions: #actions_ts,
                    guard: |_, _, _| ::bloxide_core::transition::Guard::Transition(
                        ::bloxide_core::topology::LeafState::new(#state_tokens)
                    ),
                }
            })
        }
        TokenTree::Ident(kw) if kw == "guard" => {
            // Parse `guard(ctx, results) { ... }`
            i += 1;
            // Expect `(ctx, results)` group
            if i >= tokens.len() {
                return Err("transitions!: expected `(ctx, results)` after `guard`".to_string());
            }
            let (ctx_ident, results_ident) = if let TokenTree::Group(ref g) = tokens[i] {
                if g.delimiter() == Delimiter::Parenthesis {
                    let args: Vec<TokenTree> = g.stream().into_iter().collect();
                    // Expect: IDENT , IDENT
                    let ctx = extract_ident_from_arg_list(&args, 0)?;
                    let res = extract_ident_from_arg_list(&args, 1)?;
                    i += 1;
                    (ctx, res)
                } else {
                    return Err("transitions!: expected `(ctx, results)` after `guard`".to_string());
                }
            } else {
                return Err("transitions!: expected `(ctx, results)` after `guard`".to_string());
            };

            // Expect `{ ... }` guard body
            if i >= tokens.len() {
                return Err("transitions!: expected `{ ... }` guard chain body".to_string());
            }
            let guard_body = if let TokenTree::Group(ref g) = tokens[i] {
                if g.delimiter() == Delimiter::Brace {
                    g.stream()
                } else {
                    return Err("transitions!: expected `{ ... }` guard chain body".to_string());
                }
            } else {
                return Err("transitions!: expected `{ ... }` guard chain body".to_string());
            };

            let guard_chain = generate_guard_chain(guard_body, is_root)?;

            // For shorthand patterns, wrap the guard body in a binding extraction
            // so the binding (e.g. `pong` from `PingPongMsg::Pong(pong)`) is in scope.
            let guard_body_ts = match kind {
                PatternKind::FullEvent => {
                    // Full-event: binding comes from pattern match on the event parameter.
                    // Guard param `_` discards the event.
                    quote! { |#ctx_ident, #results_ident, _| { #guard_chain } }
                }
                PatternKind::MsgShorthand => {
                    // Shorthand: extract binding from msg_payload() so it's available
                    // as a named variable in the guard chain body.
                    // msg_payload() returns Option<&T>, so the Some binding is already &T.
                    // Do NOT use `ref` here — that would produce &&T.
                    let inner_binding = extract_shorthand_inner(pat);
                    quote! {
                        |#ctx_ident, #results_ident, __ev| {
                            if let ::core::option::Option::Some(#inner_binding) = __ev.msg_payload() {
                                #guard_chain
                            } else {
                                unreachable!("guard called on non-matching event")
                            }
                        }
                    }
                }
                PatternKind::CtrlShorthand => {
                    // Ctrl shorthand: extract binding from ctrl_payload().
                    // ctrl_payload() returns Option<&T>, so the Some binding is already &T.
                    // Do NOT use `ref` here — that would produce &&T.
                    let inner_binding = extract_shorthand_inner(pat);
                    quote! {
                        |#ctx_ident, #results_ident, __ev| {
                            if let ::core::option::Option::Some(#inner_binding) = __ev.ctrl_payload() {
                                #guard_chain
                            } else {
                                unreachable!("guard called on non-matching event")
                            }
                        }
                    }
                }
            };

            Ok(quote! {
                #rule_type {
                    event_tag: #event_tag_expr,
                    matches: #matches_expr,
                    actions: #actions_ts,
                    guard: #guard_body_ts,
                }
            })
        }
        other => Err(format!(
            "transitions!: unexpected token in arm body: `{other}`"
        )),
    }
}

/// Extract the N-th comma-separated ident from a flat token stream.
fn extract_ident_from_arg_list(args: &[TokenTree], n: usize) -> Result<proc_macro2::Ident, String> {
    let idents: Vec<proc_macro2::Ident> = args
        .iter()
        .filter_map(|t| {
            if let TokenTree::Ident(id) = t {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    idents
        .get(n)
        .cloned()
        .ok_or_else(|| format!("transitions!: expected ident at position {n} in guard args"))
}

/// Generate an if/else chain from guard body tokens:
///
/// ```text
/// COND => TARGET,
/// COND => TARGET,
/// _ => TARGET,
/// ```
fn generate_guard_chain(body: TokenStream2, is_root: bool) -> Result<TokenStream2, String> {
    let arms = split_guard_arms(body)?;
    if arms.is_empty() {
        return Err("transitions!: empty guard chain".to_string());
    }
    if let Some((is_wildcard, _, _)) = arms.last() {
        if !is_wildcard {
            return Err(
                "transitions!: guard chain must end with a wildcard (_ => ...) arm".to_string(),
            );
        }
    }

    // Generate if/else chain — last arm must be wildcard `_`
    let mut result = TokenStream2::new();

    for (is_wildcard, cond, target_tokens) in arms.into_iter() {
        let target = build_guard_target(&target_tokens, is_root);
        if is_wildcard {
            // Wildcard is always the final arm — emit it as `else { target }` so the
            // entire if/else-if/else chain is a single expression of type Guard<S>.
            if result.is_empty() {
                result = quote! { #target };
            } else {
                result = quote! { #result else { #target } };
            }
            break;
        } else if result.is_empty() {
            result = quote! { if #cond { #target } };
        } else {
            result = quote! { #result else if #cond { #target } };
        }
    }

    Ok(result)
}

/// Split guard chain tokens into arms: `(is_wildcard, condition, target_tokens)`.
fn split_guard_arms(
    body: TokenStream2,
) -> Result<Vec<(bool, TokenStream2, Vec<TokenTree>)>, String> {
    let tokens: Vec<TokenTree> = body.into_iter().collect();
    let mut arms = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        // Skip commas
        if let TokenTree::Punct(ref p) = tokens[i] {
            if p.as_char() == ',' {
                i += 1;
                continue;
            }
        }
        if i >= tokens.len() {
            break;
        }

        // Collect condition up to `=>`
        let mut cond_tokens: Vec<TokenTree> = Vec::new();
        let mut is_wildcard = false;
        loop {
            if i >= tokens.len() {
                return Err("transitions!: expected `=>` in guard chain".to_string());
            }
            match &tokens[i] {
                TokenTree::Punct(p) if p.as_char() == '=' => {
                    if i + 1 < tokens.len() {
                        if let TokenTree::Punct(ref p2) = tokens[i + 1] {
                            if p2.as_char() == '>' {
                                i += 2;
                                break;
                            }
                        }
                    }
                    cond_tokens.push(tokens[i].clone());
                    i += 1;
                }
                TokenTree::Ident(id) if id == "_" && cond_tokens.is_empty() => {
                    is_wildcard = true;
                    i += 1;
                    // consume `=>`
                    if i + 1 < tokens.len() {
                        if let (TokenTree::Punct(p1), TokenTree::Punct(p2)) =
                            (&tokens[i], &tokens[i + 1])
                        {
                            if p1.as_char() == '=' && p2.as_char() == '>' {
                                i += 2;
                            }
                        }
                    }
                    break;
                }
                _ => {
                    cond_tokens.push(tokens[i].clone());
                    i += 1;
                }
            }
        }

        // Collect target tokens up to next top-level comma
        let mut target_tokens: Vec<TokenTree> = Vec::new();
        let mut depth = 0usize;
        loop {
            if i >= tokens.len() {
                break;
            }
            match &tokens[i] {
                TokenTree::Group(_) => {
                    depth += 1;
                    target_tokens.push(tokens[i].clone());
                    i += 1;
                }
                TokenTree::Punct(p) if p.as_char() == ',' && depth == 0 => {
                    i += 1;
                    break;
                }
                _ => {
                    target_tokens.push(tokens[i].clone());
                    i += 1;
                }
            }
        }

        let cond: TokenStream2 = cond_tokens.into_iter().collect();
        arms.push((is_wildcard, cond, target_tokens));
    }
    Ok(arms)
}

fn build_guard_target(tokens: &[TokenTree], _is_root: bool) -> TokenStream2 {
    if tokens.len() == 1 {
        if let TokenTree::Ident(ref kw) = tokens[0] {
            if kw == "stay" {
                return quote! { ::bloxide_core::transition::Guard::Stay };
            }
            if kw == "reset" {
                return quote! { ::bloxide_core::transition::Guard::Reset };
            }
        }
    }
    let state: TokenStream2 = tokens.iter().cloned().collect();
    quote! {
        ::bloxide_core::transition::Guard::Transition(
            ::bloxide_core::topology::LeafState::new(#state)
        )
    }
}
