# Bloxide Codebase Fix Plan

## Decisions

| # | Question | Decision |
|---|----------|----------|
| 1 | Spawn crate: delete orphaned code or complete redesign? | Complete the redesign; delete all orphaned code that doesn't fit |
| 2 | blox_messages Copy derive: opt-in or opt-out? | Opt-in via `#[blox_messages(copy)]`; default to `Debug, Clone` only. Best for no_std/microcontrollers — avoids requiring Copy on messages with heap types |
| 3 | Mailboxes::poll_next signature: breaking change? | Yes — change to `Poll<Option<E>>`. Cleaner, correct, breaking is fine |
| 4 | Fix codegen now or defer? | Fix now |
| 5 | Visualizer in scope? | Out of scope — skip all visualizer fixes |
| 6 | OTP restart strategies: implement now? | Implement now |
| 7 | History states / event deferral: defer? | Defer to future tasks |

No backwards compatibility is required for any fix.

---

## Phase 1: Critical Bugs (must fix before any release)

### Fix 1: Factory auto-detection broken in BloxCtx macro

**Problem:** `is_fn_type` in `crates/bloxide-macros/src/blox_ctx/analyze.rs:260` checks `Type::Path` for `fn`/`Fn`/`FnMut`/`FnOnce`, but bare `fn(...)` pointer types parse as `Type::BareFn` and never match. Factory fields silently fall through to `FieldRole::State` and get zero-initialized via `Default::default()`, producing non-functional factories. The Pool blox works around this with the deprecated `#[ctor]` annotation.

**Files to modify:**
- `crates/bloxide-macros/src/blox_ctx/analyze.rs`
  - Fix `is_fn_type` (line 260) to also match `Type::BareFn`:
    ```rust
    fn is_fn_type(ty: &Type) -> bool {
        match ty {
            Type::BareFn(_) => true,
            Type::Path(TypePath { path, .. }) => {
                path.segments.iter().any(|s| {
                    matches!(s.ident.to_string().as_str(), "fn" | "Fn" | "FnMut" | "FnOnce")
                })
            }
            _ => false,
        }
    }
    ```
  - Add naming-convention fallback in the field classification logic: if field name ends with `_factory`, treat as `FieldRole::Ctor` regardless of type (handles type aliases like `WorkerSpawnFn<R>`).

- `crates/bloxide-macros/src/blox_ctx/generate.rs:148-167`
  - Fix `generate_accessor_impl` to return by value for `Copy` types (fn pointers, `ActorId`, etc.) instead of always returning `&FieldType`.
  - When field role is `Ctor` (factory), generate `fn field(&self) -> FieldType` (by value) instead of `fn field(&self) -> &FieldType`.
  - Detect `Copy` by checking if the field type is `BareFn` or if the role is `Ctor` with a `_factory` naming convention.

- `crates/bloxes/pool/src/generated/ctx.rs`
  - Remove `#[ctor]` annotations from `self_ref` and `worker_factory`.
  - For `self_ref`: keep `#[ctor]` only if we want to suppress `HasSelfRef` auto-generation. **Decision:** Remove `#[ctor]` from `self_ref` and let auto-detection generate `HasSelfRef` — the Pool doesn't need it but it's harmless. Remove `#[ctor]` from `worker_factory` and rely on fixed auto-detection.
  - Remove manual `impl HasWorkerFactory` block (lines 39-63) once auto-generation works correctly.

**Tests to add:**
- `crates/bloxide-macros/tests/blox_ctx_factory.rs`:
  - Test: context struct with `foo_factory: fn(ActorId) -> u32` field — verify generated constructor accepts the fn pointer as a parameter (not zero-initialized).
  - Test: context struct with `bar_factory: SomeFnTypeAlias<R>` field where field name ends in `_factory` — verify auto-detection via naming convention.
  - Test: generated accessor returns by value for factory fields, not by reference.

**Verification:** `cargo check -p bloxide-macros && cargo test -p bloxide-macros && cargo check -p pool-blox`

---

### Fix 2: Dynamic supervised children cannot be killed

**Problem:** `runtimes/bloxide-tokio/src/supervision.rs:237-251` — `spawn_dynamic_supervised_child` calls `tokio::spawn(...)` but discards the `JoinHandle`. The handle is never registered with `TokioKillCap`. When the supervisor calls `kill_cap.kill(child_id)` for a dynamic child with `ChildPolicy::Kill`, it silently does nothing.

**Files to modify:**
- `runtimes/bloxide-tokio/src/supervision.rs:225-252`
  - Add `kill_cap: &TokioKillCap` parameter to `spawn_dynamic_supervised_child`.
  - Capture the `JoinHandle` from `tokio::spawn(...)`:
    ```rust
    let handle = tokio::spawn(task_builder(lifecycle_rx, notify, child_id));
    kill_cap.register(child_id, handle);
    ```
  - Update function signature and doc comment.

- `runtimes/bloxide-tokio/src/lib.rs:172-185`
  - Update `spawn_child_dynamic!` macro to pass `$builder.kill_cap()` to `spawn_dynamic_supervised_child`.

- `examples/tokio-demo.rs` (around line 751-758)
  - Update any call sites that use `spawn_child_dynamic!` to ensure the builder has `KillCap`.

**Tests to add:**
- `runtimes/bloxide-tokio/src/supervision.rs` (test module):
  - `spawn_dynamic_child_then_kill_aborts_task`: spawn a dynamic child, call `kill_cap.kill(child_id)`, poll `JoinHandle::is_finished()` after a brief sleep, assert true.
  - `spawn_dynamic_child_registers_with_kill_cap`: spawn a dynamic child, verify `kill_cap` contains the child ID.

**Verification:** `cargo test -p bloxide-tokio && cargo check --example tokio-demo`

---

### Fix 3: Complete bloxide-spawn redesign and delete orphaned code

**Problem:** `factory.rs` (98 lines), `output.rs` (72 lines), and `tests/` (161 lines) are not declared as modules in `lib.rs`. They reference non-existent types. AGENTS.md advertises `PeerCtrl`, `HasPeers`, `introduce_peers` in this crate but none exist. The spec at `spec/plans/extensible-child-factory.md` and `spec/architecture/11-dynamic-actors.md` describe the intended design.

