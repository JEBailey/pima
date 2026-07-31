# Type System Proposal for Pima

Generated as a design review after evaluating the current Pima runtime, compiler, VM IR, and namespace model.

---

## Pima — Current Type Model

### What Exists Today

Pima has a **structural type tag system**, not a type system in the traditional sense. The key facts:

1. **Every value has a type list** — a non-empty list of symbols. The first symbol is the fundamental runtime type (`:integer`, `:string`, `:namespace`, etc.).
2. **Type tags are declared by namespaces** via a `pub val types` member:
   ```pima
   val :Square {
       pub val :types (:square :shape)
   }
   ```
   The runtime prepends `:namespace` automatically, so `Square` instances have type list `(:namespace :square :shape)`.
3. **Type testing is runtime-only** — `Types.is? (value :symbol)` checks if a symbol appears in a value's type list. No compile-time enforcement exists.
4. **Namespaces are bags of bindings** — `new Template` executes a block in a fresh namespace environment. The resulting namespace has whatever bindings the block created. There is no schema, no required-field checking, and no invariant enforcement beyond the `types` member validation.
5. **`new` validates minimally** — it checks that `types` is `pub val`, contains only unique symbols, and contains no fundamental type symbols. That's it.

### Runtime Data Structures

| Component | Role | Key Constraint |
|---|---|---|
| `Value::Namespace(NamespaceRef)` | Runtime value for namespaces | `NamespaceRef = Gc<Namespace>` |
| `Namespace` | Holds environment + type list + error metadata | `environment: EnvironmentRef`, `types: Vec<SymbolId>` |
| `Environment` | Map of `SymbolId → Binding` | `IndexMap<SymbolId, Binding>` in `RefCell<Gc>` |
| `Binding` | Value + mutability + visibility | `value: Value`, `mutability: Immutable\|Mutable`, `visibility: Private\|Public` |
| `SymbolInterner` | String → `SymbolId` dedup | Per-VM-instance (not global) |

### VM IR for Namespaces

- `MakeNamespace { destination, bindings }` — Creates a namespace from a list of `{name, source: Register, public: bool}`. Each source register is read and linked into the namespace.
- `LoadMember { destination, namespace, name }` — Loads a public member by name. Enforces visibility at runtime.
- No IR instruction exists for type checking, field validation, or invariant enforcement beyond what `MakeNamespace` does via `context.make_namespace()`.

### Compiler for `new`

The compiler (`vm/compiler/blocks.rs::compile_new`) walks the template block's statements, extracts bindings and functions, then emits a single `MakeNamespace` instruction. It does not validate that the template satisfies any schema or that instances will have specific members.

### What the Spec Says

The language reference section 13 explicitly excludes:
- Static typing
- Classes or inheritance beyond namespace templates

The spec's type model is deliberately dynamic: "Pima is dynamically and strongly typed: native operations validate their operands and do not implicitly coerce values."

---

## Current Limitations

1. **No field contracts** — A namespace template may declare `val width 80`, but nothing enforces that all instances have `width`. If the template block has a bug and skips a binding, the instance silently lacks that field.

2. **No type-level relationships** — There's no way to express that `:square` is a subtype of `:shape` beyond the type list convention. `Types.is? (value :shape)` works, but there's no compile-time or structural enforcement.

3. **No constructor validation** — `new Template` runs the block and hopes for the best. If a binding fails or produces the wrong type, you only discover it at the point of use (or not at all).

4. **No function signature enforcement** — Functions accept one list argument and pattern-match it. Wrong shapes produce `:match_error` at runtime with no提前 warning.

5. **No way to declare "this namespace must have these members"** — The annotated block `@(:name :score)` checks context bindings for `do`, but there's no equivalent for namespace membership contracts.

---

## Suggestion 1: Structural Type Declarations with `type` Keyword

**The idea:** A compile-time `type` declaration that defines a named shape — a required set of fields with optional type tags — enforced at `new` time.

**Why it fits Pima:** Pima already has namespace templates as blocks. A `type` declaration annotates a template with its contract. The compiler validates `MakeNamespace` against the contract, emitting compile-time diagnostics for missing required fields.

