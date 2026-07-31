# Threading Proposal for Pima

Generated as a design review after evaluating the current Pima architecture, specification, and runtime.

---

## Pima — Evaluation

**What it is:** An experimental, expression-oriented programming language with a register-based VM in Rust. Prefix notation, physical line endings as statement terminators, immutable data by default, first-class blocks (not functions), typed errors as namespace values, and a module system with lifecycle tracking.

### Architecture — Strong

The crate layout is clean and well-disciplined. The inward-pointing dependency graph (`source → syntax → runtime → native`, with `engine` and `vm` as coordinators) is exactly right. The separation between VM IR, compiler passes, and the machine itself gives real extensibility. The architecture doc is thorough — it reads like a spec, not aspirational notes.

The 14-stage VM migration is fully implemented: from literals to a complete register VM with closures, namespaces, typed errors, modules, and cross-module block dispatch.

### Code Base — ~11.5K LOC

- **~7.9K** core library (lexer, parser, runtime, VM, compiler, natives, engine)
- **~3.5K** language server
- **~3.2K** tests
- **~200** standard library (Pima source)

Tight for a full language implementation. The parser at 1K LOC and compiler at 1.2K LOC are doing heavy lifting.

### What Stands Out

1. **Blocks as first-class uninstantiated code** — Blocks don't capture; they resolve at execution time via `do`. Annotated blocks (`@(:name...)`) declare context contracts. A clean alternative to closures for template-like patterns.
2. **Typed errors as namespace values** — Errors are ordinary values with type lists, thrown explicitly, caught by `attempt`. No Rust-style `Result` leakage.
3. **Register VM with compiler passes** — Scope analysis, register allocation, IR, optimization passes (jump threading, no-op removal), and source-span preservation throughout.
4. **Language server** — Full LSP with completions, signatures, semantic tokens, renaming, folding, inlay hints, and document symbols.
5. **Module lifecycle** — Four-state machine (`:unloaded → :loading → :loaded/:failed`) with cycle detection and cached failures.

### Concerns / Areas to Watch

1. **Parser at 1K LOC** — Dense for a language this expressive. May be hard to extend or debug edge cases.
2. **Standard library at 207 lines** — Very compact for the breadth of functions exposed. Worth auditing against the spec's function list.
3. **No package manager / build system** — Fine for experimental, a gap if this matures.
4. **Single-threaded** — Explicitly excluded per spec. Hard ceiling on what applications can do (TCP server blocks on `accept`/`read`).
5. **Benchmarks exist but no baselines** — Without historical data, hard to tell if the VM is "fast enough" or regressing.

---

## Threading Design — Context

### Current Architecture Constraints

- **GC:** Pima uses `dumpster::unsync::Gc` — non-thread-safe. Values traced by the GC cannot safely cross thread boundaries.
- **Value types:** `Value` contains `Gc` pointers (blocks, closures, namespaces, cells). Only scalar values (integers, floats, strings, booleans, symbols) and arena-indexed handles (TCP) are inherently transportable.
- **Host resources:** TCP listeners/connections use arena indices (`TcpListenerId`, `TcpConnectionId`) into `HostResources` — a pattern that threads should follow.
- **Native functions:** All host access goes through `NativeContext` → `VmNativeContext` → `HostResources`. No new syntax or VM instructions needed for threading — native modules suffice.

### Key Insight

Pima blocks are already inert, unexecuted code that don't capture environments. This makes them the natural unit of work for threading — `spawn` takes a block and runs it on another thread. No closures to serialize, no environment to snapshot.

---

## Suggestion 1: Work-Stealing Task Pool — `spawn` + `join`

**The idea:** A native `/pima/thread` module that spawns OS threads executing Pima blocks, with a `join` handle that blocks until completion and returns the result.

**Why it fits Pima:** Blocks are already first-class, uninstantiated code. A `spawn` operation takes a block (already inert) and executes it on a separate thread's VM. No new syntax needed — it's a native function operating on existing `:block` values.

```pima
import "/pima/thread" as thread

val handle [thread.spawn {
    Math.sum (1 2 3 4 5)
}]

val result [thread.join (handle)]
```

### Implementation Shape

- Add `ThreadHandle` to `Value` (arena-indexed, like TCP resources)
- `HostResources` holds a `thread::JoinHandle` arena — same pattern as TCP listeners/connections
- Each spawned thread gets its **own** `Machine` + `VmNativeContext` (no shared mutable VM state)
- The block is referenced by `BlockRef` (just a `Gc` pointer) and resolved against the program's AST
- `join` blocks the calling thread and returns the result value (or a typed error if the spawned block threw)
- The GC must NOT trace across thread boundaries during collection — use `Sync`-wrapped arenas with copy-on-read result transfer

### Key Constraint

Since Pima uses `dumpster::unsync::Gc` (non-thread-safe), the spawned thread needs its own GC root set. Values cross the boundary only at spawn-time (the block reference) and join-time (single result). No shared mutable state between threads by design.