**Approach:** Complete the redesign by implementing the planned types, then delete any orphaned code that doesn't fit the final design.

**Step 1: Read and understand the planned design.**
- Read `spec/plans/extensible-child-factory.md` for the factory redesign vision.
- Read `spec/architecture/11-dynamic-actors.md` for `introduce_peers` and peer management.
- Read `spec/architecture/16-spawn-service.md` for the spawn service pattern.
- Read `spec/architecture/17-spawn-cap-design.md` for SpawnCap design.

**Step 2: Implement the following types in `crates/bloxide-spawn/src/`:**

- `capability.rs` — Already has `SpawnCap`. Keep as-is.

- `factory.rs` — Rewrite to implement the extensible factory pattern:
  - `SpawnFactory` trait: factory trait for creating actor tasks with runtime-agnostic parameters.
  - `SpawnFactoryFor<M, R>`: typed factory wrapper for spawning actors of message type `M` on runtime `R`.
  - `ErasedSpawnFactory<R>`: type-erased factory for supervisor-owned spawning (stores `Box<dyn FnOnce(...)>`).
  - Ensure no dependency on `bloxide-supervisor` (spawn is lower-level than supervision).
  - All types must be `no_std` compatible (use `alloc` for `Box`).

- `output.rs` — Rewrite to implement spawn output tracking:
  - `SpawnOutput`: tracks the result of a spawn operation (actor ID, channel refs).
  - Remove dependency on `bloxide-supervisor::registry::ChildPolicy` — instead define a local `SpawnPolicy` or accept policy as a generic parameter.
  - If `ChildPolicy` is needed, move it to a shared location or duplicate the relevant enum.

- `peer.rs` (new file) — Implement peer management:
  - `PeerCtrl`: control message type for peer management (AddPeer, RemovePeer, ListPeers).
  - `HasPeers`: trait for accessing and mutating a peer list (already defined in `crates/actions/pool-actions/src/traits.rs` — move it here or re-export from spawn).
  - `introduce_peers`: generic function that sends `AddPeer` messages to mutually introduce two actors.

- `lib.rs` — Declare all new modules:
  ```rust
  pub mod capability;
  pub mod factory;
  pub mod output;
  pub mod peer;
  pub mod prelude;
  #[cfg(feature = "std")]
  pub mod test_impl;
  ```

- `tests/` — Rewrite tests to use the new types. Fix all broken imports:
  - Remove imports of non-existent types (`DynamicActorSupport`, `ChildType`, `SpawnParams`, `SpawnReplyTo`).
  - Replace with imports from the new `factory`, `output`, `peer` modules.
  - Fix `test_impl` imports to use actual functions (`drain_spawned`, `spawned_count`).

- `Cargo.toml` — Add `bloxide-core` dependency (already present). Do NOT add `bloxide-supervisor` dependency (spawn is lower-level).

**Step 3: Update dependent crates.**
- `crates/actions/pool-actions/src/traits.rs` — If `HasPeers` is moved to `bloxide-spawn`, re-export or update imports.
- `crates/bloxes/worker/src/` — Update if worker references `PeerCtrl` or `introduce_peers` from spawn.
- `AGENTS.md:28` — Update description to accurately reflect what bloxide-spawn contains.

**Tests to add:**
- `crates/bloxide-spawn/src/tests/spawn_test.rs`:
  - Test `SpawnFactory` creates actors with correct channel refs.
  - Test `introduce_peers` sends `AddPeer` to both actors.
  - Test `PeerCtrl::AddPeer` / `RemovePeer` peer list management.
  - Test `SpawnOutput` tracks actor ID and refs correctly.

**Verification:** `cargo check -p bloxide-spawn && cargo test -p bloxide-spawn && cargo check -p pool-actions && cargo check -p worker-blox`

---

### Fix 4: TestReceiver never registers waker

**Problem:** `crates/bloxide-core/src/test_utils.rs:74-84` — `poll_next` returns `Poll::Pending` without storing the waker. `send_via` doesn't wake any waiting task. If `TestRuntime` is used with any async event loop, the actor hangs forever once the queue drains.

**Files to modify:**
- `crates/bloxide-core/src/test_utils.rs`:

  - Add a `waker` field to `TestReceiver` and `TestSender`:
    ```rust
    pub struct TestSender<M: Send + 'static> {
        queue: Arc<Mutex<VecDeque<Envelope<M>>>>,
        full: Arc<AtomicBool>,
        waker: Arc<Mutex<Option<Waker>>>,
    }

    pub struct TestReceiver<M: Send + 'static> {
        queue: Arc<Mutex<VecDeque<Envelope<M>>>>,
        full: Arc<AtomicBool>,
        waker: Arc<Mutex<Option<Waker>>>,
    }
    ```

  - Update `TestRuntime::channel` to create shared `waker` Arc and pass to both sender and receiver.

  - In `TestReceiver::poll_next`:
    ```rust
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut lock = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        match lock.pop_front() {
            Some(env) => Poll::Ready(Some(env)),
            None => {
                *self.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
    ```

  - In `TestRuntime::send_via` and `try_send_via`, after pushing to the queue, wake the receiver:
    ```rust
    sender.queue.lock().unwrap_or_else(|e| e.into_inner()).push_back(envelope);
    if let Some(waker) = sender.waker.lock().unwrap_or_else(|e| e.into_inner()).take() {
        waker.wake();
    }
    ```

  - Remove redundant `unsafe impl Send/Sync` for `TestSender` (line 55-56) — auto-derived from `Arc<Mutex<...>>` and `Arc<AtomicBool>`.
  - Remove redundant `impl Unpin for TestReceiver` (line 86) — auto-derived from `Arc` contents.
  - Change all `.lock().unwrap()` to `.lock().unwrap_or_else(|e| e.into_inner())` for poison resilience.

