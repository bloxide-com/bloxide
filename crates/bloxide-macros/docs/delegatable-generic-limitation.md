// Copyright 2025 Bloxide, all rights reserved
# Generic Trait Delegation Limitation

## Current Status

The `#[delegatable]` macro now accepts generic traits and generates the delegation macro with the correct trait path (e.g., `HasPeers<WorkerMsg, R>`), but there's a remaining limitation:

**Method signatures in the generated impl still use the trait's generic parameter names instead of the concrete type arguments.**

## The Problem

For a trait like:
```rust
#[delegatable]
pub trait HasPeers<M: Send + 'static, R: BloxRuntime> {
    fn peers(&self) -> &[ActorRef<M, R>];
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>>;
}
```

When invoked with `#[delegates(HasPeers<WorkerMsg, R>)]`, the generated impl would be:
```rust
impl<R, B> HasPeers<WorkerMsg, R> for WorkerCtx<R, B>
where
    B: HasPeers<WorkerMsg, R>,
{
    fn peers(&self) -> &[ActorRef<M, R>] {  // ← ERROR: M is undefined!
        self.behavior.peers()
    }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<M, R>> {  // ← ERROR: M is undefined!
        self.behavior.peers_mut()
    }
}
```

The method signature copies `M` from the trait definition, but `M` doesn't exist in the impl's scope - only the concrete type `WorkerMsg` from `trait_args`.

## The Fix Needed

The `#[delegatable]` macro needs to:
1. Capture the trait's generic parameter names (`M`, `R`)
2. Capture the concrete type arguments from `trait_args` (`WorkerMsg`, `R`)
3. Create a mapping: `M → WorkerMsg`, `R → R`
4. **Substitute the parameter names in method signatures with the concrete types**

This requires transforming the `syn::FnSig` to replace type paths matching the generic params with the concrete types.

## Workaround

For now, generic traits with type parameters that appear in method signatures must use **manual implementations**:

```rust
// Manual impl for generic trait
impl<R: BloxRuntime, B: HasPeers<WorkerMsg, R>> HasPeers<WorkerMsg, R> for WorkerCtx<R, B> {
    fn peers(&self) -> &[ActorRef<WorkerMsg, R>] {
        self.behavior.peers()
    }
    fn peers_mut(&mut self) -> &mut Vec<ActorRef<WorkerMsg, R>> {
        self.behavior.peers_mut()
    }
}
```

## When Manual Impl is NOT Needed

If a generic trait's method signatures **don't reference the generic params** (e.g., only use associated types), then `#[delegates]` works fine:

```rust
// This would work - no M in method signature
#[delegatable]
pub trait SomeTrait<M> {
    type Output;
    fn process(&self) -> Self::Output;  // No M here
}
```

## Non-Generic Traits

Non-generic traits work fully with `#[delegates]`:

```rust
#[delegatable]
pub trait HasCurrentTask {  // No generic params
    fn task_id(&self) -> u32;        // ✓ Works
    fn set_task_id(&mut self, id: u32);  // ✓ Works
}
```
