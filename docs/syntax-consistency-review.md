# Syntax Consistency Review

Status: design review for reference. Always-list call packing, explicit symbolic
binding destinations, and unified function/match patterns are implemented.

## Purpose

Pima is converging on a prefix language organized around names, symbols, lists,
blocks, and line-oriented invocation. Several older conveniences now overlap or
change meaning by context. This document identifies those inconsistencies and
proposes a smaller set of universal syntax rules.

## Recommended Core Model

The language should commit to these principles:

1. Bare names represent bindings.
2. `:name` always represents a literal symbol.
3. Every invocation receives one argument list.
4. A line or bracket implicitly constructs the outer argument list.
5. Parentheses explicitly delimit nested lists or list patterns.
6. Reserved forms are syntax, not simulated runtime functions.
7. Brackets delimit a nested invocation; they do not select another invocation
   mode.

For example:

```pima
val :result [divide 10 2]

match result (
    (:ok value) { value }
    (:error message) { Console.println message }
)
```

Conceptually:

- `val` receives the literal destination `:result` and its initializer;
- `divide` receives the argument list `(10 2)`;
- `match` receives syntax describing its subject and arms;
- `:ok` and `:error` are literal symbols;
- `value` and `message` are pattern captures.

## Call Packing

### Implemented behavior

Every invocation should receive an implicitly constructed outer list:

```text
[f]       -> ()
[f x]     -> (x)
[f x y]   -> (x y)
[f (x y)] -> ((x y))
[f ((x y))] -> (((x y)))
```

Function parameter patterns then consistently describe that argument list:

```pima
function :square (value) {
    * value value
}
```

A bare parameter remains useful for capturing the complete argument list:

```pima
function :arguments values {
    values
}

[arguments 1 2 3] // (1 2 3)
```

Every pair of parentheses adds a list layer. A list can therefore be supplied
as one operand directly:

```pima
function :first (values) {
    List.head values
}

[first (1 2 3)]
```

Here `first` receives `((1 2 3))`, and its pattern captures the inner list as
`values`. By contrast, `[first 1 2 3]` receives `(1 2 3)` and attempts to match
three arguments against a one-element parameter pattern.

## Names and Symbols

The intended universal distinction is:

```pima
foo       // resolve or introduce the binding named foo
:foo      // the literal symbol :foo
```

In expression position:

```pima
val :foo 42

foo       // 42
:foo      // :foo
```

An unbound bare name is an error, while a symbol literal is always a valid
value.

### Match patterns

Match now follows the intended rule:

```pima
match result (
    (:ok value) { value }
    (:error message) { Console.println message }
)
```

- `:ok` and `:error` match literal symbols;
- `value` and `message` introduce captures;
- `_` remains a wildcard.

### Binding destinations and function patterns

Declarations and assignment require literal symbol destinations:

```pima
val (:left :right) pair
var :count 0
let :count 1
```

This makes the operation explicit: the form changes the binding named by the
symbol rather than resolving that name as an expression. Function parameters,
by contrast, use the same pattern language as `match`:

```pima
function :add (left right) {
    + left right
}
```

A symbol in a function parameter is therefore a literal constraint:

```pima
function :handle (:get path) {
    ...
}
```

This function would accept only argument lists beginning with `:get`.

Annotated block requirements remain reasonable:

```pima
@(:name :score) { ... }
```

These are explicitly symbolic name descriptors rather than binding patterns.

## Brackets and Invocation

The AST currently distinguishes calls with an `immediate` Boolean, but both
line calls and bracketed calls execute normally. The useful distinction is only
their syntactic boundary:

```pima
foo 1 2
```

is a line invocation, while:

```pima
val :result [foo 1 2]
```

uses brackets to delimit an invocation inside another form.

Recommended changes:

- call `[...]` a bracketed or nested invocation;
- remove the unused `immediate` field from call AST nodes;
- do not describe brackets as selecting another evaluation mode.

Outside brackets, a bare function name evaluates to the function value. Inside
brackets, a callee with no operands receives the empty argument list:

```pima
val :operation calculate
[operation]
```

## Reserved Forms and Operand Boundaries

Reserved forms currently consume operands using different parser rules:

- `new` and `do` consume through a line or bracket boundary;
- `return` and `break` consume zero or one expression, while `throw` consumes
  exactly one expression;
- `val`, `var`, and `let` consume a pattern and one expression;
- `function` consumes a name, parameter pattern, and body expression;
- `if` consumes two or three expressions;
- `while` and `until` consume a condition and body expression;
- `attempt` consumes one protected expression;
- `match` and `branch` assign structural meaning to parentheses.

Some variation is necessary because these are syntax forms, but no form
reinterprets or greedily consumes the rest of a physical line. Each consumes
its documented expression operands and rejects trailing operands.

The parser should have an explicit concept of the current expression boundary:

```text
physical line ending
]
)
}
```

A prefix form should consume within its containing boundary. This is especially
important for forms such as `new` and `do` nested inside lists or other forms.

