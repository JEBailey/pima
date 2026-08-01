# Object Template Composition

Status: implemented language contract for local and remote object
construction.

## Motivation

Piman object templates already describe concrete object structure. A
template that declares a member supplies that member; a separate `require`
declaration or contract schema would duplicate the structure already expressed
by the template.

Template composition builds an object from multiple templates. A general
template supplies a complete baseline, while more-specific templates add or
override behavior and state.

```pima
val Counter {
    pub val types (:counter)

    var count 0

    pub function increment () {
        let count [+ (count 1)]
    }

    pub function get () {
        count
    }
}

val MyCounter {
    pub val types (:my_counter)

    var count 10

    pub function increment () {
        let count [+ (count 2)]
    }
}

val counter [new MyCounter Counter]
```

The resulting object contains `count` and `increment` from `MyCounter` and
`get` from `Counter`.

This is template composition rather than a separate nominal or structural type
system. Composition guarantees the resulting structure by constructing it from
the declared members of every participating template.

## Syntax

The existing single-template form remains valid:

```pima
val counter [new Counter]
```

A list operand composes two or more templates. The list may be packed
implicitly from the remaining expressions in the bracket:

```pima
val counter [new MyCounter Counter]
```

The explicit-list spelling remains equivalent because `new` is a special form
whose operand is a template collection, not a runtime call whose arguments are
being flattened:

```pima
val counter [new (MyCounter Counter)]
```

Every element must resolve to an object template. The initial implementation
should require each template to be statically known, matching the current
restriction on `new`.

## Precedence

Templates are applied from right to left. Entries nearer the beginning of the
list are more specific and take precedence over entries to their right.

```pima
new DebugCounter PersistentCounter Counter
```

Conceptually, this means:

1. Start with `Counter`.
2. Overlay `PersistentCounter`.
3. Overlay `DebugCounter`.

When multiple templates declare the same name, the leftmost declaration wins.
The winning declaration supplies the member's value, mutability, and
visibility. No conformance error is needed: an overridden declaration remains
present through its replacement.

This precedence rule intentionally places the most specific template first. It
must be documented because many general-purpose merge functions instead give
the last entry precedence.

## Construction Semantics

Composition creates one object, not a chain of objects.
Conceptually, `new (A B C)` performs the following steps:

1. Create a fresh object environment linked to the surrounding scope.
2. Combine declarations from `C`, `B`, and `A`, with later overlays replacing
   earlier declarations of the same name.
3. Discard overridden declarations without evaluating their initializers.
4. Evaluate surviving initializers in composition order, from the rightmost
   template to the leftmost template.
5. Permit a template being evaluated to read members supplied by templates to
   its right.
6. Complete all functions against the final composed object environment.
7. Validate and combine the declared object type symbols.
8. Return one completed object value.

If construction throws, the incomplete composed object is discarded under
the same rules as existing single-template construction.

Because all functions belong to the completed object, a function inherited
from a general template observes the final bindings, including bindings
overridden by a more-specific template. This provides natural behavioral
specialization without creating an inheritance hierarchy.

## Declarations and Assignment

A declaration in a more-specific template replaces a declaration with the same
name:

```pima
val StartAtTen {
    var count 10
}

val counter [new (StartAtTen Counter)]
```

Ordinary assignment may also update an inherited mutable binding during
construction:

```pima
val StartAtTen {
    let count 10
}

val counter [new (StartAtTen Counter)]
```

The second form preserves the inherited binding's visibility and mutability.
It fails under the ordinary language rules if the inherited name is missing or
immutable. Redeclaration, in contrast, replaces all binding metadata.

Private members participate in composition even though they remain inaccessible
through external member access. A more-specific declaration may replace a
private member because composition occurs before the object is published.

## Type Lists

The public immutable `types` member needs composition-specific treatment. It is
metadata contributed by every template, rather than an ordinary member for
which only the leftmost value survives.

For `new (A B C)`, concatenate type symbols in the source order `A`, `B`, `C`,
remove duplicates while retaining the first occurrence, and prepend
`:object` as usual.

```pima
val Counter {
    pub val types (:counter)
}

val Audited {
    pub val types (:audited :counter)
}

val counter [new (Audited Counter)]
[Types.of counter] // (:object :audited :counter)
```

Each contributed `types` declaration must independently obey the existing
rules: it must be `pub val`, contain only symbols, contain no duplicates, and
must not declare a fundamental type symbol.

## Errors

Composition introduces no contract-violation error family. Failures use
existing language errors:

- an unknown or dynamic template is a compiler diagnostic while composition is
  restricted to statically known templates;
- assignment to an absent or immutable inherited binding uses the existing name
  or mutability error;
- an invalid `types` declaration uses the existing type error;
- any initializer or statement that throws aborts construction normally.

## Compiler Direction

The compiler implements composition by extending `new` lowering to recognize a
list of statically known blocks. It analyzes all participating declarations,
resolves precedence, and emits one `MakeObject` for the final set of
bindings. `remote` packages those same blocks as a worker blueprint, where the
ordinary `new` lowering applies the identical rules.
No contract table, `CheckContract` instruction, runtime reflection API, or new
runtime value category is required.

The lowering preserves initializer side effects for surviving declarations and
right-to-left visibility. Overridden declarations are definitions that do not
belong to the merged object, so their initializers do not execute.

## Non-goals

This proposal does not introduce:

- abstract members or `require` declarations;
- an `as` conformance operator;
- static type inference or typed bindings;
- nominal subtyping or an inheritance hierarchy;
- runtime merging of already-instantiated objects;
- dynamically selected templates in the initial implementation.

Those features can be considered separately if concrete use cases remain after
template composition is available.
