# Pima Language Reference

Status: draft normative specification, derived from the programs in `examples/`.

This document defines the Pima implementation target. Java interoperability is
not part of the language. In particular, `examples/java_support.pima` is excluded
from conformance.

## 1. Language model

Pima is a dynamically typed, expression-oriented language. Calls use prefix
notation, functions are first-class values, and code blocks are first-class,
uninstantiated chunks of code that may be stored and passed around.

An implementation conforms to this specification when it can parse and execute
all example programs other than `java_support.pima`, subject to the standard
library requirements in section 10.

## 2. Source text

Source text is UTF-8. Spaces and tabs separate tokens but are otherwise
insignificant. A line ending normally terminates the current statement.

Line endings inside `[]` and `()` do not terminate the surrounding statement.
Braces follow the continuation rule below:

- If `{` is the first non-whitespace token on its physical line, it begins a
  standalone block. The block may contain any number of line-terminated
  statements.
- If `{` is not the first non-whitespace token on its physical line, the block
  is inline with the statement containing `{`. The complete balanced block,
  including all of its lines and statements, acts as one operand on that
  statement's logical line. A physical line ending inside the block does not
  terminate the containing statement.
- After an inline block's closing `}`, the containing statement may continue.
  This permits the alternative block in `if predicate { ... } { ... }`.

Thus a block may always contain multiple lines and commands. “Inline” describes
the block's relationship to its containing statement; it does not require the
block's contents to occupy one physical line.

For example, the following `if` is one logical statement even though each branch
contains several statements:

```pima
if ready {
    println "starting"
    run task
} {
    println "not ready"
    record failure
}
```

Here the first `{` follows other tokens, so its balanced contents continue the
`if` statement. The second `{` likewise continues that same statement after the
first block closes.

Pima supports two kinds of comments:

```pima
// line comment

/*
   block comment
*/
```

A block comment ends at the next `*/`. Block comments need not nest.

Identifiers are case-sensitive. An identifier may contain letters, decimal
digits, `_`, or operator punctuation. It must not be mistaken for a number,
string, delimiter, or reserved word. Examples include:

```text
fibonacci  good_enough  empty?  <=  ..
```

A period directly between identifier tokens is the namespace member operator,
not part of either identifier. Whitespace is not permitted around this
operator. A punctuation-only operator such as `..` remains an identifier when
it appears as its own token.

A colon immediately followed by an identifier forms a symbol literal:

```pima
:x  :item  :good_enough
```

The colon is not part of the symbol's name. A symbol denotes a name without
resolving that name as a binding.

The reserved words are:

```text
as  attempt  break  continue  do  function  if  import  let  match  new  pub
return  set  throw  until  var  while
```

## 3. Grammar

The following grammar is written in EBNF. `NL` is a physical line ending and
`logical-NL` is a line ending that is not suppressed by a balanced delimiter or
the inline-block continuation rule.

