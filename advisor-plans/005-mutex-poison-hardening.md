# Plan 005: Stop a poisoned in-memory lock from cascading into 500s

> **Executor instructions**: Follow this plan step by step. Run every
> verification command. If a STOP condition occurs, stop and report. When done,
> update this plan's row in `advisor-plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 14f9d95..HEAD -- controller/src/shell.rs controller/src/routes/deployments.rs controller/src/routes/agent.rs controller/src/state.rs`
> If the lock call sites moved, re-locate them with the grep in "Current state"
> before editing.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug (robustness)
- **Planned at**: commit `14f9d95`, 2026-06-19

## Why this matters

The controller keeps two in-memory registries on `AppState` behind
`std::sync::Mutex`: the live deploy-progress map (`state.progress`) and the
container-shell session registry (`state.shells`). Every access uses
`.lock().expect("...")`. `std::sync::Mutex` **poisons** if any thread panics
while holding the guard — and once poisoned, every subsequent `.lock().expect()`
panics too. So a single panic inside one of these short critical sections turns
into a permanent failure of *all* progress polling or *all* shell sessions for
the lifetime of the process: the deployment UI's live progress and the
in-browser terminal both hard-fail with 500s until the controller is
restarted. The blast radius is out of proportion to the cause.

These are ephemeral in-memory caches (recreated on restart, deliberately — per
doctrine, in-memory progress/shell state is by design). That is exactly the
case where recovering a poisoned guard is the right call instead of propagating
the panic.

## Current state

The seven call sites (`grep -rn '\.lock()\.expect' controller/src`):

```
controller/src/shell.rs:125:    state.shells.lock().expect("shells lock").insert(
controller/src/shell.rs:213:    let mut reg = registry.lock().expect("shells lock");
controller/src/shell.rs:236:        let mut reg = state.shells.lock().expect("shells lock");
controller/src/routes/deployments.rs:104:    let map = state.progress.lock().expect("progress lock");
controller/src/routes/deployments.rs:214:    state.progress.lock().expect("progress lock").remove(&id.0);
controller/src/routes/agent.rs:255:        state.progress.lock().expect("progress lock").remove(&id.0);
controller/src/routes/agent.rs:271:        let mut map = state.progress.lock().expect("progress lock");
```

These are synchronous `std::sync::Mutex` (not `tokio::sync::Mutex`, which would
be `.lock().await`). `std::sync::PoisonError` exposes `into_inner()` to recover
the guard, which is safe here because the protected data is a plain map used as
a cache, not an invariant-bearing structure.

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Locate sites | `grep -rn '\.lock()\.expect' controller/src` | the 7 lines above (0 after) |
| Build | `cargo build -p foundry-controller` | exit 0 |
| Lint | `cargo clippy -p foundry-controller --all-targets -- -D warnings` | exit 0 |
| Tests | `cargo test -p foundry-controller` | all pass |

## Scope

**In scope**:
- `controller/src/shell.rs`
- `controller/src/routes/deployments.rs`
- `controller/src/routes/agent.rs`
- A small helper — preferably a tiny extension or free function near the
  `AppState` definition (`controller/src/state.rs`) so all sites share it.

**Out of scope**:
- Adding a new dependency (e.g. `parking_lot`). The std recovery pattern needs
  no new crate; do not add one in this plan.
- Changing the locks to `tokio::sync::Mutex` — that's an async refactor with
  wider blast radius; not this plan.
- The semantics of progress/shell state (still in-memory, still lost on
  restart — that's by design).

## Git workflow

- Branch: `advisor/005-mutex-poison-hardening`
- One commit: `controller: recover poisoned in-memory locks instead of panicking`
- Do NOT push unless instructed.

## Steps

### Step 1: Add a poison-recovering lock helper

Add one small helper so the seven sites don't each hand-roll recovery. Two
acceptable shapes — pick the one that fits the codebase's style:

- A free function:
  ```rust
  /// Lock an in-memory cache mutex, recovering the guard if a prior holder
  /// panicked. These maps are ephemeral caches; a poisoned lock must not
  /// take down progress/shell handling.
  pub fn lock_recover<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
      m.lock().unwrap_or_else(|e| e.into_inner())
  }
  ```
- Or an extension trait with a `.lock_recover()` method on `Mutex<T>`.

Place it next to `AppState` (`controller/src/state.rs`) and export it for the
route/shell modules.

**Verify**: `cargo build -p foundry-controller` → exit 0.

### Step 2: Replace all seven `.lock().expect(...)` call sites

Swap each `state.progress.lock().expect("progress lock")` /
`...shells.lock().expect("shells lock")` (and the `registry.lock().expect(...)`
at `shell.rs:213`) for the helper. Behavior on the happy path is identical; the
only change is that a poisoned lock recovers instead of panicking.

**Verify**: `grep -rn '\.lock()\.expect' controller/src` → no matches.

### Step 3: Gate

**Verify**: `cargo clippy -p foundry-controller --all-targets -- -D warnings`
→ exit 0; `cargo test -p foundry-controller` → all pass.

## Test plan

- Add a unit test next to the helper: build a `Mutex<HashMap<..>>`, poison it
  by catching a panic from a closure that panics while holding the guard
  (`std::panic::catch_unwind` around a `let _g = m.lock(); panic!()`), then
  assert `lock_recover(&m)` returns a usable guard (insert + read back).
- Verification: `cargo test -p foundry-controller` → the new test passes.

## Done criteria

- [ ] `grep -rn '\.lock()\.expect' controller/src` returns nothing
- [ ] `cargo clippy -p foundry-controller --all-targets -- -D warnings` exits 0
- [ ] `cargo test -p foundry-controller` exits 0, helper test present
- [ ] Only the four in-scope files changed (`git status`)
- [ ] `advisor-plans/README.md` status row updated

## STOP conditions

- A `.lock()` site protects data where recovering after a panic could expose a
  broken invariant (not just a cache) — if any of the protected types turns out
  to be more than a plain map/registry, STOP and report rather than blanket-
  recovering it.
- The locks turn out to be `tokio::sync::Mutex` (async `.lock().await`) — then
  poisoning doesn't apply and this plan is moot; report it.

## Maintenance notes

- If the team later wants to drop poisoning semantics entirely, `parking_lot`
  is the standard move — but that's a dependency decision for a separate plan,
  not a robustness hotfix.
- Any new in-memory `Mutex` cache on `AppState` should use the same helper;
  note it in review.
