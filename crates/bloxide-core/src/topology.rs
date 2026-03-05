/// Describes the parent-child relationship of a state enum and provides
/// precomputed root-first ancestry paths.
///
/// Implement this trait via `#[derive(StateTopology)]` from `bloxide-macros`.
/// The derive requires:
/// - `#[repr(u8)]` on the enum (so `as usize` gives the discriminant index)
/// - `#[composite]` attribute on composite (non-leaf) states
/// - `#[parent(ParentVariant)]` attribute on each non-top-level state
///
/// # Invariants
///
/// - `parent()` forms a forest (no cycles). Every chain of `parent()` calls
///   must terminate at `None`.
/// - `path(self)` is root-first and ends with `self`.
/// - `as_index(self)` returns the `#[repr(u8)]` discriminant cast to `usize`,
///   suitable for indexing `HANDLER_TABLE`.
/// - `STATE_COUNT` equals the total number of variants in the enum.
pub trait StateTopology: Copy + Eq + core::fmt::Debug + Send + 'static {
    /// Total number of states (variants) in the enum. Equals `HANDLER_TABLE.len()`.
    const STATE_COUNT: usize;

    /// Returns the parent of this state, or `None` for top-level states.
    fn parent(self) -> Option<Self>;

    /// Returns `true` if this state has no children (is a leaf).
    fn is_leaf(self) -> bool;

    /// Returns the root-first ancestry path for this state, ending at `self`.
    ///
    /// For a leaf state `C` with parent `B` and grandparent `A` (top-level):
    /// `path()` returns `&[A, B, C]`.
    /// For a top-level state `A`: `path()` returns `&[A]`.
    fn path(self) -> &'static [Self];

    /// Returns the `#[repr(u8)]` discriminant of this state as a `usize`,
    /// suitable for indexing into `HANDLER_TABLE`.
    fn as_index(self) -> usize;
}

// ── LeafState newtype ─────────────────────────────────────────────────────────

/// A newtype wrapper that can only be constructed for **leaf** states.
///
/// `Guard::Transition` takes `LeafState<S::State>`
/// instead of `S::State`, turning attempts to transition to a composite state
/// into a compile-time or debug-time error rather than silent UB.
///
/// The `transitions!` proc macro auto-wraps state targets in `LeafState::new`,
/// so user-facing transition syntax is unchanged.
///
/// # Construction
///
/// - `LeafState::new(state)` — asserts `state.is_leaf()` in debug builds.
///   In release builds the assertion is elided but the wrapper still provides
///   a type-level guarantee when the macro generates it.
/// - `LeafState::new_unchecked(state)` — bypasses the assertion.
///   Use only when you have proven the state is a leaf via other means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeafState<S: StateTopology>(S);

impl<S: StateTopology> LeafState<S> {
    /// Wrap `state` as a `LeafState`. Asserts in debug builds that `state.is_leaf()`.
    #[inline]
    pub fn new(state: S) -> Self {
        debug_assert!(
            state.is_leaf(),
            "transition target {:?} is not a leaf state — use only leaf states as targets",
            state
        );
        Self(state)
    }

    /// Wrap `state` without checking `is_leaf()`.
    ///
    /// Only for use by the `transitions!` proc macro after validating the leaf
    /// invariant at code-gen time. Not part of the public API.
    #[doc(hidden)]
    #[inline]
    pub fn new_unchecked(state: S) -> Self {
        Self(state)
    }

    /// Unwrap the inner state value.
    #[inline]
    pub fn into_inner(self) -> S {
        self.0
    }

    /// Read the inner state value without consuming the wrapper.
    #[inline]
    pub fn get(&self) -> S {
        self.0
    }
}