```ebnf
program          = layout*, [ statement-list ], layout* ;

statement-list   = statement, { terminator+, statement } ;
statement        = expression ;
terminator       = logical-NL ;
layout           = horizontal-space | NL | comment ;
separator        = horizontal-space | suppressed-NL | comment ;

expression       = literal
                 | symbol
                 | identifier
                 | member-access
                 | list
                 | block
                 | annotated-block
                 | bracket-expression
                 | declaration
                 | assignment
                 | match-expression
                 | conditional
                 | loop
                 | control-transfer
                 | attempt-expression
                 | import-expression
                 | new-expression
                 | do-expression
                 | call ;

literal          = number | string | "true" | "false" ;
symbol           = ":", identifier-name ;
number           = [ "-" ], digit, { digit }, [ ".", digit, { digit } ] ;
string           = '"', { string-character | escape }, '"' ;
escape           = "\", ( '"' | "\" | "n" | "r" | "t" | "0"
                 | "u{", hex-digit, { hex-digit }, "}" ) ;

list             = "(", separator*, [ expression-list ], separator*, ")" ;
block            = "{", layout*, [ statement-list ], layout*, "}" ;
annotated-block  = "@", separator*, context-requirements, separator*, block ;
context-requirements
                 = "(", separator*,
                   [ symbol, { separator+, symbol } ],
                   separator*, ")" ;
bracket-expression
                 = "[", separator*, expression-list, separator*, "]" ;
expression-list  = expression, { separator+, expression } ;

declaration      = [ visibility, separator+ ], ( binding
                 | function-declaration ) ;
visibility       = "pub" ;
binding          = ( "set" | "var" ), separator+,
                   binding-pattern, separator+, expression ;
assignment       = "let", separator+, binding-pattern, separator+, expression ;
binding-pattern  = binding-name | "_" | binding-list-pattern ;
binding-name     = identifier | symbol ;
binding-list-pattern
                 = "(", separator*, [ binding-pattern,
                   { separator+, binding-pattern } ], separator*, ")" ;
match-pattern    = identifier | symbol | literal | "_" | match-list-pattern ;
match-list-pattern
                 = "(", separator*, [ match-pattern,
                   { separator+, match-pattern } ], separator*, ")" ;
function-declaration
                 = "function", separator+, identifier, separator*,
                   parameter-list, separator*, block ;
member-access    = identifier, ".", identifier, { ".", identifier } ;
parameter-list   = "(", separator*,
                   [ symbol, { separator+, symbol } ],
                   separator*, ")" ;

conditional      = "if", separator+, expression, separator+,
                   expression, [ separator+, expression ] ;
loop             = ( "while" | "until" ), separator+,
                   expression, separator+, block ;
control-transfer = return-expression
                 | break-expression
                 | "continue"
                 | throw-expression ;
return-expression
                 = "return", [ separator+, expression ] ;
break-expression = "break", [ separator+, expression ] ;
throw-expression = "throw", separator+, expression ;
attempt-expression
                 = "attempt", separator+, block ;
match-expression = "match", separator+, expression, separator*,
                   "(", separator*, match-arm,
                   { separator*, match-arm }, separator*, ")" ;
match-arm        = match-pattern, separator*, block ;
import-expression
                 = "import", separator+, ( string | import-path ),
                   [ separator+, "as", separator+, identifier ] ;
new-expression   = "new", separator+, expression ;
do-expression    = "do", separator+, expression ;

call             = expression, { separator+, expression } ;
```

At statement level, the first expression is the callee and the remaining
expressions on the same logical line are its arguments. The call ends at the
logical line ending; parsing does not depend on the callee's runtime arity. A
bracket expression explicitly invokes its first expression with the remaining
expressions as arguments and provides a boundary when a call is nested:

```pima
println [fibonacci 12]
+ [fibonacci 5] [fibonacci 6]
```

Built-ins such as `println` receive every remaining operand on their logical
line. User functions must receive the arity declared by their parameter list.
Except for syntax defined as a special form, an unbracketed nested call is not
permitted. `[function]` immediately invokes `function` with zero arguments.

Parentheses construct lists; they do not group arithmetic expressions.
Brackets immediately invoke a call expression. Braces create a block value and
do not execute its body merely by being encountered.

`@` is a block-construction special form with two syntactic operands: a literal
list of required context symbols and a block body:

```pima
@(:name :score) {
    Console.println name score
}
```

The result is a block value, not a function.

## 4. Values

Every runtime value belongs to one of these categories:

- Boolean: `true` or `false`
- Integer: signed 64-bit
- Float: IEEE 754 binary64
- String
- Symbol
- List
- Function, including closures and partially applied functions
- Code block (uninstantiated code)
- Namespace
- Unit

The empty list is written `()` and has type `:list`. Unit is a distinct value
with type `:unit`; it is returned by constructs that complete without producing
another value. Unit has no source literal in this language version.

Strings are immutable UTF-8 values. Their logical elements are Unicode scalar
values rather than encoded bytes.

### 4.1 Runtime types

Every value has an immutable, non-empty list of type symbols. The native
`types` function returns that list:

```pima
[types 42]          // (:integer)
[types 4.2]         // (:float)
[types "hello"]     // (:string)
[types :name]       // (:symbol)
[types ()]          // (:list)
```

The fundamental runtime type is always the first symbol. The fundamental type
symbols are:

```text
:unit  :boolean  :integer  :float  :string
:symbol  :list  :function  :block  :namespace
```

The native predicate `is? value type-symbol` reports whether `type-symbol`
occurs in the value's type list:

```pima
[is? 42 :integer]   // true
[is? 42 :string]    // false
```

The second operand to `is?` must be a symbol. Type lists contain symbols only,
contain no duplicates, and cannot be modified.

Types belong to values rather than bindings. Bindings and parameters do not
have declared types, and a mutable binding may be assigned values of different
types. Pima is dynamically and strongly typed: native operations validate their
operands and do not implicitly coerce values, except for integer-to-float
promotion in mixed numeric operations.

Conditions for `if`, `while`, and `until` must evaluate to `:boolean`. Pima does
not use general truthiness. The `not` function likewise accepts only a Boolean.

### 4.2 Namespace type symbols

A namespace template may declare additional semantic types with an immutable,
public `types` member:

