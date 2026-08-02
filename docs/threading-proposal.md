# Concurrency Design History

Status: superseded by the implemented model in
[`remote-objects.md`](remote-objects.md).

This file records the alternatives considered before Pima selected isolated
remote objects. It is not a description of current syntax or runtime modules.

## Implemented decision

Pima concurrency uses `remote`, `await`, and annotated block requirements:

```pima
val Worker @(configuration *workload &service) {
    pub function process () workload
}

val worker [remote Worker]
val result [await worker.process]
```

- Every remote object owns an isolated VM and heap.
- Public reads and calls return repeatable future handles.
- `await` is the explicit synchronization point.
- Bare requirements copy transportable values into immutable worker bindings.
- `*name` moves a transportable value and invalidates its shared caller-side
  location only after construction succeeds.
- `&name` shares supported synchronized handles: remote objects, futures, and
  TCP listeners.
- Local objects, closures, blocks, binding cells, and TCP connections do not
  cross the worker boundary.
- Requests to one remote object are serialized; separate remote objects may run
  concurrently.

Moved locations become provenance-carrying
`(:error :move_error :moved_value)` values. Failed transport or construction is
transactional and leaves every caller-side location unchanged.

## Rejected earlier shape

The initial exploration proposed `/pima/thread`, `/pima/channel`, and
`/pima/mutex` native modules. Those modules do not exist. That design was
rejected because passing executable GC-backed values directly to OS threads
would blur VM ownership, while a general mutex would introduce shared mutable
Pima heap state.

The selected remote-object model keeps heaps isolated and transports only an
explicit value representation or synchronized opaque handles. Channels remain
an implementation detail of remote handles and futures rather than a public
language abstraction. A future runtime may schedule isolated workers on a pool
without changing Pima source semantics.

See [`remote-objects.md`](remote-objects.md) for normative concurrency behavior
and [`language-reference.md`](language-reference.md#13-remote-object-construction)
for syntax.
