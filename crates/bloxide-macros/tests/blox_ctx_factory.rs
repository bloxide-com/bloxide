// Copyright 2025 Bloxide, all rights reserved
//! Tests for factory field auto-detection in #[derive(BloxCtx)].
use bloxide_macros::BloxCtx;

// ── Test 1: Bare fn pointer factory field ────────────────────────────────────
// `foo_factory: fn(ActorId) -> u32` should be auto-detected as a factory
// (FieldRole::Accessor) and generate an accessor trait impl.

/// Accessor trait that the macro should auto-implement.
pub trait HasFooFactory {
    fn foo_factory(&self) -> fn(bloxide_core::messaging::ActorId) -> u32;
}

#[derive(BloxCtx)]
pub struct FactoryCtx {
    pub self_id: bloxide_core::messaging::ActorId,
    pub foo_factory: fn(bloxide_core::messaging::ActorId) -> u32,
}

#[test]
fn fn_pointer_factory_is_constructor_param() {
    // Verify that foo_factory is a constructor parameter (not zero-initialized)
    let f: fn(bloxide_core::messaging::ActorId) -> u32 = |_| 42;
    let ctx = FactoryCtx::new(1usize, f);
    assert_eq!((ctx.foo_factory)(1usize), 42);
}

#[test]
fn fn_pointer_factory_accessor_returns_by_value() {
    // The auto-generated accessor should return by value (fn pointers are Copy)
    let f: fn(bloxide_core::messaging::ActorId) -> u32 = |_| 99;
    let ctx = FactoryCtx::new(1usize, f);
    // If this returns by reference, calling it would require dereferencing.
    // By-value return means we can call directly.
    let retrieved: fn(bloxide_core::messaging::ActorId) -> u32 = ctx.foo_factory();
    assert_eq!(retrieved(1usize), 99);
}

// ── Test 2: Type alias factory field (naming convention fallback) ───────────
// `bar_factory: SomeFnTypeAlias` where the type name doesn't contain fn/Fn/FnMut/FnOnce
// should be detected as FieldRole::Ctor via the `_factory` naming convention.

pub type SomeFnTypeAlias = fn(u32) -> u32;

#[derive(BloxCtx)]
pub struct AliasFactoryCtx {
    pub self_id: bloxide_core::messaging::ActorId,
    pub bar_factory: SomeFnTypeAlias,
}

#[test]
fn type_alias_factory_is_constructor_param() {
    // Verify that bar_factory is a constructor parameter (not zero-initialized)
    let f: SomeFnTypeAlias = |x| x + 1;
    let ctx = AliasFactoryCtx::new(1usize, f);
    assert_eq!((ctx.bar_factory)(5), 6);
}

// ── Test 3: Generic type alias factory field ────────────────────────────────
// Tests that a generic type alias like WorkerSpawnFn<R> is detected as Ctor.
// We can't easily test generics in isolation, but we can test with a concrete alias.

pub type ConcreteSpawnFn = fn(u32) -> u64;

#[derive(BloxCtx)]
pub struct ConcreteFactoryCtx {
    pub self_id: bloxide_core::messaging::ActorId,
    pub spawn_factory: ConcreteSpawnFn,
}

#[test]
fn concrete_type_alias_factory_is_constructor_param() {
    let f: ConcreteSpawnFn = |x| (x as u64) * 2;
    let ctx = ConcreteFactoryCtx::new(1usize, f);
    assert_eq!((ctx.spawn_factory)(21), 42);
}