```pima
set Square {
    pub set types (:square :shape)
}

set square [new Square]
[types square]          // (:namespace :square :shape)
[is? square :shape]     // true
```

For a namespace, the runtime prepends `:namespace` to the declared type list.
The declared list may contain application-defined symbols but cannot contain a
fundamental runtime type symbol. `new` validates the member when present:
`types` must be declared with `pub set`, its value must be a list of unique
symbols, and it cannot be reassigned. A namespace without this member has the
type list `(:namespace)`.

## 5. Evaluation

Literals and symbols evaluate to themselves. An identifier evaluates to the
value in its nearest lexical binding. Symbols are immutable and compare by
their name.

A list evaluates its elements from left to right and produces a new immutable
list.

A bracket expression evaluates its callee and arguments from left to right,
immediately invokes the callee, and yields the result:

```pima
[* 6 7]                 // 42
[produce_value]         // zero-argument invocation
```

A block literal evaluates to an inert code-block value. Creating or passing a
code block does not execute it, create a scope, bind its free identifiers, or
capture an environment. A code block becomes instantiated only when an
operation such as `do`, `attempt`, or `new` supplies an execution
environment.

An annotated block declares context bindings that must be visible when `do`
executes it:

```pima
set report @(:name :score) {
    Console.println name score
}
```

The annotation does not create bindings and does not capture their current
values. Each symbol must be unique and cannot be a reserved word. The first
version treats the annotation as a guaranteed minimum contract: other free
identifiers may still resolve normally from the execution environment.

When an instantiated block executes, its statements run from left to right and
its final value is returned. An empty block returns unit.

Booleans, calls, and blocks are all expressions and may occupy the same
expression positions:

```pima
true
[foo bar]
{
    foo 2
    bar 3 x
}
```

They do not necessarily produce the same kind of value: a block literal
produces an inert `:block` value until a block-aware form supplies an execution
environment. When such a form executes the block, the block's final statement
is its resulting value. This lets a multi-statement block serve anywhere that
form would otherwise accept a single result-producing expression.

Calls evaluate the callee and ordinary arguments from left to right, then invoke
the callee. The special forms `if`, `while`, `until`, `function`, `set`, `let`,
`var`, `pub`, `import`, `new`, `do`, `attempt`, `return`, `break`,
`continue`, and `throw` control evaluation of one or more operands.

Operations fail by throwing typed error values. Error conditions include:

- resolving an unbound identifier;
- calling a value that is not callable;
- supplying the wrong number of arguments;
- applying an operation to unsupported value types;
- applying `head` or `rest` to an empty list; and
- importing a source file that cannot be found or parsed.

### 5.1 Errors

Every error is a namespace classified with at least `:error`:

```pima
set InvalidOrder {
    pub set types (:error :validation_error :invalid_order)
    pub set message "The order is invalid"
    pub set order_id 42
}

throw [new InvalidOrder]
```

Consequently, all errors satisfy both:

```pima
[is? error :namespace]
[is? error :error]
```

An error namespace must expose an immutable public `message` string. It may
expose any additional immutable application data and may add progressively
specific type symbols. Custom error types require no global registration.

`throw value` immediately stops normal evaluation and begins unwinding Pima
function calls. Its operand must be classified as `:error`; throwing any other
value produces a `:type_error`. Native failures create and throw error
namespaces through this same mechanism; a host-language panic must never escape
as a Pima error.

Errors remain ordinary values. A function may return an error namespace when an
error is an expected alternative that its caller should inspect. Returning an
error does not automatically propagate it. `throw` is the explicit request for
automatic propagation.

The standard native error classifications are:

```text
(:error :syntax_error)
(:error :name_error)
(:error :type_error)
(:error :value_error)
(:error :arity_error)
(:error :mutation_error)
(:error :match_error)
(:error :visibility_error)
(:error :import_error)
(:error :numeric_error)
(:error :control_flow_error)
(:error :index_error)
(:error :conversion_error)
(:error :io_error)
```

When an error is thrown, the runtime attaches its source file, line, column, and
Pima call stack for diagnostic reporting. This metadata does not alter the
error's declared type list.

`attempt block` instantiates the block in the caller's current environment and
executes it:

```pima
set result [attempt {
    read_file path
}]

if [is? result :error] {
    println result.message
} {
    process result
}
```

- If the block completes normally, `attempt` returns its result.
- If evaluation throws an error, `attempt` stops unwinding and returns that
  error namespace as an ordinary value.
- If the block returns an error normally, `attempt` returns that same value;
  it does not distinguish returned errors from caught errors.
- `attempt` catches only `throw`. It does not intercept `return`, `break`, or
  `continue`.