**Tests to add:**
- `crates/bloxide-core/src/test_utils.rs` (test module) or `src/tests/`:
  - `test_runtime_async_wakeup`: Create a `TestRuntime` channel, use `block_on` to start `run_actor` with a simple machine, send a message after the actor is waiting, verify the actor processes it (proving waker mechanism works).
  - `test_runtime_waker_idempotent`: Send multiple messages while actor is waiting, verify all are processed in order.

**Verification:** `cargo test -p bloxide-core`

---

## Phase 2: High Severity Bugs

### Fix 5: `fail` keyword missing from simple transition body

**Problem:** `crates/bloxide-macros/src/transitions.rs:334-371` — The simple body handler checks for `stay` and `reset` as bare tokens but not `fail`. Writing `=> fail` (without braces) treats `fail` as a state name.

**Files to modify:**
- `crates/bloxide-macros/src/transitions.rs:345-355` — Add `fail` check after `reset`:
  ```rust
  if kw == "fail" {
      return Ok(quote! {
          #rule_type {
              event_tag: #event_tag_expr,
              matches: #matches_expr,
              actions: &[],
              guard: |_, _, _| ::bloxide_core::transition::Guard::Fail,
          }
      });
  }
  ```

**Tests to add:**
- `crates/bloxide-macros/tests/transitions.rs` (new file):
  - Test `=> fail` in non-root `transitions!` generates `Guard::Fail`.
  - Test `=> fail` in `root_transitions!` generates `Guard::Fail`.
  - Test `=> stay` and `=> reset` still work (regression).

**Verification:** `cargo test -p bloxide-macros`

---

### Fix 6: `blox_messages!` unconditionally derives Copy

**Problem:** `crates/bloxide-macros/src/blox_messages.rs:111-128` — All generated structs unconditionally derive `#[derive(Debug, Clone, Copy)]`. Messages containing non-`Copy` types (like `Vec<u8>`, `String`) won't compile.

**Decision:** Make `Copy` opt-in via `#[blox_messages(copy)]`. Default to `Debug, Clone` only. This is best for no_std/microcontrollers — avoids requiring Copy on messages that may contain heap-allocated data, while allowing users to opt into Copy for small primitive messages.

**Files to modify:**
- `crates/bloxide-macros/src/blox_messages.rs`:
  - Parse `#[blox_messages(copy)]` attribute from the input enum's attributes.
  - When `copy` is present: derive `#[derive(Debug, Clone, Copy)]`.
  - When `copy` is absent: derive `#[derive(Debug, Clone)]`.
  - Update the doc comment to reflect the new behavior.
  - Update the unit test `test_blox_messages_with_multiple_fields` to use `#[blox_messages(copy)]` for the `Vec<u8>` test case (and verify the non-copy case generates code without `Copy`).

- All existing `blox_messages!` invocations in message crates — add `#[blox_messages(copy)]` where the messages are currently Copy (all current message types are primitive/Copy):
  - `crates/messages/ping-pong-messages/src/generated/messages_pingpongmsg.rs` (if generated via macro) or the source `blox_messages!` invocation.
  - `crates/messages/pool-messages/src/generated/messages_poolmsg.rs` — same.
  - `crates/messages/pool-messages/src/generated/messages_workermsg.rs` — same.
  - `crates/messages/counter-messages/src/generated/messages_countermsg.rs` — same.
  - `crates/messages/bhsm-tst-messages/src/generated/` — same.
  - Search for all `blox_messages!` invocations across the codebase and add `(copy)` where appropriate.

**Tests to add:**
- `crates/bloxide-macros/tests/blox_messages.rs` (new file):
  - Test: messages with `Vec<u8>` field without `copy` — verify generated code does NOT derive `Copy`.
  - Test: messages with primitive fields with `copy` — verify generated code DOES derive `Copy`.
  - Test: messages with `String` field without `copy` — verify generated code compiles.

**Verification:** `cargo test -p bloxide-macros && cargo check --workspace`

---

### Fix 7: Actor ID mismatch in bhsm-tst-demo

**Problem:** `examples/bhsm-tst-demo.rs:402` uses `next_actor_id!()` instead of `bhsm_ref.id()`, unlike all other examples.

**Files to modify:**
- `examples/bhsm-tst-demo.rs:402` — Change:
  ```rust
  // Before:
  let bhsm_ctx = BhsmTstCtx::new(bloxide_tokio::next_actor_id!());
  // After:
  let bhsm_ctx = BhsmTstCtx::new(bhsm_id);
  ```
  Ensure `bhsm_id` is defined as `let bhsm_id = bhsm_ref.id();` earlier in the function.

**Verification:** `cargo check --example bhsm-tst-demo`

---

### Fix 8: TokioKillCap::register leaks tasks on overwrite

**Problem:** `runtimes/bloxide-tokio/src/kill.rs:26-28` — `HashMap::insert` replaces existing `JoinHandle` without aborting the old task.

**Files to modify:**
- `runtimes/bloxide-tokio/src/kill.rs:26-28`:
  ```rust
  pub fn register(&self, actor_id: ActorId, handle: JoinHandle<()>) {
      if let Some(old) = self.tasks.lock().unwrap().insert(actor_id, handle) {
          old.abort();
      }
  }
  ```

- Add cleanup on normal completion: In `run_supervised_actor` (both runtimes), call `kill_cap.unregister(child_id)` when the actor exits normally (not via kill). This prevents slow memory leak of completed `JoinHandle`s.

**Tests to add:**
- `runtimes/bloxide-tokio/src/kill.rs` (test module):
  - `register_overwrite_aborts_old_task`: Register a long-running task, register a second task with the same ID, verify the first task is aborted (check `is_finished()` on a saved copy of the first handle).
  - `unregister_removes_task`: Register a task, unregister it, verify it's removed from the HashMap.

**Verification:** `cargo test -p bloxide-tokio`

---

### Fix 9: Lifecycle events silently dropped when notify channel is full

**Problem:** Both runtimes' `report_outcome` discards `try_send` errors. Critical lifecycle events (`Done`, `Failed`, `Reset`, `Stopped`) can be silently lost.

**Files to modify:**
- `runtimes/bloxide-tokio/src/supervision.rs:93-95`:
  ```rust
  let send = |event| {
      if let Err(_) = <TokioRuntime as BloxRuntime>::try_send_via(notify, Envelope(actor_id, event)) {
          bloxide_log::blox_log_warn!(actor_id, "failed to send lifecycle event to supervisor (channel full or closed)");
      }
  };
  ```