### Why This Is the Right First Step

It lets Pima programs parallelize CPU-bound work (map-reduce, batch processing) without changing the type system, VM, or GC. It's what `std::thread::spawn` gives Rust — simple, correct, composable.

---

## Suggestion 2: Lock-Free Channel Primitives — `channel` + `select`

**The idea:** A native `/pima/channel` module providing multi-producer channels that carry Pima values between threads.

**Why it fits Pima:** Pima already has `attempt` for error handling and `branch` for ordered conditionals. A `channel` fits naturally as a namespace value with `send`, `receive`, and `closed?` operations. Enables worker-pool patterns that `spawn` + `join` alone can't express.

```pima
import "/pima/thread" as thread
import "/pima/channel" as channel

val (sender receiver) [channel.make ()]

val worker [thread.spawn @(:sender) {
    sender.send ("result from worker")
}]

val message [receiver.receive ()]
Console.println message
[thread.join (worker)]
```

### Implementation Shape

- Use Rust's `crossbeam-channel` (or `std::sync::mpsc` for zero new deps) — bridged through native functions
- Sender and Receiver become new `Value` variants holding arena indices into `HostResources`
- `send` and `receive` serialize values by cloning (since they cross thread boundaries)
- `receive` with a timeout variant avoids blocking the VM thread indefinitely

### Critical Gotcha: GC Crossing

Values sent across channels must handle the GC boundary. Two approaches:

**Approach A — Reject unsendable values (recommended):** At send-time, check if a value contains closures, blocks, or namespaces. If so, reject with a `:thread_error :unsendable_value`. Matches Go's channel semantics. Only scalars and simple lists of scalars cross.

**Approach B — Deep-clone serializer:** Implement a cloning visitor that re-allocates values under the receiving thread's GC heap. More powerful but significantly more complex.

---

## Suggestion 3: Shared-State Mutex Namespace — `Mutex` Template

**The idea:** A native-protected mutex that wraps mutable state accessible from multiple threads, exposed as a namespace value in Pima code.

**Why it fits Pima:** Pima already has `var` for mutation and namespaces for encapsulation. A `Mutex` is just a namespace with atomic guard acquisition.

```pima
import "/pima/thread" as thread
import "/pima/mutex" as mutex

val counter [mutex.make 0]

val handles (
    [thread.spawn @(:counter) {
        until [>= ([counter.get ()] 50000)] {
            let counter [[counter.increment ()]]
        }
    }]
    [thread.spawn @(:counter) {
        until [>= ([counter.get ()] 50000)] {
            let counter [[counter.increment ()]]
        }
    }]
)

List.foreach (handles thread.join)
Console.println [counter.get ()]  // 100000
```

### Implementation Shape

- Internal Rust type: `Arc<Mutex<Value>>` stored in the host arena
- `mutex.make initial-value` creates the mutex, returns a `:mutex` namespace handle
- `mutex.get` acquires the lock, clones the value, drops the lock, returns the value
- `mutex.set` acquires, replaces, drops
- `mutex.with block` — acquires the lock, binds the current value into the block's environment, executes the block, then commits the returned value back under the lock
- The mutex handle is sendable between threads (it's an `Arc` internally)

### Why This Completes the Picture

`spawn` gives you parallelism, `channel` gives you message passing, and `mutex` gives you shared-state coordination. Together they cover the three fundamental threading patterns without adding syntax — all three are native modules operating on existing Pima values and blocks.

---

## Recommended Implementation Order

1. **`/pima/thread`** — Self-contained, reuses the TCP resource pattern exactly, gives immediate utility
2. **`/pima/channel`** — Requires solving the GC-crossing problem, which teaches you what values are "transportable"
3. **`/pima/mutex`** — Builds on threads + the understanding of which values can safely cross boundaries

### Shared Design Principle

All three stay within Pima's existing model: no new syntax, no new AST nodes, no VM instruction changes. They're native modules that bridge Rust's threading primitives through the existing `NativeContext` interface — exactly how `/pima/tcp` and `/pima/io` work today.

---

## New Error Classifications

Threading introduces a new family of typed errors:

```
(:error :thread_error)
(:error :thread_error :unsendable_value)
(:error :thread_error :channel_closed)
(:error :thread_error :join_already_called)
(:error :thread_error :poisoned_mutex)
(:error :thread_error :timeout)
```

---

## Open Questions

- Should `spawn` accept annotated blocks? (Context contract validation happens at execution time on the new thread — likely yes, same semantics as `do`.)
- Should there be a thread pool (bounded concurrency) or unbounded `spawn`? (Start unbounded, add pool later.)
- How do thread-local values interact with the symbol interner? (Each thread's `VmNativeContext` has its own interner; symbols are interned locally.)
- What happens if the main thread exits while spawned threads are alive? (Abort or wait — Rust default is to print a warning. Pima should probably wait by default.)
