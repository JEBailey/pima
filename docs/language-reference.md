# Pima Language Reference

Status: draft normative specification, derived from the programs in `examples/`.

## 1. Language model

Pima is a dynamically typed, expression-oriented language. Calls use prefix
notation, functions are first-class values, and code blocks are first-class,
uninstantiated chunks of code that may be stored and passed around.

An implementation conforms to this specification when it can parse and execute
all example programs, subject to the standard library requirements in section
10.

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

Pima supports line and block comments:

```pima
// line comment

/*
   block comment
*/
```

A block comment ends at the next `*/`. Block comments need not nest.

Documentation tooling recognizes two specialized line-comment prefixes:

```pima
//! Documentation for the containing module.

/// Documentation for the following public declaration.
pub function greet (name) {
    Console.println "Hello" name
}
```

`pima doc` associates contiguous `///` lines with the public declaration that
follows them and emits `//!` lines at the start of a file as module
documentation. Both remain comments to the language evaluator.

The command-line development tools are:

```text
pima run file.pima
pima check file-or-directory
pima fmt [--check] file-or-directory
pima doc [--format html|markdown|json] [-o path] file-or-directory
pima lsp
```

`pima file.pima` is a compatibility spelling of `pima run file.pima`.
Formatting preserves physical line boundaries because changing them may change
the program's parse. HTML documentation is the default and produces an index,
one page per module, navigation, and a stylesheet. Markdown and JSON are
alternative representations of the same extracted public API model.

Identifiers are case-sensitive. An identifier may contain letters, decimal
digits, `_`, or operator punctuation. It must not be mistaken for a number,
string, delimiter, or reserved word. Examples include:

```text
fibonacci  good_enough  empty?  <=  ..
```

A period directly between identifier tokens is the object member operator,
not part of either identifier. Whitespace is not permitted around this
operator. A punctuation-only operator such as `..` remains an identifier when
it appears as its own token.

A colon immediately followed by an identifier forms a symbol literal:

```pima
:x  :item  :good_enough
```

The colon is not part of the symbol's name. It quotes the following name:
instead of declaring, capturing, or resolving the name according to context,
Pima produces a literal symbol. The colon is used only when a symbol value or
literal symbol constraint is intended. Name positions such as binding
destinations, function declarations, import aliases, and context requirements
use bare identifiers.

### 2.1 Naming conventions

Pima programs use these naming conventions:

| Kind | Convention | Examples |
| --- | --- | --- |
| Values, functions, and parameters | `snake_case` | `opening_balance`, `parse_value` |
| Boolean predicates | `snake_case?` | `empty?`, `starts_with?` |
| Objects and object templates | `PascalCase` | `String`, `InvalidOrder` |
| Constants | `UPPER_CASE` | `PI`, `MAX_SIZE` |
| Symbols and semantic type tags | `:snake_case` | `:good`, `:type_error` |
| Source files | `snake_case.pima` | `json_parser.pima` |

The `?` suffix communicates that a function answers a boolean question.
Functions that validate, throw, or return a result value are not predicates
and do not receive the suffix. PascalCase describes object-like values; it
does not imply that Pima has classes. Punctuation operators such as `+`, `=`,
and `<=` retain their symbolic spellings.

The reserved words are:

```text
as  attempt  await  branch  break  continue  do  function  if  import  let  match  new  pub
remote  return  this  val  throw  until  var  while
```

## 3. Grammar

The following grammar is written in EBNF. `NL` is a physical line ending and
`logical-NL` is a line ending that is not suppressed by a balanced delimiter or
the inline-block continuation rule.