```pima
// Declare a type contract
type Counter {
    require val count :integer
    pub require function increment ()
    pub require function get ()
}

// Define an implementation
val :MyCounter {
    pub val :types (:counter)

    var :count 0

    pub function :increment () {
        let :count [+ (count 1)]
    }

    pub function :get () {
        count
    }
}

// This succeeds — MyCounter satisfies Counter
val :c [new MyCounter as Counter]
```

### Implementation Shape

**New AST node:**
```rust
pub enum NodeKind {
    // ... existing ...
    TypeDeclaration {
        name: Name,
        body: BlockId,       // contains `require` statements
    },
}

pub enum NodeKind {
    // inside type body:
    TypeRequirement {
        visibility: Visibility,
        mutability: BindingKind,
        name: Name,
        type_tag: Option<NodeId>,  // optional :symbol constraint
    },
}
```

**New IR instruction:**
```rust
/// Validate that a namespace register satisfies a type contract.
/// Fails with (:error :type_error :contract_violation) if any required
/// member is missing or has wrong visibility/mutability.
CheckContract {
    destination: Register,    // output namespace (same as source if valid)
    source: Register,         // namespace to check
    contract: u16,            // index into program's contract table
}

/// A named contract: list of required members with metadata.
pub struct TypeContract {
    pub name: Arc<str>,
    pub requirements: Vec<ContractMember>,
}

pub struct ContractMember {
    pub name: Arc<str>,
    pub public: bool,
    pub mutability: BindingMutability,
    pub type_tag: Option<Arc<str>>,  // e.g. "integer" for :integer check
}
```

