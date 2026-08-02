# Ordered Namespace Composition

Status: implemented language contract for local and remote object
construction.

## Motivation

Pima object templates already describe concrete object structure. A
template that declares a member supplies that member; a separate `require`
declaration or contract schema would duplicate the structure already expressed
by the template.

Ordered namespace composition builds one fresh object namespace from multiple
code-block templates. Templates contribute definitions; they are not objects
being linked together and do not remain as hidden runtime instances.

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

This is definition selection followed by namespace construction, rather than a
nominal or structural type system. Pima determines the surviving definitions
first and then executes only those definitions to create one namespace.

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

Definitions are selected with leftmost precedence. Entries nearer the beginning
of the list take precedence over entries to their right.

```pima
new DebugCounter PersistentCounter Counter
```

Conceptually, Pima scans the templates from left to right and retains the first
complete definition of each member name.

When multiple templates declare the same name, the leftmost declaration wins.
The winning declaration supplies the member's value, mutability, visibility,
and declaration kind. No runtime override chain remains: a losing definition
is absent from the new namespace and is never executed.

This precedence rule intentionally places the most specific template first. It
must be documented because many general-purpose merge functions instead give
the last entry precedence.

## Construction Semantics

Composition creates one object, not a chain, hierarchy, or collection of
partially hidden objects.
Conceptually, `new (A B C)` performs the following steps:

1. Read the definitions contributed by `A`, `B`, and `C`.
2. Select complete surviving definitions with leftmost precedence.
3. Create one fresh object environment linked to the surrounding scope.
4. Evaluate surviving definitions in execution order, from the rightmost
   template to the leftmost template.
5. Permit a template being evaluated to read members supplied by templates to
   its right.
6. Complete all functions against the final composed object environment.
7. Validate and combine the declared object type symbols.
8. Return one completed object value.

If construction throws, the incomplete composed object is discarded under
the same rules as existing single-template construction.

Because all functions belong to the one completed object, a function supplied
by any template observes the final namespace, including definitions supplied
by other templates. Every method's `this` is the same completed object. There
are no contributing template instances, parent lookup, runtime override chain,
or `super` object.

## Declarations and Assignment

A declaration in a more-specific template replaces a declaration with the same
name:

```pima
val StartAtTen {
    var count 10
}

val counter [new (StartAtTen Counter)]
```

Ordinary assignment may also update a mutable binding selected from another
template during
construction:

```pima
val StartAtTen {
    let count 10
}

val counter [new (StartAtTen Counter)]
```

The second form preserves the selected binding's visibility and mutability.
It fails under the ordinary language rules if the selected name is missing or
immutable. Redeclaration, in contrast, replaces all binding metadata.

Private members participate in composition even though they remain inaccessible
through external member access. A more-specific declaration may replace a
private member because composition occurs before the object is published.

A destructuring binding is one definition operation even though it introduces
several names. Composition cannot select only some of its captures. If member
precedence would split a destructuring declaration, compilation fails instead
of synthesizing a declaration the programmer did not write.

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
- assignment to an absent or immutable selected binding uses the existing name
  or mutability error;
- an invalid `types` declaration uses the existing type error;
- any initializer or statement that throws aborts construction normally.

## Compiler Direction

The compiler implements ordered namespace composition in `new` lowering for a
list of statically known blocks. It analyzes all participating declarations,
selects complete definitions, and emits one internal `MakeNamespace`
instruction for the final bindings. `remote` packages those same blocks as a
worker blueprint, where ordinary `new` lowering applies identical rules.
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
- nominal subtyping, inheritance, parent objects, or `super`;
- runtime merging of already-instantiated objects;
- dynamically selected templates in the initial implementation.

Those features can be considered separately if concrete use cases remain after
ordered namespace composition is available.