```ebnf
program          = layout*, [ statement-list ], layout* ;

statement-list   = statement, { terminator+, statement } ;
statement        = line-expression ;
line-expression = expression, { separator+, expression } ;
terminator       = logical-NL ;
layout           = horizontal-space | NL | comment ;
separator        = horizontal-space | suppressed-NL | comment ;

expression       = postfix-expression
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
                 | remote-expression
                 | await-expression ;

postfix-expression
                 = primary-expression,
                   { ".", member-name } ;
primary-expression
                 = literal
                 | symbol
                 | "this"
                 | identifier
                 | "_"
                 | list
                 | block
                 | annotated-block
                 | bracket-expression ;

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
                   [ context-requirement,
                     { separator+, context-requirement } ],
                   separator*, ")" ;
context-requirement
                 = [ "*" | "&" ], identifier ;
bracket-expression
                 = "[", separator*, expression,
                   { separator+, expression }, separator*, "]" ;
expression-list  = expression, { separator+, expression } ;

declaration      = [ visibility, separator+ ], ( binding
                 | function-declaration ) ;
visibility       = "pub" ;
binding          = ( "val" | "var" ), separator+,
                   binding-pattern, separator+, expression ;
assignment       = "let", separator+, assignment-target, separator+, expression ;
assignment-target
                 = binding-pattern | member-target ;
member-target    = ( identifier | "this" ), ".", member-name,
                   { ".", member-name } ;
binding-pattern  = identifier | "_" | binding-list-pattern ;
binding-list-pattern
                 = "(", separator*, [ binding-pattern,
                   { separator+, binding-pattern } ], separator*, ")" ;
match-pattern    = identifier | symbol | literal | "_" | match-list-pattern ;
match-list-pattern
                 = "(", separator*, [ match-pattern,
                   { separator+, match-pattern } ], separator*, ")" ;
function-declaration
                 = "function", separator+, identifier, separator*,
                   match-pattern, separator+, expression ;
member-name      = identifier | reserved-word ;

conditional      = "if", separator+, expression, separator+,
                   expression, [ separator+, expression ] ;
loop             = ( "while" | "until" ), separator+,
                   expression, separator+, expression ;
control-transfer = return-expression
                 | break-expression
                 | "continue"
                 | throw-expression ;
return-expression
                 = "return", [ separator+, expression ] ;
break-expression = "break", [ separator+, expression ] ;
throw-expression = "throw", separator+, expression ;
attempt-expression
                 = "attempt", separator+, expression ;
match-expression = "match", separator+, expression, separator*,
                   "(", separator*, match-arm,
                   { separator*, match-arm }, separator*, ")" ;
match-arm        = match-pattern, separator+, expression ;
import-expression
                 = "import", separator+, ( string | import-path ),
                   [ separator+, "as", separator+, identifier ] ;
new-expression   = "new", separator+, expression,
                   { separator+, expression } ;
do-expression    = "do", separator+, expression ;
remote-expression = "remote", separator+, expression ;
await-expression  = "await", separator+, expression ;
```

`expression` describes one parsed expression; it does not recursively include
a call production. Calls are assembled by the containing physical line or
bracket. Every runtime call has a callee and one list argument. When a line
contains multiple expressions, the first is the callee and the remaining
expressions are implicitly packed into its argument list. A bracket always
establishes a call boundary: its first expression is the callee, and any
remaining expressions are packed in the same way.

For example:

```pima
Console.println "sum:" [Math.sum 1 2 3]
[+ [fibonacci 5] [fibonacci 6]]
```

Thus `operation a b` passes `(a b)`, and `int "42"` passes the singleton
argument list `("42")`. Every explicit parenthesized expression contributes
one element to that implicit argument list: `operation (a b)` passes
`((a b))`. Parentheses never disappear at a call boundary.

A physical line is a command boundary. A line containing one ordinary
expression evaluates it and, when the result is callable, invokes it with the
empty argument list. A non-callable result passes through unchanged. A line
containing two or more expressions always invokes the first with the remaining
expressions as its argument pack. Brackets are strict call boundaries:

```pima
operation       // call with () when callable; otherwise return its value
operation 1 2   // call operation with (1 2)
[operation]     // call operation with ()
[operation 1 2] // call operation with (1 2)
(operation 1 2) // construct a three-element list
```

A bracketed callee with no operands receives the empty argument list, so `[run]`
is a zero-argument invocation. `[run ()]` is different: it passes one explicit
empty list and therefore receives `(())`. Unlike a zero-operand line command,
a bracket call is strict: a non-callable callee throws a type error.

To preserve a callable value without executing it as the line's command, use
it in an operand position such as a binding value or explicit control
transfer:

```pima
function multiplier (factor) {
    function apply (value) { * factor value }
    return apply
}
```

Declarations and control structures follow the same surface principle: the
leading reserved word determines the fixed expression operands that follow it.
For example, `val Counter { ... }` supplies the binding name and block to
`val`, while `function set (value notify) { ... }` supplies the function
name, parameter pattern, and body. These are special forms rather than runtime
function calls, but they share the same prefix layout.

Parentheses explicitly construct lists; they do not group arithmetic
expressions or optionally wrap a call's operands. Brackets immediately invoke
a call expression. Braces create a block value and do not execute its body
merely by being encountered.

`@` is a block-construction special form with two syntactic operands: a list of
required context binding names and a block body:

```pima
@(name score) {
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
- Object
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
:symbol  :list  :function  block  :object
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

### 4.2 Object type symbols

An object template may declare additional semantic types with an immutable,
public `types` member:

```pima
val Square {
    pub val types (:square :shape)
}