**Compiler changes:**
- `Program` gains `contracts: Vec<TypeContract>`
- `new Template as Contract` compiles to: compile `new Template` → emit `CheckContract`
- The compiler statically checks that the template block satisfies the contract (missing fields = compile-time diagnostic). Runtime check remains for dynamic `new` (where the template isn't statically known).
- `CheckContract` at runtime iterates the namespace's environment bindings and verifies each requirement exists with correct visibility/mutability.

**Error classification:**
```
(:error :type_error :contract_violation)
  — member `count` required but missing
  — member `increment` required as public function but is private
  — member `count` required as :integer but is :string
```

### Why This Is the Right First Step

It adds enforceable structure to namespaces without changing Pima's dynamic nature. The type checking happens at `new` time (runtime) but gets compile-time optimization when the template is statically known. It's the minimum viable type system that solves the "bag of bindings" problem.

---

## Suggestion 2: Runtime Type Checker with Structural Subtyping

**The idea:** A native `/pima/typecheck` module that validates values against type descriptors at runtime, enabling pattern-based type assertions and guard functions.

**Why it fits Pima:** Pima already has `Types.is?` for single-symbol tests and `Types.of` for full type lists. A structural checker goes further: it can validate that a value has specific members with specific types, without requiring a compile-time `type` declaration.

```pima
import "/pima/typecheck" as typecheck

// Define a type descriptor (a list of requirements)
val :counter_schema (
    (:field "count" :integer :immutable :private)
    (:method "increment" :function :public)
    (:method "get" :function :public)
)

// Validate at runtime
val :c [new MyCounter]
val :valid [typecheck.matches? (c counter_schema)]

// Or throw on mismatch
typecheck.assert (c counter_schema)
```

### Implementation Shape

**Native functions in `/pima/typecheck`:**

| Function | Signature | Description |
|---|---|---|
| `matches?` | `(value descriptor)` → boolean | Check if value matches descriptor |
| `assert` | `(value descriptor)` → value | Return value or throw `:type_error` |
| `violation` | `(value descriptor)` → error\|unit | Return specific violation or unit |

**Descriptor format** — A list of requirement specs, each a list:
```pima
(:field "name" :expected-type :mutability :visibility)
(:method "name" :function :public)
(:has "name")  // just existence check
```

**Runtime validation algorithm:**
1. If value is not a namespace, fail immediately unless descriptor is empty.
2. For each `:field` requirement: check the namespace environment for a binding with matching name, mutability, and visibility. Optionally check the value's fundamental type.
3. For each `:method` requirement: check for a public binding whose value is a function.
4. For type symbol requirements: check the namespace's type list contains the expected symbol.

**Key insight:** This works with ANY namespace — not just ones created from typed templates. It's duck-typing made explicit. A namespace satisfies a descriptor if it has the right shape, regardless of how it was constructed.

### Relationship to Suggestion 1

Suggestion 1 (`type` declarations) produces compile-time guarantees. Suggestion 2 (runtime checker) works without compile-time info. They complement each other:
- `type` declarations for your own code (compile-time safety)
- `typecheck.matches?` for validating external/dynamic values (runtime flexibility)

A `type` declaration could internally compile to a descriptor that `typecheck` uses, sharing the validation logic.

---

## Suggestion 3: Algebraic Data Types via Tagged Unions

**The idea:** A `union` construct that creates values with a discriminant tag and a shaped payload, validated at construction time. Pattern-matched via `match` with compile-time exhaustiveness checking.

**Why it fits Pima:** Pima already has `match` with pattern matching on lists and literals. The conventional result pattern `(:ok value)` / (:error message)` is already used informally via lists with a symbol tag. A proper union type makes this explicit, validated, and exhaustive.

```pima
// Define a union type
type Result {
    ok :value
    error :message
}

// Construct variants
val :success [Result.ok 42]
val :failure [Result.error "something went wrong"]

// Match with exhaustiveness checking
match success (
    (:ok value) {
        Console.println ("got:" value)
    }
    (:error message) {
        Console.println ("err:" message)
    }
)
```

### Implementation Shape

**New Value variant:**
```rust
#[derive(Clone, Debug)]
pub enum Value {
    // ... existing ...
    Union {
        tag: SymbolId,        // variant discriminant
        payload: Value,       // single payload value (could be a list of fields)
        union_id: UnionTypeId, // which union type this belongs to
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnionTypeId(pub u16);
```

**New AST nodes:**
```rust
NodeKind::UnionDeclaration {
    name: Name,
    variants: Vec<UnionVariant>,
}

pub struct UnionVariant {
    pub name: Name,
    pub payload_pattern: Pattern,  // e.g. :value or (:a :b :c)
}
```

**New IR instructions:**
```rust
/// Construct a union variant. Validates payload against the variant's pattern.
MakeUnion {
    destination: Register,
    union_type: u16,      // index into program's union table
    variant: u16,         // variant index
    payload: Register,    // payload value (list if multi-field)
}

/// Extract union tag for branching (optimized match compilation).
UnionTag {
    destination: Register,
    source: Register,
}
```

**Program-level storage:**
```rust
pub struct Program {
    // ... existing ...
    pub unions: Vec<UnionDefinition>,
}

pub struct UnionDefinition {
    pub name: Arc<str>,
    pub variants: Vec<UnionVariantDef>,
}

pub struct UnionVariantDef {
    pub name: Arc<str>,
    pub payload_pattern: Vec<Arc<str>>,  // field names for multi-field payloads
}
```

**Match compilation:** When the compiler sees `match` over a binding known to be a union type, it compiles to a `UnionTag` instruction + jump table instead of sequential list-length/equality checks. The compiler can also verify exhaustiveness: if variants are missing, emit a warning diagnostic.

**Type system integration:** A union value has type list `(:union :Result)` where `:Result` is the union's declared name. `Types.is? (value :Result)` works naturally.

### Why This Is Powerful

This solves Pima's most common ad-hoc pattern: the tagged result tuple. Today, `(:ok value)` is just a list — nothing enforces that it has two elements or that the first is `:ok`. With unions:
- Construction validates the payload shape
- Match can be exhaustive (compile-time warning on missing variants)
- The runtime representation is compact (tag + single payload, not a full list)
- It integrates with `type` contracts (Suggestion 1) — a function can declare its return type as a union

### Example: HTTP Response Type

```pima
type HttpMethod (:method)
type Headers ((:list (:pair :string :string)))
type Body (:body)

type HttpRequest {
    request (:method :path :headers :body)
}

type HttpResponse {
    ok (:status :reason :headers :body)
    client_error (:status :reason)
    server_error (:status :message)
}

function :handle_request (req) -> HttpResponse {
    match req (
        (:request method path headers body) {
            if [= (method "GET")] {
                HttpResponse.ok (200 "OK" () "content")
            } {
                HttpResponse.client_error (405 "Method Not Allowed")
            }
        }
    )
}
```

---

## Recommended Implementation Order

### Phase 1: Structural Type Declarations (Suggestion 1)

**Effort:** Medium. Adds one new AST node type, one IR instruction, and a contract table to `Program`.

**Risk:** Low. Stays within the existing namespace model. Compile-time checks are optimistic (warn on known violations), runtime checks are the source of truth.

**Dependency:** None. Builds on existing `MakeNamespace` and `new` compilation.

**Impact:** Immediately useful. Every namespace in Pima code can be typed. Catches bugs at `new` time instead of at field access time.

### Phase 2: Runtime Type Checker (Suggestion 2)

**Effort:** Low-Medium. Pure native functions, no AST or IR changes.

**Risk:** Very low. Opt-in runtime validation. No compile-time behavior affected.

**Dependency:** Can share validation logic with Phase 1's `CheckContract`.

**Impact:** Enables validation of values from external sources (imports, I/O, user input). Complements compile-time contracts.

### Phase 3: Algebraic Data Types (Suggestion 3)

**Effort:** High. New `Value` variant, new AST nodes, new IR instructions, match compilation changes, exhaustiveness analysis.

**Risk:** Medium. Changes the value model and match semantics. Must coexist with existing list-based tagged unions.

**Dependency:** Phase 1 (type declaration infrastructure) and the match compiler.

**Impact:** Transforms how Pima programs represent sum types. Makes the conventional `(:ok value)` pattern explicit, validated, and optimizable.

---

## Design Constraints & Trade-offs

### What Pima Is NOT Becoming

This proposal does **not** add:
- **Static type inference** — Pima remains dynamically typed. Type declarations are contracts checked at construction, not annotations inferred across the program.
- **Gradual typing** — There's no "checked" vs "unchecked" mode. Either you validate or you don't.
- **Generics** — Type parameters would require significant compiler changes. Out of scope.
- **Subtype lattices** — Type relationships are flat: a value either satisfies a contract or it doesn't. No inheritance hierarchy.

### The GC Constraint

`dumpster::unsync::Gc` means all runtime type metadata must be either:
- Stored in `Program` (compile-time, not GC-traced)
- Stored as `Arc<str>` or `SymbolId` inside `Value` (both `'static` or GC-safe)

Union definitions and type contracts belong in `Program` — they're compile-time artifacts. Runtime type tags on values are just `SymbolId`s (already used by the type list system).

### Backward Compatibility

All three suggestions are additive:
- Existing untyped namespaces work exactly as before
- `type` declarations are optional — templates without contracts bypass validation
- `typecheck.matches?` is opt-in
- Union values are a new `Value` variant — existing code never sees them unless it uses the `type` keyword

### The Spec Question

The language reference section 13 currently excludes "static typing." These suggestions stay within bounds because:
- Validation happens at runtime (`new` time), not at compile time as a hard gate
- The compiler emits warnings, not errors, for contract violations on known templates
- The language remains dynamically typed — types belong to values, not bindings
- No type inference or type variables are introduced

The spec would need a new section: **"Type Contracts"** — describing `type` declarations as runtime-enforced namespace schemas, not static types.

---

## Open Questions

1. **Should `type` declarations be module-scoped or namespace-members?** Module-scoped keeps them simple. Namespace-members would allow `import MyModule.Counter` to bring in the type contract.

2. **How strict should compile-time checking be?** Option A: warnings only (current Pima philosophy). Option B: errors for known violations (safer but stricter).

3. **Should unions replace the list-based `(:tag payload)` convention?** Soft deprecation with a lint warning seems right. The convention works and is widespread in existing code.

4. **Can `match` exhaustiveness be enforced?** The compiler can warn when a union is matched without covering all variants. But should missing variants be errors? Pima's philosophy suggests warnings.

5. **Type aliases?** `type Alias = OtherType` — useful for renaming, but adds complexity. Defer until Phase 2+.

6. **Nested contracts?** Can a field require its value to satisfy another contract? `require val inner :InnerContract` — possible but adds recursive validation. Phase 2+ territory.