- Like `do`, `attempt` does not create a child scope. Declarations and
  assignments made before an error remain visible in caller scope; it is not a
  transaction and does not roll back side effects.

An uncaught thrown error unwinds to the embedding host and terminates the
current Pima program. Pima does not define conventional `try`/`catch` syntax in
this language version.

## 6. Bindings and scope

Pima uses lexical scope. Each function invocation and namespace instance has a
local environment linked to the environment in which it was created.

The three binding forms are defined as follows:

- `set name value` creates an immutable binding in the current environment.
- `var name value` creates a mutable binding in the current environment.
- `let name value` updates the nearest existing mutable binding named `name`.

`set` and `var` are declarations. Declaring a name that already exists in the
current environment is an error. `let` is assignment rather than declaration:
using it with an unbound name or an immutable binding is an error.

### 6.1 Patterns and destructuring

A binding target may be a pattern. Bare names and symbol names both identify
destination bindings, `_` ignores a value, and list patterns destructure
immutable lists recursively:

```pima
set (x (y _)) (3 (4 5))

var left 0
var right 0
let (left right) (10 20)
```

`set` creates immutable bindings for every capture, `var` creates mutable
bindings, and `let` updates existing mutable bindings. Destructuring is atomic:
the complete shape and every destination are validated before any declaration
or assignment occurs. A shape mismatch throws `:match_error`.

Within a binding target, `name` and `:name` have the same meaning. The symbol
spelling is accepted as a convenient name descriptor but is redundant because
`set`, `var`, or `let` already establishes that the expression is a binding
target:

```pima
set (x y) (1 2)
set (:x :y) (1 2)       // equivalent spelling

var left 0
var right 0
let (left right) (3 4)
let (:left :right) (3 4) // legal, but unnecessarily symbolic
```

The binding form, not the presence of `:`, determines the operation. In
particular, `let (:left :right) ...` does not create bindings: both names must
already resolve to mutable bindings just as they must in the bare-name form.

`match` evaluates its subject once and selects the first matching arm:

```pima
match result (
    (ok :value) {
        Console.println value
    }

    (error :error) {
        throw error
    }

    _ {
        Console.println "unknown result"
    }
)
```

Inside `match` patterns, `:name` captures, while a bare name matches the symbol with that
name, ordinary literals require equality, and `_` is a wildcard. Thus `ok`
matches the symbol `:ok`, while `:value` captures the corresponding value.
Captures are immutable and visible only within their arm. If no arm matches,
`match` throws `:match_error`.

A function declaration binds its name in the current environment before its
body can execute. This permits direct recursion. Function parameters are
immutable bindings; algorithms that need a changing local value must declare a
mutable copy with `var`.

### 6.2 Visibility

Every declaration is private unless prefixed with `pub`:

```pima
set internal_limit 10
pub set PI 3.141592653589793

function helper (:x) {
    * x 2
}

pub function calculate (:x) {
    helper x
}
```

Privacy is enforced at an environment boundary:

- At module scope, only `pub` declarations are exported by `import`.
- In a namespace instance, only `pub` members may be accessed through `.` from
  outside that namespace.
- Code executing within an environment may access its private declarations.
  Functions declared in a namespace therefore retain access to private fields.
- Nested lexical scopes may resolve private declarations in their enclosing
  environment.

`pub` modifies declarations only. It cannot prefix `let`, an expression, or a
function parameter. Public mutable state is permitted with `pub var`, though
APIs should generally prefer private state exposed through public functions.

## 7. Functions, closures, and partial application

A function declaration has a name, an ordered list of parameter symbols, and a
body:

```pima
function add (:x :y) {
    + x y
}
```

Each parameter symbol is an unbound name descriptor. Declaring the function
does not resolve it as an identifier. Calling a function creates a child lexical
environment, binds the name represented by each symbol to its corresponding
argument, and executes the body. The final expression is the return value;
explicit `return` is optional.

Parameter symbols must have distinct names. A reserved word cannot be used as a
parameter name. Violating either rule is a declaration error.

A nested function captures bindings from its defining lexical environment:

```pima
function add_to (:x) {
    function inner (:y) {
        + x y
    }
}
```

The placeholder `_` may replace one or more call arguments. Such a call does
not invoke the function; it returns a partially applied function whose
parameters correspond, from left to right, to the placeholders:

```pima
set add_five [add 5 _]
add_five 3
```

Functions are ordinary values and may be stored, passed, and returned.

A function name used without invocation evaluates to the function value.
Brackets perform immediate invocation, including zero-argument invocation:

```pima
set operation calculate
[operation]

set area_function square.area
[square.area]
```

## 8. Control flow

### 8.1 Conditional

`if` evaluates its predicate and accepts a consequent with an optional
alternative:

```pima
if predicate consequent
if predicate consequent alternative
```

A branch may be a single expression or a block. If it is a block, the selected
block is executed. The unselected branch is not evaluated. When the predicate
is true, the value of `if` is the consequent's value. When the predicate is
false, the value is the alternative's value when present, or unit when no
alternative was provided.

The two-part form is useful for conditional effects and control transfer:

```pima
if [< balance 0] {
    return :invalid
}
```

Consequently, a single expression and a block containing that expression are
equivalent branch forms:

```pima
if [< x 2] [return "this"] alternative

if [< x 2] {
    return "this"
} alternative
```

Both selected branches produce the same `return` transfer. Blocks extend an
expression position to multiple statements without changing the surrounding
control form's result semantics.

### 8.2 Loops

`while predicate body` repeatedly evaluates the predicate and executes `body`
while the predicate is true.

`until predicate body` repeatedly evaluates the predicate and executes `body`
while the predicate is false.

The predicate is re-evaluated before every iteration. A loop evaluates to the
value of its last iteration, or unit if no iteration occurs.

### 8.3 Function and loop transfer

`return`, `break`, and `continue` perform explicit control transfer:

```pima
return value
return

break
break value

continue
```

- `return value` immediately exits the current function with `value`.
- Bare `return` immediately exits the current function with unit.
- `break value` immediately exits the nearest active `while` or `until` loop
  and makes `value` the result of that loop.
- Bare `break` exits the nearest active loop with unit.
- `continue` skips the remainder of the nearest active loop body and begins its
  next predicate evaluation.

Using `return` outside a function, or `break` or `continue` outside a loop, is a
runtime error. These transfers cannot cross a function-call boundary: a
function called from inside a loop cannot break or continue its caller's loop.

Code executed by `do` behaves as though it appeared directly at the call site.
Consequently, an evaluated block may return from the function containing the
`do`, or break or continue a loop containing the `do`. If `do` is called
inside another function, that function call remains a boundary.

## 9. Blocks, context contracts, and `do`

Blocks are first-class, uninstantiated chunks of code:

```pima
set greeting {
    println "hello " name
}
```

`do block` instantiates and executes the block in the caller's current
environment. Because a block does not capture the environment in which its
literal was created, all of its free identifiers are resolved through the
evaluation environment. This allows a caller to supply names referenced by the
block, as demonstrated by `examples/function_test.pima`.

An annotated block makes important environmental dependencies visible:

```pima
set report @(:name :score) {
    Console.println name score
}

function render (:report :name :score) {
    do report
}

render report "Ada" 96
```

Before any operation executes an annotated block, it verifies that every
required name resolves through the supplied environment's ordinary lexical
lookup chain. A missing requirement throws an error classified as:

```pima
(:error :name_error :missing_context)
```

The annotation is a context contract rather than a function parameter list:
`render` already owns the `name` and `score` bindings, and the block uses those
same bindings. An annotated block remains type `:block`, captures no
environment, and is not callable. It must be executed by an operation that
supplies an environment.

Context validation is intrinsic to block execution rather than specific to
`do`. The same check applies when `if`, `while`, `until`, `attempt`, `new`, or
another block-aware form executes an annotated block. Unselected conditional
branches are neither validated nor executed. For `new`, requirements resolve
through the new namespace environment and then its enclosing environment.

`do` does not create a child scope. Declarations in the evaluated block are
created in the caller's current environment, and `let` assignments update
mutable bindings visible from that environment. The effect is the same as if
the block's statements had been written directly at the `do` call site.

A plain block has no enforced context contract and preserves the original open
block behavior. `@()` is permitted and creates an annotated block whose
required context list is empty.

## 10. Core operations

Only arithmetic and comparison operators are implicitly available to user
modules. Other core operations are exposed by the standard library's cohesive
namespaces:

| Operation | Meaning |
|---|---|
| `+ - * /` | Numeric arithmetic |
| `< > =` | Numeric comparison or value equality |
| `Maths.div`, `Maths.mod`, `Maths.int` | Integer division, remainder, and conversion |
| `Logic.not` | Boolean negation |
| `Types.of`, `Types.is?` | Inspect and test value types |
| `String.from`, `String.concat` | Display conversion and concatenation |
| `String.length`, `String.slice`, `String.chars` | Unicode-aware string operations |
| `String.code_point`, `String.from_code_point` | Convert between one-scalar strings and integer code points |
| `Console.println` | Print all operands followed by a line ending |
| `do` | Execute a block in the current environment |
| `attempt` | Execute a block and return any thrown error as a value |
| `append` | Return a new list with a value appended |
| `push` | Return a new list with a value prepended |
| `head` | Return the first list element |
| `rest` | Return all but the first list element |
| `empty?` | Test whether a list is empty |