val square [new Square]
[types square]          // (:object :square :shape)
[is? square :shape]     // true
```

For an object, the runtime prepends `:object` to the declared type list.
The declared list may contain application-defined symbols but cannot contain a
fundamental runtime type symbol. `new` validates the member when present:
`types` must be declared with `pub val`, its value must be a list of unique
symbols, and it cannot be reassigned. An object without this member has the
type list `(:object)`.

## 5. Evaluation

Literals and symbols evaluate to themselves. An identifier evaluates to the
value in its nearest lexical binding. Symbols are immutable and compare by
their name.

A list evaluates its elements from left to right and produces a new immutable
list.

At statement level, the physical line is a command. After resolving its first
expression, Pima invokes callable values and returns non-callable values. Thus
`serve` on its own line calls a zero-parameter function, while `42`, `"text"`,
or a name bound to either value simply returns that value.

A bracket expression evaluates its callee and implicitly packed argument from
left to right, immediately invokes the callee, and yields the result:

```pima
[* 6 7]                 // 42
[produce_value]         // empty-list argument
```

A block literal evaluates to an inert code-block value. Creating or passing a
code block does not execute it, create a scope, bind its free identifiers, or
capture an environment. A code block becomes instantiated only when an
operation such as `do`, `attempt`, or `new` supplies an execution
environment.

An annotated block declares context bindings that must be visible when `do`
executes it:

```pima
val report @(name score) {
    Console.println (name score)
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
    foo (2)
    bar (3 x)
}
```

They do not necessarily produce the same kind of value: a block literal
produces an inert `:block` value until a block-aware form supplies an execution
environment. When such a form executes the block, the block's final statement
is its resulting value. This lets a multi-statement block serve anywhere that
form would otherwise accept a single result-producing expression.

Calls evaluate the callee and argument from left to right, then invoke
the callee. The special forms `if`, `while`, `until`, `function`, `val`, `let`,
`var`, `pub`, `import`, `new`, `do`, `attempt`, `return`, `break`,
`continue`, and `throw` control evaluation of one or more operands.

Operations fail by throwing typed error values. Error conditions include:

- resolving an unbound identifier;
- calling a value that is not callable;
- supplying a value that does not match a function's parameter pattern;
- applying an operation to unsupported value types;
- applying `head` or `rest` to an empty list; and
- importing a source file that cannot be found or parsed.

### 5.1 Errors

Every error is an object classified with at least `:error`:

```pima
val InvalidOrder {
    pub val types (:error :validation_error :invalid_order)
    pub val message "The order is invalid"
    pub val order_id 42
}

throw [new InvalidOrder]
```

Consequently, all errors satisfy both:

```pima
[is? error :object]
[is? error :error]
```

An error object must expose an immutable public `message` string. It may
expose any additional immutable application data and may add progressively
specific type symbols. Custom error types require no global registration.

`throw value` immediately stops normal evaluation and begins unwinding Pima
function calls. Its operand must be classified as `:error`; throwing any other
value produces a `:type_error`. Native failures create and throw error
objects through this same mechanism; a host-language panic must never escape
as a Pima error.

Errors remain ordinary values. A function may return an error object when an
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

`attempt expression` evaluates the expression in the caller's current
environment and returns a thrown error as a value. A block is useful when the
protected operation requires multiple expressions:

```pima
val result [attempt {
    read_file path
}]

if [is? result :error] {
    println result.message
} {
    process result
}
```

- If the expression completes normally, `attempt` returns its result.
- If evaluation throws an error, `attempt` stops unwinding and returns that
  error object as an ordinary value.
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

Pima uses lexical scope. Each function invocation and object instance has a
local environment linked to the environment in which it was created.

The three binding forms are defined as follows:

- `val name value` creates a stable, immutable binding in the current
  environment. The binding cannot be reassigned; this does not require its
  value to be known at compile time.
- `var name value` creates a mutable binding in the current environment.
- `let name value` updates the nearest existing mutable binding named `name`.

`val` and `var` are declarations. Declaring a name that already exists in the
current environment is an error. `let` is assignment rather than declaration:
using it with an unbound name or an immutable binding is an error.

### 6.1 Patterns and destructuring

A binding target is a bare name or a list of bare names. `_` ignores a value,
and list patterns destructure immutable lists recursively:

```pima
val (x (y _)) (3 (4 5))