- `runtimes/bloxide-embassy/src/supervision.rs:91-93` — Same fix with `EmbassyRuntime`.

- Increase notify channel capacity from 16 to 32 in both runtimes' `ChildGroupBuilder`:
  - `runtimes/bloxide-tokio/src/supervision.rs` — `ChildGroupBuilder::new`
  - `runtimes/bloxide-embassy/src/supervision.rs` — `ChildGroupBuilder::new`

**Tests to add:**
- `runtimes/bloxide-tokio/src/supervision.rs` (test module):
  - `report_outcome_logs_warning_when_channel_full`: Fill notify channel to capacity, call `report_outcome` with a `Failed` event, verify warning is logged (use a test log capture or assert no panic).

**Verification:** `cargo test -p bloxide-tokio && cargo test -p bloxide-embassy`

---

### Fix 10: Pool silently drops DoWork when worker channel is full

**Problem:** `crates/bloxes/pool/src/actions.rs:19` — `let _ = domain_ref.try_send(...)` discards the result. If the worker's channel is full, the task is lost and `pending` never reaches 0.

**Files to modify:**
- `crates/bloxes/pool/src/actions.rs:19`:
  ```rust
  if let Err(_) = domain_ref.try_send(self_id, WorkerMsg::DoWork(DoWork { task_id })) {
      bloxide_log::blox_log_warn!(self_id, "worker channel full, dropping task_id={}", task_id);
      if ctx.pending() > 0 {
          ctx.set_pending(ctx.pending() - 1);
      }
  }
  ```

**Tests to add:**
- `crates/bloxes/pool/src/tests.rs`:
  - `spawn_worker_with_full_channel_decrements_pending`: Create a pool context, set up a worker with a full channel (set `full` flag on `TestSender`), call `spawn_worker`, verify `pending` is decremented back.

**Verification:** `cargo test -p pool-blox`

---

## Phase 3: Medium Severity Bugs

### Fix 11: Mailboxes hang in release on channel close

**Problem:** `crates/bloxide-core/src/generated/mailboxes_impls.rs:16-22` — `debug_assert!(false)` panics in debug but falls through to `Poll::Pending` in release, causing actors to hang forever.

**Decision:** Breaking change — change `Mailboxes::poll_next` return type from `Poll<E>` to `Poll<Option<E>>`. `Ready(Some(e))` = event, `Ready(None)` = all streams closed, `Pending` = waiting. This is cleaner and correct.

**Files to modify:**

- `crates/bloxide-core/src/mailboxes.rs`:
  - Change trait signature:
    ```rust
    pub trait Mailboxes<E> {
        fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<E>>;
    }
    ```
  - Update `NoMailboxes` impl to return `Poll::Ready(None)` (already returns `Pending`, change to `Ready(None)` or keep `Pending` — `Pending` is correct for "no mailboxes, wait forever"). Actually keep `NoMailboxes` as `Pending` since it's for direct-dispatch testing where the loop is never entered.

- `crates/bloxide-core/src/generated/mailboxes_impls.rs` — For each arity (1-16), update every tuple impl:
  - When a stream returns `Poll::Ready(None)`: return `Poll::Ready(None)` instead of falling through to `Pending`.
  - When all streams return `Pending`: return `Poll::Pending`.
  - The generated code needs to track whether any stream returned `Ready(None)` and propagate it.

  Example for arity 1:
  ```rust
  fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<E>> {
      match Pin::new(&mut self.0).poll_next(cx) {
          Poll::Ready(Some(item)) => Poll::Ready(Some(E::from(item))),
          Poll::Ready(None) => Poll::Ready(None),
          Poll::Pending => Poll::Pending,
      }
  }
  ```

  For arity 2+:
  ```rust
  fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<E>> {
      match Pin::new(&mut self.0).poll_next(cx) {
          Poll::Ready(Some(item)) => return Poll::Ready(Some(E::from(item))),
          Poll::Ready(None) => return Poll::Ready(None),
          Poll::Pending => {}
      }
      match Pin::new(&mut self.1).poll_next(cx) {
          Poll::Ready(Some(item)) => return Poll::Ready(Some(E::from(item))),
          Poll::Ready(None) => return Poll::Ready(None),
          Poll::Pending => {}
      }
      Poll::Pending
  }
  ```

- `crates/bloxide-core/src/actor.rs`:
  - In `run_actor`, `run_actor_to_completion`, `run_actor_auto_start` — handle `Poll::Ready(None)` from `mailboxes.poll_next`:
    ```rust
    let event = match poll_fn(|cx| mailboxes.poll_next(cx)).await {
        Some(event) => event,
        None => return, // all mailbox streams closed — graceful shutdown
    };
    ```

- `runtimes/bloxide-tokio/src/lib.rs:200-205` — In `run_root`, handle `None`:
  ```rust
  loop {
    let event = match poll_fn(|cx| mailboxes.poll_next(cx)).await {
        Some(event) => event,
        None => return,
    };
    match machine.dispatch(event) {
        DispatchOutcome::Reset | DispatchOutcome::Stopped | DispatchOutcome::Failed => return,
        DispatchOutcome::Done(_) => return,
        _ => {}
    }
  }
  ```

- `runtimes/bloxide-embassy/src/lib.rs:203-208` — Same fix.

- `runtimes/bloxide-tokio/src/supervision.rs` — In `run_supervised_actor`, the `poll_fn` closure that polls lifecycle stream and domain mailboxes. Update to handle `Ready(None)` from domain mailboxes.

- `runtimes/bloxide-embassy/src/supervision.rs` — Same.

- `crates/bloxide-timer/src/` — If the timer service uses `Mailboxes::poll_next`, update it too.

**Tests to add:**
- `crates/bloxide-core/src/tests/`:
  - `mailbox_close_returns_ready_none`: Create a `TestRuntime` channel, drop the sender, poll the receiver stream via `Mailboxes::poll_next`, verify `Poll::Ready(None)`.
  - `run_actor_exits_on_mailbox_close`: Drop all senders, verify `run_actor` returns.

