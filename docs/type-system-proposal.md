# Type-System Options

Status: exploratory. Typed pattern constraints are the only committed feature
in this document's subject area; their design is specified in
[`typed-pattern-constraints.md`](typed-pattern-constraints.md).

## Current model

Pima is dynamically typed. Every value exposes runtime type symbols through
`Types.of`, and `[Types.is? value :type]` tests membership in that list.

Objects may declare additional tags with an immutable public `types` member:

```pima
val Square {
    pub val types (:square :shape)
}
```

Object construction validates the `types` member, but Pima has no schemas,
subtype relationships, inferred types, casts, or compile-time field contracts.
Functions validate argument shapes through runtime patterns.

Relevant runtime structures are `Value::Namespace`, `Namespace`, `Environment`,
and `Binding`. `MakeNamespace` retains each member's source location,
visibility, and mutability, and completes the object's `this` binding after
successful construction. Ordered namespace composition selects complete
definitions before that instruction is emitted.

## Committed next step: typed patterns

A capture suffix will require a runtime type symbol:

```pima
function length (values:list) {
    List.length values
}
```

This extends existing pattern matching rather than adding static typing. See
the typed-pattern document for supported positions, error behavior, and parser
rules.

## Optional future features

These ideas are independent and have not been accepted.

### Object contracts

A named contract could describe required members, visibility, mutability, and
optional runtime type tags. Construction or an explicit check would validate an
object against that shape.

Questions that must be settled first:

- whether contracts are declarations or ordinary object values;
- whether known violations are warnings or errors;
- whether validation occurs only at construction or can be requested later;
- how ordered namespace composition exposes the final selected shape; and
- whether a contract name belongs in an object's runtime type list.

### Structural checking

A library operation could validate an arbitrary object against a descriptor at
runtime. This would support data received through imports or I/O without
requiring construction through a particular template. It should reuse the same
descriptor and error model as object contracts if both features are adopted.

### Tagged unions

A union facility could validate a tag and payload shape, then allow `match` to
check exhaustiveness when the union definition is statically known. This is a
larger change: it requires a declared variant model, construction rules,
transport support, formatter and language-server support, and a decision about
whether exhaustiveness failures are warnings or errors.

Ordinary tagged lists such as `(:ok value)` remain valid regardless of whether
unions are added.

## Constraints

Any future design must preserve these properties:

- untyped objects and existing runtime type tags continue to work;
- types describe values rather than imposing inferred binding types;
- remote transport uses explicit portable representations;
- contracts cannot introduce inheritance, implicit coercion, or hidden parent
  objects; and
- errors remain ordinary typed object values.

The language reference's excluded-functionality section remains authoritative
until a proposal is accepted and implemented.