var left 0
var right 0
let (left right) (10 20)
```

`val` creates immutable bindings for every capture, `var` creates mutable
bindings, and `let` updates existing mutable bindings. Destructuring is atomic:
the complete shape and every destination are validated before any declaration
or assignment occurs. A shape mismatch throws `:match_error`.

The surrounding binding form already establishes that these names are
destinations rather than expressions, so quoting them with `:` would be
redundant. The form determines the operation: `let (left right) ...` does not
create bindings; both names must refer to existing mutable bindings.

`match` is Pima's structural selection form. It evaluates its subject once and
selects the first matching pattern arm. It does not evaluate arm patterns as
Boolean conditions. In this
example, `result` is a two-element immutable list. Its first element is a
symbol that identifies the kind of result, and its second element is the
associated value:

```pima
val result (:error "the input was not valid")
// A successful result might instead be (:good 42).

match result (
    (:good value) {
        Console.println value
    }

    (:error message) {
        Console.println message
    }

    _ {
        Console.println "unknown result"
    }
)
```

The subject therefore has one of these conventional shapes:

```pima
(:error "an error message") // the symbol :error followed by a string
(:good value)               // the symbol :good followed by any result value
```

The arm `(:error message)` is also a two-element list pattern. Its symbol
literal `:error` matches itself, while the bare name `message` captures the
list's second element. Similarly, `(:good value)` matches a list beginning
with `:good` and captures its second element as `value`. The pattern itself
does not require `message` to be a string; that is part of this result
representation's convention.

More generally, inside `match` patterns, a bare name captures, `:name` matches
the literal symbol with that name, ordinary literals require equality, and
`_` is a wildcard. The colon therefore has the same meaning in expressions and
patterns: it prevents name lookup and denotes a literal symbol. Captures are
immutable and visible only within their arm. Arms are tested from top to
bottom, and only the selected result is evaluated. If no arm matches, `match`
throws `:match_error`; use `_` as an explicit exhaustive fallback.

A function declaration binds its name in the current environment before its
body can execute. This permits direct recursion. Function parameters are
immutable bindings; algorithms that need a changing local value must declare a
mutable copy with `var`.

### 6.2 Visibility

Every declaration is private unless prefixed with `pub`:

```pima
val internal_limit 10
pub val PI 3.141592653589793

function helper (x) {
    * (x 2)
}

pub function calculate (x) {
    helper (x)
}
```

Privacy is enforced at an environment boundary:

- At module scope, only `pub` declarations are exported by `import`.
- In an object instance, only `pub` members may be accessed through `.` from
  outside that object.
- Code executing within an environment may access its private declarations.
  Functions declared in an object therefore retain access to private fields.
- Nested lexical scopes may resolve private declarations in their enclosing
  environment.

`pub` modifies declarations only. It cannot prefix `let`, an expression, or a
function parameter. Public mutable state is permitted with `pub var`, though
APIs should generally prefer private state exposed through public functions.
Deliberately public mutable members are writable through member assignment:

```pima
let counter.count 10
```

Only members declared with `var` are assignable. `pub val` remains externally
readable but immutable, and private members remain inaccessible from outside
their object.

## 7. Functions, closures, and partial application

A function declaration has a name, one parameter pattern, and one body
expression:

```pima
function add (x y) {
    + (x y)
}
```

Calling a function evaluates one argument value, matches the parameter pattern
against it, creates immutable bindings for its captures, and evaluates the body
expression in a child lexical environment. A block is an expression and may be
used as a multi-statement body. Its final expression is the return value;
explicit `return` remains optional.

Bare names capture values. Symbols match themselves as literal constraints.
Capture names within the pattern must be distinct.

```pima
function identity value value
function unwrap (value) value
```

`identity` captures its complete argument. `unwrap` requires a one-element list
and captures that element.

A nested function captures bindings from its defining lexical environment:

```pima
function add_to (x) {
    function inner (y) {
        + (x y)
    }
}
```

The placeholder `_` may replace one or more elements in a call's argument list. Such a call does
not invoke the function; it returns a partially applied function whose
parameters correspond, from left to right, to the placeholders:

```pima
val add_five [add 5 _]
add_five (3)
```

Functions are ordinary values and may be stored, passed, and returned.

A function name used without invocation evaluates to the function value.
Brackets perform immediate invocation:

```pima
val operation calculate
[operation]

val area_function square.area
[square.area]
```

## 8. Control flow

Pima has three selection forms with deliberately separate roles:

| Form | Selects using | Intended shape | No selection |
| --- | --- | --- | --- |
| `if` | One Boolean predicate | Consequent and optional alternative | Unit when false without an alternative |
| `branch` | Ordered Boolean conditions | Any number of condition/result pairs | Unit |
| `match` | Patterns against one subject | Any number of pattern/result arms | Throws `:match_error` |

`if` and `branch` evaluate conditions and require Boolean results. `match`
evaluates its subject once but treats the left side of every arm as a pattern,
not an ordinary expression. The forms remain separate so source code always
makes that evaluation distinction visible.

### 8.1 Conditional

`if` evaluates its predicate and accepts a consequent with an optional
alternative:

```pima
if predicate consequent
if predicate consequent alternative
```

The consequent and alternative may each be a single expression or a block. If
one is a block, the selected block is executed. The unselected result is not
evaluated. When the predicate is true, the value of `if` is the consequent's
value. When the predicate is false, the value is the alternative's value when
present, or unit when no alternative was provided. The predicate must evaluate
to a Boolean.

The two-part form is useful for conditional effects and control transfer:

```pima
if [< balance 0] {
    return :invalid
}
```

Consequently, a single expression and a block containing that expression are
equivalent result forms:

```pima
if [< x 2] [return "this"] alternative