**Verification:** `cargo check --workspace && cargo test --workspace && cargo check --workspace --examples`

---

### Fix 12: `run_root` ignores terminal/error states

**Problem:** Both runtimes' `run_root` only returns on `DispatchOutcome::Reset`. If the root supervisor enters a terminal or error state, it keeps dispatching events to a completed machine.

**Files to modify:**
- `runtimes/bloxide-tokio/src/lib.rs:200-205`:
  ```rust
  loop {
      let event = match poll_fn(|cx| mailboxes.poll_next(cx)).await {
          Some(event) => event,
          None => return,
      };
      match machine.dispatch(event) {
          DispatchOutcome::Reset
          | DispatchOutcome::Stopped
          | DispatchOutcome::Failed
          | DispatchOutcome::Done(_) => return,
          _ => {}
      }
  }
  ```

- `runtimes/bloxide-embassy/src/lib.rs:203-208` — Same fix.

**Tests to add:**
- `runtimes/bloxide-tokio/src/lib.rs` (test module):
  - `run_root_exits_on_terminal_state`: Create a root machine whose initial state is terminal, dispatch Start, verify `run_root` returns.

**Verification:** `cargo test -p bloxide-tokio && cargo test -p bloxide-embassy`

---

### Fix 13: No guard against transitions out of terminal/error states

**Problem:** `crates/bloxide-core/src/engine.rs` — The engine doesn't check `is_terminal()`/`is_error()` before dispatching domain events. An actor run with `run_actor` (non-terminating) can leave terminal states if the state has non-empty transition rules.

**Files to modify:**
- `crates/bloxide-core/src/engine.rs` — In `process_operational_event` (line 285+), add early return for terminal/error states:
  ```rust
  fn process_operational_event(&mut self, event: S::Event) -> DispatchOutcome<S::State> {
      let current = match self.current {
          MachineState::State(s) => s,
          MachineState::Init => unreachable!("process_operational_event called while in Init"),
      };

      // Terminal and error states absorb all events without transition.
      // The runtime is responsible for stopping the event loop when
      // a terminal/error outcome is observed.
      if S::is_terminal(&current) || S::is_error(&current) {
          return DispatchOutcome::HandledNoTransition;
      }

      // ... existing bubbling dispatch logic ...
  }
  ```

**Tests to add:**
- `crates/bloxide-core/src/tests/`:
  - `terminal_state_absorbs_events`: Create a machine in a terminal state with non-empty transition rules, dispatch a matching event, verify `HandledNoTransition` is returned with no state change or callbacks.
  - `error_state_absorbs_events`: Same for an error state.

**Verification:** `cargo test -p bloxide-core`

---

### Fix 14: Or-patterns get wrong event tag in FullEvent mode

**Problem:** `crates/bloxide-macros/src/transitions.rs:209-280` — `extract_event_tag` only captures the first variant's tag for `Event::Foo | Event::Bar`, silently dropping events matching the second variant in the fast pre-filter.

**Files to modify:**
- `crates/bloxide-macros/src/transitions.rs` — In `extract_event_tag`:
  - Scan the token stream for top-level `|` Punct tokens.
  - If any `|` is found at the top level (not inside a group), return `WILDCARD_TAG` (255):
    ```rust
    let has_or_pattern = tokens.iter().any(|t| {
        matches!(t, TokenTree::Punct(p) if p.as_char() == '|')
    });
    if has_or_pattern {
        return EventTagKind::Constant(quote! { ::bloxide_core::event_tag::WILDCARD_TAG });
    }
    ```

**Tests to add:**
- `crates/bloxide-macros/tests/transitions.rs`:
  - Test: `Event::Foo | Event::Bar` in `FullEvent` mode — verify both events trigger the transition (not just `Foo`).

**Verification:** `cargo test -p bloxide-macros`

---

### Fix 15: Codegen requires manual patches

**Problem:** 4 of 5 examples manually patch generated `spec_skeleton.rs`/`ctx.rs` files because codegen can't handle generic context types, trait bounds, or delegate imports.

**Files to modify:**

- `crates/tools/bloxide-codegen/src/spec_skeleton.rs` (or equivalent generation module):
  - Fix `spec_skeleton.rs` generation to handle:
    1. Generic context types (`PoolCtx<R>` where `R: BloxRuntime`) — emit correct generic parameters and bounds.
    2. Trait bounds on context generics (e.g., `B: HasCurrentTimer + CountsRounds`) — parse from `blox.toml` and emit in `where` clauses.
    3. Delegate trait imports — generate `use` statements for delegated traits.
  - The generated `MachineSpec` impl must include all trait bounds from the context definition.

- `crates/tools/bloxide-codegen/src/ctx.rs` (or equivalent):
  - Fix `ctx.rs` generation to:
    1. Emit `#[derive(BloxCtx)]` with correct field annotations (no `#[ctor]` needed once Fix 1 is done).
    2. Generate correct field types including generics.
    3. Generate delegate imports for `#[delegates(...)]` fields.
    4. Do NOT generate manual trait impls in the generated file — those should come from the macro.

- `crates/tools/bloxide-codegen/src/lib.rs`:
  - Ensure round-trip generation: parse `blox.toml` → generate → output matches hand-patched files.

- `crates/tools/cargo-blox/src/` — Update codegen invocation if needed.

- **Remove manual patch functions from all examples:**
  - `examples/embassy-demo.rs` — Remove `patch_generated_ping` and `patch_generated_pong` functions (around line 59-60 and their call sites). Regenerate via `cargo blox generate`.
  - `examples/tokio-demo.rs` — Remove patch code around line 507 and the doc comment about patching at line 4.
  - `examples/tokio-pool-demo.rs` — Remove patch code around lines 110-111.
  - `examples/bhsm-tst-demo.rs` — Remove any patch code if present.