### 10.1 Numbers

Pima has signed 64-bit integers and IEEE 754 binary64 floating-point numbers.
Arithmetic follows these rules:

- Integer `+`, `-`, and `*` return an integer. Overflow is a runtime error.
- When either operand is a float, arithmetic promotes the integer operand to a
  float and returns a float.
- `/` accepts numeric operands and always returns a float.
- `div` accepts integers, divides them, and truncates the result toward zero.
- `mod` accepts integers and returns the Euclidean remainder. For a nonzero
  divisor, its result is always greater than or equal to zero and less than the
  absolute value of the divisor.
- Division or remainder by zero is a runtime error for both numeric types.
- `int` converts an integer to itself and truncates a finite float toward zero.
  Conversion fails when the float is NaN, infinite, or outside the signed
  64-bit range.
- Floating-point operations use IEEE 754 behavior and may otherwise produce
  infinities or NaN.

Numeric comparisons promote mixed integer/float operands to floats. Float
equality is exact IEEE equality; approximate comparison belongs in a library
function rather than the `=` operator.

`=` follows these rules:

- Booleans, numbers, strings, and symbols compare by value. Symbols are equal
  when their names are equal. Integer and floating-point numbers with the same
  mathematical value compare equal.
- Unit values compare equal.
- Lists compare structurally: they are equal when they have the same length and
  each corresponding element is equal under these same rules.
- Functions, code blocks, and namespaces compare by identity. Two distinct
  instances are unequal even when they contain equivalent code or members.
- Values of unrelated categories are unequal rather than producing an error.

### 10.2 Strings

Strings are immutable sequences of Unicode scalar values stored as valid UTF-8.
Source files must also be valid UTF-8. Pima never exposes byte indexes through
its core string API.

String literals support these escapes:

```text
\"  \\  \n  \r  \t  \0  \u{1F600}
```

The hexadecimal value in `\u{...}` must denote a valid Unicode scalar value.
Malformed UTF-8, an invalid escape, or an invalid scalar value produces a
`:syntax_error` while loading source.

Core string operations are:

- `concat strings...` returns their concatenation. It accepts strings only and
  does not implicitly convert other values.
- `length string` returns its number of Unicode scalar values.
- `slice string begin end` returns the half-open scalar range `[begin, end)`.
  Indexes must be nonnegative integers satisfying
  `begin <= end <= length`.
- `chars string` returns an immutable list in which each element is a
  one-scalar string.
- `code_point string` requires exactly one Unicode scalar value and returns its
  integer code point.
- `from_code_point integer` returns the one-scalar string for a valid Unicode
  scalar value. Negative integers, values above `0x10FFFF`, and the surrogate
  range `0xD800..0xDFFF` produce `:value_error`.
- `string value` explicitly converts a value to the same human-readable form
  used by `println`.

An invalid string operand produces `:type_error`. An invalid index produces an
error classified as `(:error :index_error)`. Arithmetic `+` is numeric only;
string concatenation always uses `concat`.

### 10.3 Immutable lists

Lists cannot be modified after creation. Every list operation leaves its input
unchanged:

```pima
set original (1 2)
set extended [push original 0]

// original is still (1 2)
// extended is (0 1 2)
```

The list operations have these exact contracts:

- `push list value` returns a new list beginning with `value`, followed by every
  element of `list`.
- `append list value` returns a new list containing every element of `list`,
  followed by `value`.
- `head list` returns the first element. It is an error when `list` is empty.
- `rest list` returns a new list containing every element except the first. It
  is an error when `list` is empty.
- `empty? list` returns whether the list contains no elements.

Implementations should use structural sharing where practical. In particular,
`push` and `rest` should be constant-time operations. `append` may take time
proportional to the list's length.

There is no mutating `pop` operation. Traversal uses `head` and `rest`, while
construction uses `push` or `append`.

The standard library imported as `/pima/library/standard` exports cohesive
namespace values rather than individual global functions:

- `Maths`: `pow`, `less_or_equal`, `greater_or_equal`, `increment`,
  `decrement`, `range`, `absolute`, `minimum`, `maximum`, `clamp`, `sum`,
  `product`, `average`, `div`, `mod`, `int`, `E`, and `PI`;