if [< x 2] {
    return "this"
} alternative
```

Both selected branches produce the same `return` transfer. Blocks extend an
expression position to multiple statements without changing the surrounding
control form's result semantics.

### 8.2 Branch

`branch` expresses an ordered series of condition/result pairs without nested
`if` expressions. It is intended for multiple independent Boolean tests, not
for decomposing one value:

```pima
branch (
    [< score 60] { "fail" }
    [< score 90] { "pass" }
    true           { "excellent" }
)
```

Conditions are evaluated from top to bottom in the current scope. The result
paired with the first true condition is evaluated and becomes the value of the
`branch`; later conditions and all unselected results are not evaluated. Each
result may be a single expression or a block. Every evaluated condition must be
a Boolean. If no condition is true, or the pair list is empty, `branch` returns
unit. A final `true` condition is therefore the explicit default form.

Use `if` when there is one predicate. Use `branch` when several predicates are
tested in order. Use `match` instead when the decision is based on the shape or
literal content of one value.

### 8.3 Loops

`while predicate body` repeatedly evaluates the predicate and executes `body`
while the predicate is true.

`until predicate body` repeatedly evaluates the predicate and executes `body`
while the predicate is false.

The predicate is re-evaluated before every iteration. A loop evaluates to the
value of its last iteration, or unit if no iteration occurs.

### 8.4 Function and loop transfer

`return`, `break`, and `continue` perform explicit control transfer:

```pima
return value
return

break
break value