**Tests to add:**
- `crates/tools/bloxide-codegen/src/` (test module):
  - Test: generate spec_skeleton for a blox with generic context (`Ctx<R: BloxRuntime, B: Trait1 + Trait2>`) — verify output contains correct generics and where clause.
  - Test: generate ctx.rs for a blox with `#[delegates(Trait1, Trait2)]` — verify output contains derive and delegates annotation.
  - Test: round-trip all existing blox.toml files — generate, then compare against committed generated files (after removing patches).

**Verification:** `cargo test -p bloxide-codegen && cargo blox generate (in each blox dir) && cargo check --workspace --examples`

**Dependencies:** Fix 1 must be done first (removes `#[ctor]` dependency).

---

### Fix 16: bloxide-viz-export is non-functional

**OUT OF SCOPE — Skip per decision.**

---

## Phase 4: Missing Functionality

### Fix 17: Implement OTP restart strategies (one-for-all, rest-for-one)

**Problem:** Only implicit one-for-one restart is available via `ChildPolicy::Restart { max }`. No `one-for-all` or `rest-for-one` strategies.

**Files to modify:**

- `crates/bloxide-supervisor/src/registry.rs`:
  - Add `RestartStrategy` enum:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RestartStrategy {
        /// Restart only the failed child (default, current behavior).
        OneForOne,
        /// Restart all children when any child fails.
        OneForAll,
        /// Restart the failed child and all children declared after it.
        RestForOne,
    }
    ```
  - Add `restart_strategy: RestartStrategy` field to `ChildGroup`.
  - Default to `OneForOne` in `ChildGroupBuilder::new`.
  - Add `pub fn restart_strategy(mut self, strategy: RestartStrategy) -> Self` builder method.
  - In `handle_done_or_failed` (where `ChildPolicy::Restart` is handled), implement the strategies:
    - `OneForOne`: current behavior — reset only the failed child.
    - `OneForAll`: stop all children, then start all children. For each child: send `Reset` if running, or `Start` if already in Init/AwaitingReset. Increment restart counts for all restarted children.
    - `RestForOne`: stop and restart the failed child and all children after it in declaration order. Children before the failed one are untouched.

- `crates/bloxide-supervisor/src/control.rs`:
  - Re-export `RestartStrategy` from the prelude.
  - Add `RestartStrategy` to `ChildGroupBuilder`.

- `crates/bloxide-supervisor/src/supervisor.rs`:
  - Update `SupervisorCtx` to store and pass `RestartStrategy` to the registry.
  - The supervisor state machine doesn't need new states — the restart strategy is applied in the registry's `handle_done_or_failed`.

- `crates/bloxide-supervisor/src/prelude.rs`:
  - Add `pub use crate::registry::RestartStrategy;`

- `crates/bloxide-supervisor/src/event.rs`:
  - No changes needed — events stay the same.

- `spec/architecture/08-supervision.md`:
  - Document all three restart strategies with examples.
  - Add decision tree for choosing a strategy.

**Tests to add:**
- `crates/bloxide-supervisor/src/supervisor.rs` (test module):
  - `one_for_all_restarts_all_children`: Create a group with 3 children and `OneForAll` strategy. Fail child 2. Verify all 3 children receive Reset + Start.
  - `rest_for_one_restarts_subsequent_children`: Create a group with 4 children and `RestForOne` strategy. Fail child 2. Verify children 2, 3, 4 receive Reset + Start. Verify child 1 is untouched.
  - `one_for_one_restarts_only_failed_child`: Regression test for existing behavior with explicit strategy.
  - `restart_strategy_respects_max_restarts`: With `OneForAll` and `max: 1`, fail child 1 twice. Verify all children are marked `PermanentlyDone` after the second failure.

**Verification:** `cargo test -p bloxide-supervisor`

---

### Fix 18: History states

**DEFERRED — Future task.**

---

### Fix 19: Event deferral

**DEFERRED — Future task.**

---

### Fix 20: Add tests for Guard::Fail path

**Problem:** The engine's `Guard::Fail` → `transition_to_init()` + `DispatchOutcome::Failed` path has zero test coverage.

**Files to modify:**
- `crates/bloxide-core/src/tests/lifecycle_dispatch.rs`:
  - Add test `fail_guard_transitions_to_init_and_reports_failed`:
    1. Create a machine in state A (operational).
    2. Dispatch an event that triggers `Guard::Fail`.
    3. Verify outcome is `DispatchOutcome::Failed`.
    4. Verify machine is now in `MachineState::Init`.
    5. Verify exit callbacks fired for state A (and ancestors).
    6. Verify `on_init_entry` was called.

  - Add test `fail_guard_from_deep_state_exits_full_chain`:
    1. Create a machine in state C (deep nested under Other under Top).
    2. Dispatch an event that triggers `Guard::Fail`.
    3. Verify exit callbacks fire for C, Other, Top in order.
    4. Verify `on_init_entry` is called.

**Verification:** `cargo test -p bloxide-core`

---

### Fix 21: Add tests for KillCap / ChildPolicy::Kill

**Problem:** The supervisor's kill functionality is entirely untested.

**Files to modify:**
- `crates/bloxide-supervisor/src/registry.rs` (test module):
  - `kill_child_aborts_task`: Create a `ChildGroup` with `KillCap`, register a child, call `kill_child`, verify returns `true`, verify phase is `Killed`, verify `stopped_count` incremented.
  - `kill_child_without_kill_cap_returns_false`: Create a `ChildGroup` without `KillCap`, call `kill_child`, verify returns `false`.
  - `kill_already_killed_child_returns_false`: Kill a child, then kill it again, verify second call returns `false`.
  - `kill_stopped_child_returns_false`: Stop a child (set phase to `Stopped`), then kill it, verify returns `false`.
  - `kill_child_with_kill_policy`: Set `ChildPolicy::Kill`, simulate a `Failed` event, verify `kill_child` is called and child is `Killed`.

**Verification:** `cargo test -p bloxide-supervisor`

---

### Fix 22: `event!()` macro doesn't allow custom derives

**Problem:** `crates/bloxide-macros/src/blox_event_new.rs:103-108` hard-codes `#[derive(Debug)]`.

