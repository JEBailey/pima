# Remote Objects and Futures

Status: implemented concurrency model.

## Decision

Pima concurrency is based on objects instantiated in isolated worker VMs.
The creating VM receives an opaque remote object handle. The object,
environment, mutable bindings, closures, blocks, and garbage-collected object
graph remain owned by the worker that instantiated it.

The vocabulary is:

```text
new       construct a local object
remote    construct a remotely owned object
await     obtain the value of a future
```

Every public read or call through a remote handle queues a request and
immediately returns a future. There is no implicitly blocking remote member
operation. `await` is the visible synchronization point.

## Language Shape

```pima
val worker [remote Worker]

val status_request worker.status
val work_request [worker.process input]

val done [work_request.complete?]
val status [await status_request]
val result [await work_request]
```

Ordered namespace composition has the same complete-definition,
leftmost-wins semantics as local `new`:

```pima
val service [remote (Service Observable Restartable)]
```

`remote` accepts a statically known object template or template list. It
does not accept a closure or arbitrary operation. Construction waits until the
worker has either initialized successfully or reported an initialization
error; requests made after construction are asynchronous.

## Future Semantics

A remote data member read returns a future containing a transported snapshot:

```pima
val pending service.status
val status [await pending]
```

A remote function call transports its argument list, queues the invocation,
and returns a future:

```pima
val pending [service.process request]
val result [await pending]
```

Futures are immutable handles with one directly callable member:

```pima
[pending.complete?]
```

`complete?` reports whether a value or error is available without waiting.
`await pending` waits when necessary and then returns the transported value.
Awaiting the same completed future repeatedly returns the same value; it does
not consume the result.

If the remote operation throws, the future stores a transportable error record.
`await` reconstructs and throws the typed error in the waiting VM, so ordinary
error handling remains valid:

```pima
val outcome [attempt [await pending]]
```

## Remote Construction Context

Only names explicitly declared by an annotated template are captured:

```pima
val limit 10
val workload (1 2 3)
val service [remote Service]

val Worker @(limit *workload &service) {
    var processed 0

    pub function limit () limit
}

val worker [remote Worker]
```

Bare requirements such as `limit` are resolved before the worker starts,
transported by value, and installed as immutable worker-local bindings. A
`*workload` requirement uses the same transport representation but, after
successful worker creation, replaces the caller's shared location with an
`(:error :move_error :moved_value)` value. Every reference-like alias observes
the same replacement. The error records the remote-construction operation and
source span for diagnostics and logs. Failed construction does not consume
the binding. An `&service` requirement accepts only an existing remote-object
or future handle and preserves that synchronized identity in the worker. TCP
listener handles are also shareable, enabling multiple isolated workers to
block in `accept` on one listening socket.

A caller `var` never becomes a shared mutable cell. Mutable state declared
inside `Worker` remains exclusively owned by that worker.

Missing requirements fail with `:missing_context`. Bare requirements use
`Value.copy` and reject errors or identity-bearing values with
`:copy_error :uncopyable_value`. Invalid moves and shares fail with
`:unsendable_value`.

Move does not recursively serialize a local object graph. Local objects,
closures, bound methods, code blocks, binding cells, and TCP connections remain
owned by their VM. Encountering any of them—including inside a persistent
list—fails the complete worker construction transaction and preserves every
source alias. Worker-local graphs must be constructed inside the worker from
transported data snapshots. Remote objects, futures, and TCP listeners are
handle identities rather than serialized local graphs; use `&` where sharing
is supported.

## Ownership and Ordering

Conceptually:

```text
local VM                              worker VM
--------                              ---------
remote handle ----------------------> object instance
future handle <---------------------- mailbox request
                                       private mutable state
                                       public functions and values
```

Requests for one remote object are processed serially through its mailbox.
Pima code therefore never observes two public operations executing
concurrently against the same object instance. Different remote objects
may execute concurrently.

The implementation may currently use one OS thread per worker VM. The language
only promises an isolated execution context; a later runtime may schedule
multiple remote VMs over a bounded worker pool without changing source syntax.

## Transport Boundary

Values cross workers as independent transport representations:

```text
TransportValue
    Unit
    Boolean
    Integer
    Float
    String
    SymbolName
    List<TransportValue>
    RemoteObjectHandle
    FutureHandle
```

Symbols travel by name because symbol IDs belong to one interpreter. A list is
transportable only when every element is transportable. Remote and future
handles are opaque synchronized identities and cross the boundary only through
explicit `&` sharing or `*` transfer.

Bare and `*` requirements use this representation as copy and move policies,
respectively. `&` accepts only the
opaque synchronized handle variants. Worker interpreters participating in a
share use the same concurrency hub, allowing the shared handle to be used from
either worker without sharing either VM heap.

The initial model rejects:

- closures and partial functions;
- bindings and mutable cells;
- blocks;
- local objects;
- VM-local native resources; and
- lists containing any rejected value.

Remote mutable state is never copied to the caller. A caller requests a
snapshot or invokes a remote function that mutates worker-owned
state.

## Executable Templates

The local runtime never sends a garbage-collected block pointer to another
thread. It extracts a thread-safe blueprint containing source for the selected
templates, a public-function manifest, and transportable constructor context.
The worker independently loads the blueprint and constructs the object in
its own VM and heap.

The public-function manifest lets `service.process` produce a callable proxy
without an extra lookup request. Calling that proxy still returns a future.
Reading a public data member immediately queues a read request and returns its
future. The worker remains the source of truth and validates visibility.

## Errors

A worker converts a thrown Pima value into a transport error containing its
type-symbol names and message. `await` reconstructs that error in the waiting
VM. Required classifications include:

```pima
(:error :remote_error)
(:error :remote_error :unsendable_value)
(:error :remote_error :unknown_member)
(:error :remote_error :stopped)
(:error :remote_error :worker_failure)
```

A Rust panic must never cross the boundary and instead becomes
`:worker_failure` with safe diagnostic metadata.

## Lifecycle

The current lifecycle surface is provided by `/pima/remote`:

```pima
[Remote.alive? worker]
[Remote.stop worker]
```

`stop` requests orderly mailbox shutdown. New requests fail with `:stopped`.
Structured ownership, cancellation, and a public worker-shutdown awaitable may
be added later.

## Future Direction

Likely extensions are:

- structured remote ownership and cancellation;
- public channels using the same transport representation;
- richer transported stack metadata;
- bounded worker scheduling; and
- nonblocking host I/O integration.

General shared-memory mutex protection and arbitrary closure scheduling are not
part of this model. Composite mutable state belongs inside a remote object.