continue
```

Each transfer consumes at most one value expression (`throw` requires one).
A nested call therefore uses its normal bracket boundary:

```pima
return [calculate value]
throw [invalid_value value]
```

`return calculate value` is invalid because it supplies two expressions to
`return`; it is not reinterpreted as a call.

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
val greeting {
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
val report @(name score) {
    Console.println name score
}

function render (report name score) {
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
through the new object environment and then its enclosing environment.

`do` does not create a child scope. Declarations in the evaluated block are
created in the caller's current environment, and `let` assignments update
mutable bindings visible from that environment. The effect is the same as if
the block's statements had been written directly at the `do` call site.

A plain block has no enforced context contract and preserves the original open
block behavior. `@()` is permitted and creates an annotated block whose
required context list is empty.

Each requirement may declare how its value crosses an isolated worker
boundary:

```pima
val Worker @(
    configuration
    *workload
    &service
) { ... }
```

- `name` copies a transportable snapshot and is the default;
- `*name` moves the value and replaces its shared caller-side location with an
  `(:error :move_error :moved_value)` value after the worker is created; and
- `&name` shares an existing remote-object, future, or TCP-listener handle.

Move is transactional with respect to worker creation. A missing or
untransportable value leaves the caller's binding unchanged. Sharing an
ordinary scalar, list, local object, closure, or mutable cell fails with
`:unsendable_value`. For local block execution these markers do not alter
lexical lookup; they describe only transport across an isolated boundary.

`*` does not imply serialization of a local runtime graph. Local objects,
functions, bound methods, code blocks, binding cells, and TCP connections are
VM-bound and cannot be copied or moved into an isolated worker. If any element
of a persistent list is VM-bound, transport of the whole list fails
transactionally. No source location or alias is invalidated on failure.
Construct local objects and closures inside the worker from transported scalar,
string, symbol, and persistent-list snapshots. Existing remote objects,
futures, and TCP listeners cross contexts only through the explicitly supported
handle rules; TCP listeners require `&`.

Assignment preserves identity for reference-like values: objects, functions,
blocks, remote objects, futures, and native resource handles. A binding made
from one of these values is another reference to the same VM location, not a
copy and not a move. Consequently, successfully moving through any such alias
replaces the shared source location, and every caller-side alias observes the
same `:moved_value` error. Scalar and persistent-data assignment retains value
semantics.

Member access and imports are location-producing operations even when the
stored value is scalar. For example, `val current_count state.count` resolves
`state.count` once and retains that member location. Rebinding `state` later
does not retarget `current_count`. The retained member keeps its complete owning
object alive until the final external reference disappears.

The moved error records where and how the move occurred. It exposes immutable
`move_operation`, `move_source`, `move_start`, and `move_end` fields, and its
runtime metadata retains the move instruction as the diagnostic origin. This
provenance is created only when worker creation succeeds.

Reading a function member produces a bound function. Storing, passing, or
returning that function creates another reference; it does not remove the
member or change its object. Its `this` value remains the object from which the
function was read. Ownership changes only at an explicit move/share boundary.

## 10. Core operations

Only arithmetic and comparison operators are implicitly available to user
modules. Other core operations are exposed by the standard library's cohesive
objects:

| Operation | Meaning |
|---|---|
| `+ - * /` | Numeric arithmetic |
| `< > =` | Numeric comparison or value equality |
| `Math.div`, `Math.mod`, `Math.int` | Integer division, remainder, and conversion |
| `Logic.not` | Boolean negation |
| `Types.of`, `Types.is?` | Inspect and test value types |
| `Reference.same?` | Test whether two references identify the same storage |
| `String.from`, `String.concat` | Display conversion and concatenation |
| `String.length`, `String.slice`, `String.chars` | Unicode-scalar string operations |
| `String.byte_length` | UTF-8 encoded byte length |
| `String.code_point`, `String.from_code_point` | Convert between one-scalar strings and integer code points |
| `Console.println` | Print all operands followed by a line ending |
| `do` | Execute a block in the current environment |
| `attempt` | Evaluate an expression and return any thrown error as a value |
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

Numeric comparisons compare mixed integer/float operands mathematically without
first rounding the integer through `f64`. This keeps `=`, `<`, and `>` coherent
for integers outside the float's exact-integer range. NaN is unordered, so
equality and both relational comparisons involving NaN are false. Float
equality is otherwise exact IEEE equality; approximate comparison belongs in a
library function rather than the `=` operator.

`=` follows these rules:

- Booleans, numbers, strings, and symbols compare by value. Symbols are equal
  when their names are equal. Integer and floating-point numbers with the same
  mathematical value compare equal.
- Unit values compare equal.
- Lists compare structurally: they are equal when they have the same length and
  each corresponding element is equal under these same rules.
- Functions, code blocks, and objects compare by identity. Two distinct
  instances are unequal even when they contain equivalent code or members.
- Error objects are unordered and never equal, including to themselves or to
  another alias of the same error. This applies to every object whose types
  include `:error`, not only built-in failures. An error nested in a list makes
  the corresponding element comparison false. Inspect errors with `Types.is?`
  and their public metadata rather than equality.
- Values of unrelated categories are unequal rather than producing an error.

`Reference.same?` is separate from value equality. It returns `true` only when
both arguments are references to the same resolved storage location. Equal
values stored in different locations, and non-reference values, return `false`.
Aliases remain identical after their shared location is moved or invalidated,
even though the resulting error values remain unequal under `=`.

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
val original (1 2)
val extended [push original 0]

// original is still (1 2)
// extended is (0 1 2)
```

The list operations have these exact contracts:

- `push (list value)` returns a new list beginning with `value`, followed by every
  element of `list`.
- `append (list value)` returns a new list containing every element of `list`,
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
object values rather than individual global functions:

- `Math`: `pow`, `less_or_equal`, `greater_or_equal`, `increment`,
  `decrement`, `range`, `absolute`, `minimum`, `maximum`, `clamp`, `sum`,
  `product`, `average`, `div`, `mod`, `int`, `E`, and `PI`;
- `String`: `concat`, `length`, `byte_length`, `slice`, `chars`, `code_point`,
  `from_code_point`, `from`, `string`, `lower`, `upper`, `trim`, `contains?`,
  `starts_with?`, `ends_with?`, `replace`, `split`, and `join`;
- `List`: `push`, `append`, `head`, `rest`, `empty?`, `reverse`, `foreach`,
  `map`, `length`, `contains?`, `fold`, `filter`, `any?`, and `all?`;
- `Types`: `of` and `is?`;
- `Reference`: `same?`;
- `Console`: `println`; and
- `Logic`: `not` and `select`.

Arithmetic and comparison operators remain unqualified language primitives.

An implementation may provide these definitions as Pima source or equivalent
built-ins.

## 11. Modules, templates, and objects

A block may be used as an object template:

```pima
val Counter {
    var value 0

    function increment () {
        let value [+ value 1]
    }
}

val counter [new Counter]
```

`new template` instantiates the template block in a fresh object environment
and executes it there. Each invocation creates independent bindings. The
original block remains uninstantiated and may be used to create more objects.