**Files to modify:**
- `crates/bloxide-macros/src/blox_event_new.rs`:
  - Parse `#[derive(...)]` attributes from the input enum.
  - Pass them through in the generated enum definition.
  - If no `#[derive(...)]` is present, default to `#[derive(Debug)]`.
  - Example: `event!({ #[derive(Debug, Clone, Copy)] enum MyEvent { ... } })` generates the enum with all specified derives.

**Tests to add:**
- `crates/bloxide-macros/tests/blox_event.rs` (new file):
  - Test: `event!()` with `#[derive(Debug, Clone, Copy)]` — verify generated code compiles and all derives are present.
  - Test: `event!()` without custom derives — verify default `Debug` derive is present.

**Verification:** `cargo test -p bloxide-macros`

---

## Phase 5: Low Severity / Cleanup

### Fix 23: `test.log` tracked in git

**Files to modify:**
- `test.log` — `git rm test.log`
- `.gitignore` — Add `test.log` entry

---

### Fix 24: 6 keywords in workspace Cargo.toml (crates.io max is 5)

**Files to modify:**
- `Cargo.toml:48` — Remove `"embedded"` keyword (covered by `"no_std"`):
  ```toml
  keywords = ["actor", "hsm", "state-machine", "no_std", "async"]
  ```

---

### Fix 25: Message crates use path deps instead of workspace deps

**Files to modify:**
- `crates/messages/ping-pong-messages/Cargo.toml` — Change `bloxide-macros = { path = "../../bloxide-macros" }` to `bloxide-macros = { workspace = true }`
- `crates/messages/pool-messages/Cargo.toml` — Change `bloxide-macros` and `bloxide-core` to `{ workspace = true }`
- `crates/messages/counter-messages/Cargo.toml` — Change `bloxide-macros` to `{ workspace = true }`
- `crates/messages/bhsm-tst-messages/Cargo.toml` — Change `bloxide-macros` to `{ workspace = true }`

---

### Fix 26: Dead code — blox_mailboxes.rs in bloxide-macros

**Files to modify:**
- `crates/bloxide-macros/src/blox_mailboxes.rs` — Delete file (340 lines, unused, marked `#[allow(dead_code)]`)
- `crates/bloxide-macros/src/lib.rs` — Remove `mod blox_mailboxes;` declaration

---

### Fix 27: Dead code — erased_reply.rs in bloxide-tokio

**Files to modify:**
- `runtimes/bloxide-tokio/src/erased_reply.rs` — Delete file (never declared as module, `ArcErasedSender` never used)

---

### Fix 28: Dead code — BloxEventInput in blox_event.rs

**Files to modify:**
- `crates/bloxide-macros/src/blox_event.rs` — Remove `BloxEventInput` struct (lines 10-16), its `Parse` impl (lines 57-172), and `blox_event_simple_inner` function. These are marked `#[allow(dead_code)]` and never used by any exported macro.

---

### Fix 29: Deprecation warning for old annotations is a no-op

**Problem:** `crates/bloxide-macros/src/blox_ctx/generate.rs:118-127` — `#[deprecated]` on `const _: () = ()` never triggers because the anonymous const is never referenced.

