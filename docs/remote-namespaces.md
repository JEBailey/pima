# Remote Namespaces and Futures

Status: implemented concurrency model.

## Decision

Pima concurrency is based on namespaces instantiated in isolated worker VMs.
The creating VM receives an opaque remote namespace handle. The namespace,
environment, mutable bindings, closures, blocks, and garbage-collected object
graph remain owned by the worker that instantiated it.

The vocabulary is:

```text
new       construct a local namespace
remote    construct a remotely owned namespace
await     obtain the value of a future
```

Every public read or call through a remote handle queues a request and
immediately returns a future. There is no implicitly blocking remote member
operation. `await` is the visible synchronization point.

## Language Shape

```pima
val :worker [remote Worker]

val :status_request worker.status
val :work_request [worker.process input]

val :done [work_request.complete?]
val :status [await status_request]
val :result [await work_request]
```

Template composition has the same ordered, leftmost-wins semantics as `new`:

```pima
val :service [remote (Service Observable Restartable)]
```

`remote` accepts a statically known namespace template or template list. It
does not accept a closure or arbitrary operation. Construction waits until the
worker has either initialized successfully or reported an initialization
error; requests made after construction are asynchronous.

## Future Semantics

A remote data member read returns a future containing a transported snapshot:

```pima
val :pending service.status
val :status [await pending]
```

A remote function call transports its argument list, queues the invocation,
and returns a future:

```pima
val :pending [service.process request]
val :result [await pending]
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
val :outcome [attempt [await pending]]
```

## Remote Construction Context

Only names explicitly declared by an annotated template are captured:

```pima
val :limit 10

val :Worker @(:limit) {
    var :processed 0

    pub function :limit () limit
}

val :worker [remote Worker]
```

`limit` is resolved before the worker starts, transported by value, and
installed as an immutable worker-local binding. A caller `var` never becomes a
shared mutable cell. Mutable state declared inside `Worker` remains exclusively
owned by that worker.

Missing requirements fail with `:missing_context`. Values that cannot cross the
transport boundary fail with `:unsendable_value`.

## Ownership and Ordering

Conceptually:

```text
local VM                              worker VM
--------                              ---------
remote handle ----------------------> namespace instance
future handle <---------------------- mailbox request
                                       private mutable state
                                       public functions and values
```

Requests for one remote namespace are processed serially through its mailbox.
Pima code therefore never observes two public operations executing
concurrently against the same namespace instance. Different remote namespaces
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
    RemoteNamespaceHandle
    FutureHandle
```

Symbols travel by name because symbol IDs belong to one interpreter. A list is
transportable only when every element is transportable. Remote and future
handles are opaque synchronized identities and may themselves cross the
boundary.

The initial model rejects:

- closures and partial functions;
- bindings and mutable cells;
- blocks;
- local namespaces;
- VM-local native resources; and
- lists containing any rejected value.

Remote mutable state is never copied to the caller. A caller deliberately
requests a snapshot or invokes a remote function that mutates worker-owned
state.

## Executable Templates

The local runtime never sends a garbage-collected block pointer to another
thread. It extracts a thread-safe blueprint containing source for the selected
templates, a public-function manifest, and transportable constructor context.
The worker independently loads the blueprint and constructs the namespace in
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
part of this model. Composite mutable state belongs inside a remote namespace.