## Special Forms Are Not Ordinary Calls

The conceptual model of a prefix operation receiving a list is useful, but
fully parenthesized special forms are ambiguous with patterns.

For example:

```pima
val (:left :right) pair
```

uses `(left right)` as a destructuring pattern. If an opening parenthesis after
`val` could instead delimit all of `val`'s operands, the same syntax would have
two incompatible parses.

A fully wrapped form would require another layer:

```pima
val ((:left :right) pair)
```

Supporting both dialects would complicate the grammar without adding runtime
capability. The recommended rule is therefore:

- reserved forms have documented, fixed syntactic operands;
- their prefix layout resembles invocation but is not runtime invocation;
- fully parenthesized special forms are a conceptual representation, not an
  alternate source spelling.

The normal source form remains:

```pima
val :Counter { ... }
```

## Parentheses

Parentheses currently delimit several kinds of list-shaped structure:

- runtime list values;
- binding list patterns;
- match list patterns;
- match-arm collections;
- branch condition/result collections;
- annotated-block requirement lists.

Saying that parentheses always construct a runtime list is therefore
incomplete. A more accurate rule is:

> Parentheses delimit list-shaped structures. In expression position they
> construct a list value; in pattern and special-form positions they delimit
> structural syntax.

This is compatible with the language's prefix character and explains the
existing uses without claiming that match arms or binding patterns are runtime
values.

## Blocks

Braces consistently construct inert blocks:

```pima
{ ... }
```

Body positions accept any expression. When that expression is a block, the
surrounding form executes its statements in the supplied environment:

```pima
if condition { ... }
while condition { ... }
attempt { ... }
new { ... }
do block
```

Blocks are therefore a convenience for multiple expressions, not a body
requirement. A single expression may be used directly in functions,
conditionals, loops, match arms, branches, and `attempt`.

The documentation should preserve this distinction: braces do not execute
their contents. The surrounding form executes the resulting block when its
semantics require it.

Functions and blocks retain different roles:

- functions capture lexical bindings and are invoked with an argument list;
- blocks are inert code and are executed against a supplied environment.

## Selection Forms

The three selection forms belong to one family but retain distinct semantics:

```text
if       one Boolean decision
branch   ordered Boolean decisions
match    one structural pattern decision
```

`if` evaluates one predicate and selects its consequent or optional
alternative. A false predicate without an alternative produces unit.

`branch` evaluates condition/result pairs from top to bottom and selects the
first true condition. If none is true, it produces unit. A final `true` pair is
the explicit default.

`match` evaluates one subject and tests patterns from top to bottom. Patterns
are structural syntax and are not evaluated as Boolean expressions. If no
pattern matches, it throws `:match_error`; `_` is the explicit fallback.

Combining `if` and `branch` would give one keyword incompatible operand shapes.
Combining `branch` and `match` would make an arm's left side switch between an
evaluated condition and an unevaluated pattern based on context. Keeping all
three preserves a visible evaluation rule while sharing these conventions:

- only the selected result is evaluated;
- results may be single expressions or blocks;
- alternatives or arms are considered in source order; and
- blocks provide multiple expressions but are never required.

## Infix Exceptions

Imports use `as` in an otherwise prefix-oriented language:

```pima
import "/module" as :module
```

This should remain a deliberate grammar exception. Import paths and aliases are
declarative syntax, and a prefix alternative would be less readable. `as`
should remain specific to imports rather than becoming a general composition or
conversion operator.

Namespace-template composition should use prefix argument order:

```pima
[new Specialized Base]
```

rather than an infix form such as:

```pima
[new Specialized as Base]
```

## Recommended Migration Order

1. **Completed:** Make every call receive an implicit outer list, including
   singleton calls.
2. **Completed:** Require literal symbols for binding and assignment targets.
3. **Completed:** Use the same literal/capture patterns in functions and
   `match`.
4. Remove the unused `immediate` field from call AST nodes.
5. Introduce delimiter-aware expression boundaries in the parser.
6. Clarify in the specification that reserved forms have fixed syntax rather
   than being ordinary functions.
7. **Completed:** Update examples so every explicit pair of parentheses adds a
   list layer, retaining parentheses only when a list value is an operand.
8. Implement namespace-template composition after the parsing rules settle.

## Target Syntax Summary

```text
foo       binding lookup or capture
:foo      literal symbol in every context
(...)     explicit list-shaped structure
{...}     inert block
[...]     nested invocation boundary

head operand...   invoke head with the argument list (operand...)
```

This model gives Pima one consistent explanation for name lookup, symbolic
data, patterns, calls, and nested expression boundaries.

## Required Future Constraint Syntax

Pima must add typed pattern suffixes such as `value:list` and
`:value:list`. They constrain captures and binding destinations using the
value's existing list of type symbols; they do not introduce static typing or
general expression syntax. The normative future design is recorded in
[`typed-pattern-constraints.md`](typed-pattern-constraints.md).