`new` accepts exactly one code block and does not accept constructor arguments.
Pima has no reserved initializer method. A function that accepts values and
creates an object is an ordinary constructor:

```pima
val InvalidBalance {
    pub val types (:error :validation_error :invalid_balance)
    pub val message "Opening balance cannot be negative"
}

function create_account (opening_balance) {
    if [< opening_balance 0] {
        throw [new InvalidBalance]
    } {
        new {
            pub val types (:account)

            var balance opening_balance

            pub function current_balance () {
                balance
            }

            pub function deposit (amount) {
                let balance [+ balance amount]
                balance
            }
        }
    }
}

val account [create_account 100]
```

The block passed to `new` is instantiated in a fresh object whose enclosing
environment is the scope in which `new` is evaluated. Its declarations may
therefore initialize fields from constructor parameters. Functions declared
inside the block close over the completed object environment.

`this` is a reserved value referring to that completed object from inside its
methods:

```pima
val Counter {
    pub val count 42
    pub function current () this
    pub function read () this.count
}
```

Each `new` expression creates its own `this` binding, so nested objects refer
to themselves rather than an enclosing object. The binding is private and
immutable and cannot be redeclared. It is filled when construction completes;
before completion it carries the construction's provisional `:invalid_object`
lifecycle value. Outside object construction, `this` is an unbound reserved
value.

`new` accepts one or more code-block templates through its containing line or
bracket boundary. Multiple operands perform **ordered namespace composition**:
they are packed in source order with the same leftmost-wins behavior as an
explicit template list:

```pima
[new Specialized Base]
[new (Specialized Base)]
```

`do`, by contrast, accepts exactly one code block. Additional operands are a
syntax error.

Object construction proceeds as follows:

1. Evaluate the operands and require one or more code-block templates.
2. Select complete definitions in source order, with the leftmost definition
   for a member name taking precedence.
3. Create one fresh object environment linked to the current scope.
4. Execute only the surviving definitions in that object environment.
5. Validate its optional public `types` declaration.
6. Return the completed object.

The contributing templates are code blocks, not constructed parent objects.
They do not remain as reachable or hidden runtime instances. There is one
namespace, one object identity, and one `this` shared by every surviving
method. Losing definitions and their initializers do not execute. A
destructuring declaration must survive as a whole; composition that would
select only some of its captures is a compiler error. The public immutable
`types` definition is the sole composition-specific exception: every valid
template contribution is merged in source order.

If block execution, a declaration, or type validation throws, construction
fails and the incomplete object is discarded. External side effects already
performed by the block are not rolled back. There is no implicit call to
`init`, `new`, or any other member.

Functions and blocks created within a construction share its lifecycle token.
Successful completion activates that token. If construction fails or exits
before completion, every escaped function, bound method, block, and alias tied
to it evaluates to `(:error :object_error :invalid_object)` when used. Ordinary
data copied out before failure remains valid. The failed `new` still produces
its original error; the invalid-object error appears only when an escaped
reference is later used. Its public `construction_error` member retains the
original failure, including its diagnostic origin and stack metadata.

An object member is accessed with the whitespace-free `.` operator:

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
the function with its object environment already captured. It can therefore
be called with ordinary prefix-call syntax:

```pima
[counter.increment]
square.set_width 40
```

An object value is not implicitly callable, and `square set_width 40` does
not perform member lookup. Accessing a private member from outside its object
is an error, even if the member's name is known.

`let` also accepts a member-access target. A public mutable member may be
updated externally, while a method may update its own private mutable members
through `this`:

```pima
val counter new {
    var count 0

    pub function increment () {
        let this.count [+ this.count 1]
    }
}

counter.increment
```

Member assignment evaluates the target object and replacement expression
before committing the update. If target resolution, visibility checking,
mutability checking, or replacement evaluation fails, the existing member
value is unchanged. Assigning a private member externally raises
`:visibility_error`; assigning an immutable member raises `:mutation_error`.
Remote-object members cannot be assigned directly; their state changes through
the remote object's public functions.

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

An import may be assigned an object alias:

```pima
import "/pima/library/standard" as standard
standard.List.reverse (1 2 3)
```

The alias is a literal binding destination, so the `:` is required just as it
is for `val`, `var`, `let`, and `function` declarations.

An object's public members may be imported into the current module:

```pima
import "/pima/library/standard"
import Math.*

[pow 2 8]
```

One public member may be selected, optionally under a different local name:

```pima
import Logic.not
import Math.pow as exponentiate

[not false]
[exponentiate 2 8]
```

Object paths may be nested:

```pima
import "/pima/library/standard" as standard
import standard.Logic.not as negate
```

The object-import forms are:

```text
import object-path.*
import object-path.member
import object-path.member as local-name
```

They are permitted only at module scope. Every path begins with an ordinary
identifier and every intermediate segment must be a public object member.
`*` adds live references to all public bindings. A selected import adds one
live reference using either the member name or its `as` name. Each reference
preserves the source binding's mutability: an imported `pub var` is writable,
while an imported `pub val` remains immutable. Private members are never
imported.

Object imports are atomic: if a target name would collide with a binding
already declared in the current module, the import fails without introducing
anything. Wildcard imports cannot use `as`; preserving an object under
another local name is ordinary value binding rather than an import:

```pima
val arithmetic Math
```

With an alias, the module's public declarations are available only as members
of that alias object. Without an alias, public declarations are introduced
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

Imported names refer to their source bindings rather than copying their values.
If the source is a `pub var`, assignment through either the imported name or a
qualified member updates the same storage location, and both forms immediately
observe the change. If the source is a `pub val`, both forms remain immutable.
Selected-import aliases preserve this behavior; the alias changes only the
local spelling. Moving through an imported reference invalidates that shared
location under the ordinary move rules, so every other reference observes the
same moved error. Unaliased module and object imports introduce these references
directly into the importing module.

A reference to an object member retains the complete object that owns that
storage, including its private state and other members. Rebinding the name that
previously held the object does not destroy it while such a reference remains.
The object and its member storage become collectible together after the final
external reference disappears.

### 12.2 Standard I/O module

Filesystem I/O is provided by the bundled `/pima/io` module rather than by core
syntax:

```pima
import "/pima/io" as io

val text [io.read_text "input.txt"]
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

I/O failures throw error objects classified with `:io_error` and a more
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

### 12.3 TCP module

The `/pima/tcp` module exposes synchronous TCP primitives:

```pima
import "/pima/tcp" as tcp

val listener [tcp.listen "127.0.0.1" 8080]
val connection [tcp.accept listener]
tcp.set_timeout connection 5000
val request [tcp.read connection 1024]
tcp.write (connection "response")
tcp.close connection
tcp.close listener
```

| Operation | Result |
|---|---|
| `listen address port` | Opaque TCP listener |
| `accept listener` | Opaque TCP connection |
| `read connection maximum` | Up to `maximum` bytes decoded as UTF-8 |
| `write connection text` | Write the complete UTF-8 encoding |
| `set_timeout connection milliseconds` | Set read and write timeouts |
| `close resource` | Close a listener or connection |

`accept` and `read` block the current interpreter thread. `read` performs one
socket read rather than imposing message framing; an empty string denotes an
orderly peer shutdown. Its maximum must be between 1 and 1,048,576 bytes.
Reads that are not valid UTF-8 and operating-system socket failures throw
`:tcp_error`. Closed resources cannot be reused.

TCP deliberately does not parse or generate application protocols. The
repository's `examples/http_server_lib.pima` implements request framing,
HTTP/1.x parsing, handler dispatch, response validation, and serialization in
Pima. `demos/http_file_server.pima` combines it with the static file-serving
example.

A TCP listener is a synchronized host handle and may be supplied to remote
workers through an `&listener` context requirement. This supports bounded
accept-worker pools. Accepted connections remain owned by the worker that
accepted them; ordinary Pima objects and VM heaps are not shared.

## 13. Remote object construction

`remote Template` is the remote counterpart of `new Template`. It constructs
the object in an isolated worker VM and returns a remote object handle.
Every public member request returns a future immediately; `await` is the only
language operation that waits for its transported result.

The operand denotes object templates rather than executable work:

```pima
val worker [remote Worker]
val composed [remote (Worker Observable)]
```

An arbitrary call, closure, or value is not a valid `remote` operand. Templates
must currently be statically known. Names explicitly listed by an annotated
template are resolved in the caller, transported as values, and installed as
immutable worker-local bindings. Mutable cells never cross the worker boundary.
Ordered namespace composition transports the union of external requirements
and uses the same complete-definition, leftmost-wins contract for `new` and
`remote`.

Reads and calls both produce futures:

```pima
val status_request worker.status
val work_request [worker.process input]
val done [work_request.complete?]
val status [await status_request]
val result [await work_request]
```

Arguments are transported before a request is queued. The worker completes its
future with the transported result or error. Futures expose the zero-argument
`complete?` member. `await` returns the completed value, or rethrows its error,
and may be repeated on the same future.

## 14. Excluded functionality

The following are not required:

- static typing;
- classes, inheritance, parent objects, or `super`; and
- arbitrary closure scheduling through `remote`.

## 15. Conformance examples

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
object_test.pima
newton.pima
test.pima
timing.pima
while.pima
```
