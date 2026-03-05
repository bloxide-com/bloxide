// Copyright 2025 Bloxide, all rights reserved
use bloxide_macros::delegatable;

// ── Trait with associated type + multiple methods ────────────────────────────

#[delegatable]
pub trait CountsRounds {
    type Round: Copy
        + PartialEq
        + PartialOrd
        + core::ops::Add<Output = Self::Round>
        + From<u8>
        + core::fmt::Display;
    fn round(&self) -> Self::Round;
    fn set_round(&mut self, round: Self::Round);
}

// ── Concrete inner type that implements the trait ─────────────────────────────

struct Inner {
    round: u32,
}

impl CountsRounds for Inner {
    type Round = u32;
    fn round(&self) -> u32 {
        self.round
    }
    fn set_round(&mut self, round: u32) {
        self.round = round;
    }
}

// ── Outer wrapper that delegates via the generated macro ─────────────────────

struct Wrapper {
    inner: Inner,
}

__delegate_CountsRounds! {
    struct_name: Wrapper,
    field: inner,
    field_type: Inner,
    impl_generics: {},
    ty_generics: {},
    where_clause: {}
}

#[test]
fn forwarding_round_read() {
    let w = Wrapper {
        inner: Inner { round: 5 },
    };
    assert_eq!(w.round(), 5u32);
}

#[test]
fn forwarding_round_write() {
    let mut w = Wrapper {
        inner: Inner { round: 0 },
    };
    w.set_round(42);
    assert_eq!(w.round(), 42);
}

// ── Trait without associated types (methods-only) ────────────────────────────

#[delegatable]
pub trait TracksName {
    fn name(&self) -> &str;
}

struct NamedInner;

impl TracksName for NamedInner {
    fn name(&self) -> &str {
        "hello"
    }
}

struct NamedWrapper {
    inner: NamedInner,
}

__delegate_TracksName! {
    struct_name: NamedWrapper,
    field: inner,
    field_type: NamedInner,
    impl_generics: {},
    ty_generics: {},
    where_clause: {}
}

#[test]
fn forwarding_name() {
    let w = NamedWrapper { inner: NamedInner };
    assert_eq!(w.name(), "hello");
}

// ── Generic wrapper struct (non-generic trait) ───────────────────────────────

struct GenericWrapper<T> {
    inner: T,
}

__delegate_CountsRounds! {
    struct_name: GenericWrapper,
    field: inner,
    field_type: T,
    impl_generics: { <T> },
    ty_generics: { <T> },
    where_clause: {}
}

#[test]
fn forwarding_generic_wrapper() {
    let mut w = GenericWrapper {
        inner: Inner { round: 10 },
    };
    assert_eq!(w.round(), 10);
    w.set_round(99);
    assert_eq!(w.round(), 99);
}