- `String`: `concat`, `length`, `slice`, `chars`, `from`, `lower`, `upper`,
  `trim`, `contains?`, `starts_with?`, `ends_with?`, `replace`, `split`, and
  `join`;
- `List`: `push`, `append`, `head`, `rest`, `empty?`, `reverse`, `foreach`,
  `map`, `length`, `contains?`, `fold`, `filter`, `any?`, and `all?`;
- `Types`: `of` and `is?`;
- `Console`: `println`; and
- `Logic`: `not` and `select`.

Arithmetic and comparison operators remain unqualified language primitives.

An implementation may provide these definitions as Pima source or equivalent
built-ins.

## 11. Namespaces and objects

A block may be used as a namespace template:

```pima
set Counter {
    var value 0

    function increment () {
        let value [+ value 1]
    }
}

set counter [new Counter]
```

`new template` instantiates the template block in a fresh namespace environment
and executes it there. Each invocation creates independent bindings. The
original block remains uninstantiated and may be used to create more namespaces.

`new` accepts exactly one code block and does not accept constructor arguments.
Pima has no reserved initializer method. A function that accepts values and
creates a namespace is an ordinary constructor:

```pima
set InvalidBalance {
    pub set types (:error :validation_error :invalid_balance)
    pub set message "Opening balance cannot be negative"
}

function create_account (:opening_balance) {
    if [< opening_balance 0] {
        throw [new InvalidBalance]
    } {
        new {
            pub set types (:account)

            var balance opening_balance

            pub function current_balance () {
                balance
            }

            pub function deposit (:amount) {
                let balance [+ balance amount]
                balance
            }
        }
    }
}

set account [create_account 100]
```

The block passed to `new` is instantiated in a fresh namespace whose enclosing
environment is the scope in which `new` is evaluated. Its declarations may
therefore initialize fields from constructor parameters. Functions declared
inside the block close over the completed namespace environment.

Namespace construction proceeds as follows:

1. Evaluate the operand and require a code block.
2. Create a fresh namespace environment linked to the current scope.
3. Execute the block in that namespace environment.
4. Validate its optional public `types` declaration.
5. Return the completed namespace.

If block execution, a declaration, or type validation throws, construction
fails and the incomplete namespace is discarded. External side effects already
performed by the block are not rolled back. There is no implicit call to
`init`, `new`, or any other member.

Discarding prevents publication of the incomplete namespace value; it does not
invalidate values deliberately published through earlier external side
effects. For example, a closure assigned to an outer mutable binding before the
failure remains callable and retains the construction environment it captured.
An arena-based implementation may therefore retain unreachable or externally
reachable construction storage until the interpreter itself is dropped.

A namespace member is accessed with the whitespace-free `.` operator:

```pima
counter.value
[counter.increment]
```

The operand before `.` is evaluated normally. The identifier after `.` is a
literal member name and is not resolved as a variable in the caller's scope.
Member access may be chained from left to right:

```pima
application.window.width
```

Accessing a data member returns its value. Accessing a function member returns
the function with its namespace environment already captured. It can therefore
be called with ordinary prefix-call syntax:

```pima
[counter.increment]
square.set_width 40
```

A namespace value is not implicitly callable, and `square set_width 40` does
not perform member lookup. Accessing a private member from outside its namespace
is an error, even if the member's name is known.

## 12. Imports

`import path` evaluates a Pima source module exactly once per interpreter
instance. Only declarations explicitly prefixed with `pub` become visible in
the importing environment. Private module declarations remain available to
functions defined by that module but cannot be named by the importer.

Both quoted and bare paths are accepted:

```pima
import "/pima/library/standard"
import /pima/library/standard
```

An import may be assigned a namespace alias:

```pima
import "/pima/library/standard" as standard
standard.List.reverse (1 2 3)
```

A namespace's public members may be statically imported into the current
module:

```pima
import "/pima/library/standard"
import Maths.*

[pow 2 8]
```

`import Namespace.*` is permitted only at module scope. It resolves
`Namespace` as an ordinary identifier, requires a namespace value, and adds
read-only live views of all its public members. The operation is atomic: if any
member would collide with a binding already declared in the current module,
the import fails without introducing any members. Private members are never
imported.

With an alias, the module's public declarations are available only as members
of that alias namespace. Without an alias, public declarations are introduced
directly into the importing environment.

An unaliased import that would collide with any existing binding is an error;
it never overwrites or silently hides a binding. An alias must itself be a new
binding in the current environment. Importing the same module through multiple
aliases is permitted, but the module body is still evaluated only once.

### 12.1 Module lifecycle

Imports are permitted only at module scope. Each canonical module path has one
of four lifecycle states:

