// Copyright 2025 Bloxide, all rights reserved
//! Shared string-case utilities for codegen modules.
//!
//! These functions are used by `events.rs`, `topology.rs`, and
//! `spec_skeleton.rs` to convert between PascalCase, snake_case, and
//! UPPER_SNAKE_CASE.

/// Convert PascalCase to snake_case.
pub fn to_snake_case(s: &str) -> String {
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
    out
}

/// Convert PascalCase to UPPER_SNAKE_CASE.
pub fn to_upper_snake_case(s: &str) -> String {
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
        out.push(c.to_ascii_uppercase());
    }
    out
}