**Files to modify:**
- `crates/bloxide-macros/src/blox_ctx/generate.rs:118-127` — Replace the non-functional deprecation with a `const _: () = { ... }` block that checks for deprecated annotations at compile time:
  ```rust
  fn generate_deprecation_warning() -> TokenStream {
      // Generate a deprecated function that must be called to trigger the warning.
      // The BloxCtx derive will call this function if deprecated annotations are found.
      quote! {
          #[deprecated(note = "BloxCtx: annotations are auto-detected by naming convention; remove #[self_id], #[provides], #[ctor]")]
          fn _bloxctx_deprecated_annotation() {}
      }
  }
  ```
  Then in the generated code, when a deprecated annotation is detected, emit a call to `_bloxctx_deprecated_annotation()` which will trigger the deprecation lint.

  Alternative approach: emit `compile_error!` for deprecated annotations (since we don't need backwards compatibility). This is simpler and more forceful:
  ```rust
  // In the field analysis, if a deprecated annotation is found, emit:
  syn::Error::new_spanned(field, "annotation is auto-detected by naming convention; remove it").to_compile_error()
  ```

  **Decision:** Use `compile_error!` since backwards compatibility is not required. Remove support for `#[self_id]`, `#[provides]`, `#[ctor]` entirely. Auto-detection by naming convention is the only supported path. For fields that need to suppress auto-detection (e.g., a `_ref` field that should NOT generate an accessor), add a new `#[blox_ctx(skip)]` annotation.

---

### Fix 30: Unused dependencies

**Files to modify:**
- `crates/bloxes/pong/Cargo.toml` — Remove `bloxide-log` dependency
- `crates/messages/ping-pong-messages/Cargo.toml` — Remove `bloxide-macros` dependency (if not used after Fix 6 changes)
- `crates/messages/pool-messages/Cargo.toml` — Remove `bloxide-macros` dependency (if not used)
- `crates/messages/counter-messages/Cargo.toml` — Remove `bloxide-macros` dependency (if not used)
- `crates/messages/bhsm-tst-messages/Cargo.toml` — Remove `bloxide-macros` dependency (if not used)

Note: If message crates use `blox_messages!` macro, they need `bloxide-macros`. Check if the macro is used in the message crate source or only in the codegen tool. If generated files don't invoke the macro, the dependency is unused.

---

### Fix 31: Add Debug impl for ActorRef, Clone for Envelope

**Files to modify:**
- `crates/bloxide-core/src/messaging.rs`:
  - Add manual `Debug` impl for `ActorRef<M, R>`:
    ```rust
    impl<M, R: BloxRuntime> core::fmt::Debug for ActorRef<M, R>
    where
        M: Send + 'static,
    {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("ActorRef")
                .field("id", &self.id)
                .field("msg_type", &core::any::type_name::<M>())
                .finish()
        }
    }
    ```
  - Add conditional `Clone` impl for `Envelope<M: Clone>`:
    ```rust
    #[derive(Debug)]
    pub struct Envelope<M>(pub ActorId, pub M);

    impl<M: Clone> Clone for Envelope<M> {
        fn clone(&self) -> Self {
            Envelope(self.0, self.1.clone())
        }
    }
    ```

---

### Fix 32: Fix misleading TokioStream doc comment

**Files to modify:**
- `runtimes/bloxide-tokio/src/channel.rs:31-33` — Update doc comment:
  ```rust
  /// Stream wrapper around `tokio::mpsc::Receiver`.
  ///
  /// Returns `Poll::Ready(None)` when the channel is closed (all senders dropped).
  /// Callers should handle `None` as a graceful shutdown signal.
  ```

---

### Fix 33: Fix supervisor `kill_child` log using actor ID 0

**Files to modify:**
- `crates/bloxide-supervisor/src/registry.rs:163` — Add `from: ActorId` parameter to `kill_child`:
  ```rust
  pub fn kill_child(&mut self, from: ActorId, child_id: ActorId) -> bool {
      // ...
      blox_log_warn!(from, "KillCap not available for child {}", child_id);
      // ...
  }
  ```
  Update all call sites to pass the supervisor's actor ID.

---

### Fix 34: Fix `stop_all`/`start_all` to filter by phase

**Files to modify:**
- `crates/bloxide-supervisor/src/registry.rs:127-157`:
  - `start_all`: Skip children with phase `Running`, `PermanentlyDone`, `Stopped`, `Killed`.
  - `stop_all`: Skip children with phase `Stopped`, `Killed`, `PermanentlyDone`.

---

### Fix 35: Fix supervisor try_send warning messages

**Files to modify:**
- `crates/bloxide-supervisor/src/registry.rs` — All `try_send` failure warnings (lines 118-123, 132-138, 148-155, 220-225, 265-270, 315-324):
  - Change message from "channel full" to "try_send failed (channel full or closed)".

---

### Fix 36: Fix TimerQueue silent message loss on delivery

**Files to modify:**
- `crates/bloxide-timer/src/actions.rs:27` — Add `blox_log_warn!` inside the `deliver` closure when `try_send` fails:
  ```rust
  let deliver = |event: E| {
      if let Err(_) = target.try_send(TIMER_ACTOR_ID, event) {
          bloxide_log::blox_log_warn!(TIMER_ACTOR_ID, "timer delivery failed (channel full or closed)");
      }
  };
  ```

---

### Fix 37: Fix timer `next_timer_id` inconsistent overflow

**Files to modify:**
- `crates/bloxide-timer/src/command.rs`:
  - Atomic path (line 56): Replace `fetch_add(1, Ordering::Relaxed)` with a `compare_exchange` loop that saturates:
    ```rust
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    // Use fetch_add and check for wrap-around
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    if id == 0 || id == usize::MAX {
        // Wrapped or saturated — return a saturated value
        return TimerId(usize::MAX);
    }
    TimerId(id)
    ```
  - Non-atomic path (line 64): Keep `saturating_add(1)` — already correct.
  - Alternatively, just use `saturating_add` semantics on both paths for consistency.

---

### Fix 38: Remove redundant booleans in ChildEntry

**Files to modify:**
- `crates/bloxide-supervisor/src/registry.rs:48-57`:
  - Remove `permanently_done: bool` and `stopped: bool` fields from `ChildEntry`.
  - Replace all usages:
    - `e.permanently_done` → `matches!(e.phase, ChildPhase::PermanentlyDone)`
    - `e.stopped` → `matches!(e.phase, ChildPhase::Stopped | ChildPhase::Killed)`
  - Update `check_shutdown` (line 239-254): use `matches!(e.phase, ...)`.
  - Update `is_health_monitored` (line 335-342): use `matches!(e.phase, ...)`.
  - Update `clear_counters` (line 368-377): remove boolean resets.
  - Update all other references throughout the file.

---

### Fix 39: Add missing Debug/Clone derives for WorkerCtrl

**Files to modify:**
- `crates/messages/pool-messages/src/lib.rs:19-31`:
  - Add `#[derive(Clone)]` to `WorkerCtrl<R>`, `AddWorkerPeer<R>`, `RemoveWorkerPeer`.
  - Add `#[derive(Debug)]` to `WorkerCtrl<R>` (possible after Fix 31 adds `Debug` to `ActorRef`).
  - Note: `WorkerEvent` can then derive `Debug` for consistency with other event types.

---

## Execution Order

```
Phase 1 (Critical):     Fixes 1, 2, 3, 4
  - Fix 1 first (unblocks Fix 15)
  - Fixes 2, 3, 4 can run in parallel with Fix 1

Phase 2 (High):         Fixes 5, 6, 7, 8, 9, 10
  - All independent, can run in parallel
  - Fix 6 may require updating all blox_messages! invocations

Phase 3 (Medium):       Fixes 11, 12, 13, 14, 15
  - Fix 11 first (unblocks Fix 12 — both touch run_root)
  - Fix 15 depends on Fix 1
  - Fix 13 depends on nothing

Phase 4 (Missing):      Fixes 17, 20, 21, 22
  - Fix 17 (OTP strategies) is the largest — implement after Phase 3
  - Fixes 20, 21 (tests) can be done anytime
  - Fix 22 independent

Phase 5 (Cleanup):      Fixes 23-39
  - All independent, can be done in any order
  - Fix 31 should be done before Fix 39 (Debug for ActorRef needed for WorkerCtrl Debug)

After each phase: run `cargo check --workspace && cargo test --workspace && cargo check --workspace --examples`
```

## Estimated Effort

| Phase | Fixes | Effort | Notes |
|-------|-------|--------|-------|
| 1 | 1-4 | Large | Fix 3 (spawn redesign) is the biggest piece |
| 2 | 5-10 | Medium | Fix 6 (blox_messages) touches many files |
| 3 | 11-15 | Large | Fix 11 (Mailboxes API) is breaking and touches many files; Fix 15 (codegen) is substantial |
| 4 | 17, 20-22 | Medium | Fix 17 (OTP strategies) is new feature work |
| 5 | 23-39 | Small | Mostly mechanical cleanup |

## Deferred Items

| Item | Reason |
|------|--------|
| History states (shallow/deep) | Large engine change, future task |
| Event deferral | Large engine change, future task |
| bloxide-viz-export / visualizer | Out of scope |