```text
:unloaded  :loading  :loaded  :failed
```

Importing a module follows this algorithm:

1. Resolve the requested path relative to the importing file and canonicalize
   it to a unique module identity.
2. If it is `:unloaded`, mark it `:loading` and evaluate it in a fresh private
   module environment.
3. If evaluation succeeds, retain its `pub` bindings and mark it `:loaded`.
4. If evaluation throws, expose no partial exports, cache the error, and mark
   the module `:failed`.

A `:loaded` module is reused without executing its body again. A `:failed`
module rethrows its cached initialization error on every later import during
that interpreter instance.

Importing a module already in `:loading` is an import cycle. Pima does not expose
partially initialized modules; it throws an error classified as:

```pima
(:error :import_error :import_cycle)
```

The error diagnostic includes the complete cycle of canonical module paths.
Module initialization side effects performed before a failure are not rolled
back.

Imported names are read-only views of the exporting module's bindings. If a
module internally changes a `pub var`, importers observe the new value, but
cannot assign to it with `let`. An aliased import exposes these views through
an immutable module namespace. An unaliased import introduces the same
read-only views directly into the importing module.

### 12.2 Standard I/O module

Filesystem I/O is provided by the bundled `/pima/io` module rather than by core
syntax:

```pima
import "/pima/io" as io

set text [io.read_text "input.txt"]
io.write_text "output.txt" text
```

The module provides synchronous whole-file, directory, and path operations:

| Operation | Result |
|---|---|
| `read_text path` | Entire UTF-8 file as a string |
| `read_lines path` | Immutable list of lines without line terminators |
| `read_bytes path` | Immutable list of integer bytes from `0` through `255` |
| `write_text path text` | Create or replace a UTF-8 text file |
| `append_text path text` | Create or append to a UTF-8 text file |
| `write_bytes path bytes` | Create or replace a binary file |
| `append_bytes path bytes` | Create or append to a binary file |
| `exists? path` | Whether any filesystem entry exists |
| `file? path` | Whether the path is a regular file |
| `directory? path` | Whether the path is a directory |
| `create_directory path` | Recursively create a directory and its parents |
| `list_directory path` | Sorted list of immediate entry names |
| `copy_file source destination` | Copy one regular file |
| `move source destination` | Rename or move an entry on the same filesystem |
| `remove_file path` | Remove a regular file |
| `remove_directory path` | Remove an empty directory |
| `join paths...` | Join one or more path components |
| `parent path` | Parent path, or unit when absent |
| `file_name path` | Final path component, or unit when absent |
| `extension path` | Extension without `.`, or unit when absent |
| `canonicalize path` | Absolute canonical path to an existing entry |
| `current_directory` | Interpreter working directory |

Operations that mutate the filesystem return unit. Relative paths are resolved
against the interpreter's configured working directory. Path composition
functions use the host platform's separators. Directory listings are sorted
lexically to keep scripts deterministic. Binary writes require an immutable
list containing only integers in the inclusive range `0..255`.

I/O failures throw error namespaces classified with `:io_error` and a more
specific symbol when one is known:

```text
(:error :io_error :file_not_found)
(:error :io_error :permission_denied)
(:error :io_error :invalid_encoding)
(:error :io_error :already_exists)
(:error :io_error :invalid_input)
(:error :io_error :timed_out)
(:error :io_error :unsupported_operation)
```

The error may expose immutable public context such as `path`. Host-specific
error details may be included in the diagnostic message but must not replace
the portable type symbols above. `read_text` throws `:invalid_encoding` when a
file is not valid UTF-8. A filename that cannot be represented as UTF-8 also
throws `:invalid_encoding`. The existence predicates return `false` for a
missing path but propagate other inspection failures. `write_text` and
`write_bytes` replace an existing regular file; failure during writing is not
specified to be atomic in this language version. `remove_directory` is
deliberately non-recursive.

The virtual path `/pima/library/standard` names the implementation's standard
library. Resolution of other relative paths is based on the importing file's
directory.

## 13. Excluded functionality

The following are not required:

- Java classes, reflection, or the `java` form;
- a JVM runtime;
- static typing;
- classes or inheritance beyond namespace templates; and
- concurrency or asynchronous evaluation.

## 14. Conformance examples

The normative behavioral suite consists of:

```text
birthday_paradox.pima
closure.pima
curried_example.pima
fibonacci.pima
foreach.pima
function_test.pima
import_test.pima
json_parser.pima
lib.pima
list.pima
namespace_test.pima
newton.pima
test.pima
timing.pima
while.pima
```

`java_support.pima` is intentionally excluded.
