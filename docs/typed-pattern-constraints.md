# Typed Pattern Constraints

Status: required future language feature. This is part of Pima's intended
language design, although it is not yet implemented.

## Purpose

Pima must support minimal runtime type constraints without introducing static
typing, type inference, casts, or a second type system. A constraint reuses the
existing list of type symbols carried by every value.

The required syntax is a type-symbol suffix on a capture or binding
destination:

```pima
value:list
```

This means: capture the value as `value`, but accept it only when its type list
contains `:list`.

The two parts retain their existing meanings:

```text
value   capture or resolved binding name
:list   literal type symbol used as a constraint
```

Adjacency composes them into a constrained pattern; it does not change the
meaning of `:` elsewhere.

## Required Uses

Function parameter patterns:

```pima
function head (values:list) {
    List.head values
}
```

Match patterns:

```pima
match input (
    value:string [process_text value]
    value:list [process_items value]
    value [process_other value]
)
```

Nested list patterns:

```pima
function render ((name:string score:integer)) {
    render_score name score
}
```

Binding and assignment destinations:

```pima
val names:list ("Ada" "Grace")
var count:integer 0
let count:integer 1
```

In a binding destination, the bare name identifies the destination and the
suffix names the symbol that constrains the assigned value:

```text
name:type
```

## Semantics

`value:list` succeeds exactly when the value's existing type list contains
`:list`. It is conceptually equivalent to testing:

```pima
[Types.is? value :list]
```

The mechanism must work for fundamental types and object-defined semantic
types:

```pima
count:integer
account:account
error:validation_error
shape:shape
```

It must not introduce a separate registry or hierarchy. An object satisfies
a constraint when the requested symbol occurs in the object's normal type
list.

## Pattern Behavior

A constraint is part of pattern matching:

- In `match`, a failed constraint rejects that arm and matching continues.
- In a function parameter, a failed constraint produces the existing argument
  pattern mismatch.
- In `val` or `var`, failure prevents every declaration in the pattern.
- In `let`, the entire pattern is validated before any mutation, preserving
  atomic assignment.

Captures remain immutable within function and match scopes. A successful
constraint does not convert or cast the value.

## Scope

The first implementation must support exactly one type suffix per capture or
destination:

```pima
value:list
```

Repeated constraints such as `value:object:account` are deferred. They may
later mean that every listed type symbol is required, but that behavior is not
part of the initial requirement.

The suffix is pattern syntax only. It must not become a general expression:

```pima
value:list // invalid outside a pattern or binding destination
```

Runtime type questions continue to use `Types.is?`. This avoids ambiguity
between a constraint, assertion, cast, and Boolean type test.

## Non-goals

Typed pattern constraints do not add:

- static typing or inference;
- compile-time proof of types;
- implicit conversion or casting;
- generic types;
- subtype declarations; or
- required object fields.

They are a minimal constraint layer over Pima's existing dynamic type lists.

## Implementation Requirement

This feature should extend the shared pattern representation used by function
parameters and `match`, plus the binding-pattern representation used by
`val`, `var`, and `let`. Constraint validation should reuse the runtime type
membership operation so all pattern contexts have identical semantics.
