# Agent DSL Schema and Template Type System Specification

## Status

Draft specification.

This document defines the schema syntax used to type agent inputs and outputs, with particular focus on:

- readable object and field declarations
- nullable values
- arrays and fixed arrays
- enums
- tagged variants
- safe template access
- expression-level matching without `if` / `else`
- compile-time guarantees that template expressions cannot crash because of null or variant-dependent fields

The DSL intentionally keeps the top-level surface small. The only top-level declaration covered by this spec is:

```wire
schema name {
    ...
}
```

Other constructs such as `enum`, `variant`, `maybe`, and `match` are type expressions or value expressions, not top-level declarations.

---

# 1. Goals

## 1.1 Primary goals

The schema system should be:

1. **Readable**
   - Simple schemas should be easy to scan.
   - Complex nested objects should not require awkward postfix syntax.

2. **Strongly typed**
   - Agent inputs and outputs should be validated before execution.
   - Template expressions should be checked statically.
   - Unsafe access through nullable values or variants should fail at compile time.

3. **JSON-compatible**
   - Schemas should map naturally to JSON Schema.
   - Variants should map to discriminated JSON objects.

4. **LLM-friendly**
   - Field descriptions should be preserved into generated JSON Schema descriptions.
   - Output schemas should guide structured LLM output.

5. **Minimal**
   - Avoid introducing many top-level keywords.
   - Keep complex behavior inside type expressions and expression syntax.

---

# 2. Non-goals

The schema system does not currently attempt to support:

- optional object fields
- inheritance
- open-ended structural subtyping
- arbitrary dependent types
- template `if` / `else` blocks
- block-level narrowing
- user-defined type aliases outside `schema`

Nullable values and optional fields are intentionally separate concepts.

This spec only includes nullable fields, where the field must exist but may contain `null`.

---

# 3. Schema declarations

A schema declares an object shape.

```wire
schema participant {
    name: string
    role: string
}
```

A schema may reference another schema using the `schema.` namespace:

```wire
schema research_summary {
    title: string
    lead: schema.participant
    participants: [schema.participant]
}
```

## 3.1 Schema naming

Schema names must be lowercase `snake_case`.

Valid:

```wire
schema research_summary {
    title: string
}
```

Invalid:

```wire
schema ResearchSummary {
    title: string
}
```

Invalid:

```wire
schema researchSummary {
    title: string
}
```

## 3.2 No schema assignment syntax

A schema declaration must not use assignment syntax.

Valid:

```wire
schema participant {
    name: string
}
```

Invalid:

```wire
schema status = enum {
    draft
    ready
    published
}
```

If a type-like property needs to be shared with other schemas, declare it as a field inside a shared schema and reference that field type.

```wire
schema shared {
    status: enum {
        draft
        ready
        published
    }
}

schema some_other_schema {
    status: schema.shared.status
}
```

---

# 4. Field declarations

A field declaration has this shape:

```wire
field_name: TypeExpression
```

Example:

```wire
schema user {
    id: string
    name: string
    age: number
}
```

Fields are required by default.

This means the following schema:

```wire
schema user {
    name: string
    bio: maybe string
}
```

expects both `name` and `bio` to exist.

Valid JSON:

```json
{
  "name": "Rafael",
  "bio": null
}
```

Invalid JSON:

```json
{
  "name": "Rafael"
}
```

The field `bio` is nullable, not optional.

---

# 5. Descriptions

Descriptions should use doc comments.

```wire
schema user {
    /// Stable user identifier
    id: string

    /// Display name shown to other users
    name: string

    /// Optional biography. The field exists, but may be null.
    bio: maybe string
}
```

Doc comments are preserved into generated JSON Schema descriptions.

This style is preferred over postfix string descriptions because it remains readable for complex multiline types.

Preferred:

```wire
/// A nullable nested object value
nullable_object: maybe {
    /// Nested string field
    string_value: string

    /// Nested number field
    number_value: number
}
```

Avoid postfix descriptions after complex multiline types.

Descriptions should be placed before the field or nested field using doc comments.

---

# 6. Primitive types

The following primitive types are supported:

```wire
string
number
float
boolean
object
```

## 6.1 `string`

A JSON string.

```wire
name: string
```

## 6.2 `number`

A JSON number.

```wire
score: number
```

`number` may represent either integer-like or floating-point JSON numbers.

## 6.3 `float`

A numeric value specifically intended to be floating-point.

```wire
confidence: float
```

## 6.4 `boolean`

A JSON boolean.

```wire
enabled: boolean
```

## 6.5 `object`

An arbitrary JSON object.

```wire
metadata: object
```

Use `object` only when the structure is intentionally unknown or unconstrained.

---

# 7. Nullable values

Nullable values use `maybe`.

```wire
bio: maybe string
```

This means:

- the field must exist
- the value may be either `string` or `null`

Equivalent conceptual type:

```text
string | null
```

However, the DSL should prefer `maybe string` instead of union syntax.

## 7.1 Nullable object

```wire
profile: maybe {
    name: string
    email: string
}
```

Valid JSON:

```json
{
  "profile": null
}
```

Valid JSON:

```json
{
  "profile": {
    "name": "Rafael",
    "email": "rafael@example.com"
  }
}
```

## 7.2 Nullable array

```wire
tags: maybe [string]
```

This means the whole array may be null.

```json
{
  "tags": null
}
```

or:

```json
{
  "tags": ["dsl", "agent"]
}
```

## 7.3 Array of nullable values

```wire
tags: [maybe string]
```

This means the array itself must exist, but each item may be null.

```json
{
  "tags": ["dsl", null, "agent"]
}
```

## 7.4 Nullable array of nullable values

```wire
tags: maybe [maybe string]
```

This allows:

```json
{
  "tags": null
}
```

and:

```json
{
  "tags": ["dsl", null, "agent"]
}
```

---

# 8. Arrays

An array type uses square brackets.

```wire
tags: [string]
```

An array of objects:

```wire
items: [{
    id: string
    score: number
}]
```

A nested array:

```wire
matrix: [[number]]
```

---

# 9. Fixed-size arrays

A fixed-size array uses this syntax:

```wire
triple: [string; 3]
```

This means exactly three strings.

Valid JSON:

```json
{
  "triple": ["a", "b", "c"]
}
```

Invalid JSON:

```json
{
  "triple": ["a", "b"]
}
```

Invalid JSON:

```json
{
  "triple": ["a", "b", "c", "d"]
}
```

---

# 10. Tuples

A tuple uses parentheses.

```wire
value: (string, number, boolean)
```

A tuple with a fixed-size array:

```wire
tuple_value: (string, number, [string; 3])
```

Tuples map to JSON arrays with fixed length and positional item types.

Example:

```json
{
  "tuple_value": ["abc", 123, ["x", "y", "z"]]
}
```

---

# 11. Enums

Enums are type expressions.

```wire
status: enum {
    draft
    ready
    published
}
```

This represents a string enum with the values:

```json
"draft"
"ready"
"published"
```

## 11.1 One-line enum syntax

When enum values are defined on one line, commas are required.

```wire
status: enum { draft, ready, published }
```

## 11.2 Multiline enum syntax

When enum values are defined on separate lines, commas are not required.

```wire
status: enum {
    draft
    ready
    published
}
```

## 11.3 Nullable enum

```wire
status: maybe enum {
    draft
    ready
    published
}
```

Valid JSON:

```json
{
  "status": null
}
```

Valid JSON:

```json
{
  "status": "ready"
}
```

---

# 12. Inline objects

Inline object types use braces.

```wire
profile: {
    name: string
    email: string
}
```

Inline objects can be nested.

```wire
user: {
    id: string

    profile: {
        display_name: string
        avatar_url: maybe string
    }
}
```

An array of inline objects:

```wire
participants: [{
    name: string
    role: string
}]
```

A nullable inline object:

```wire
profile: maybe {
    name: string
    email: string
}
```

---

# 13. Variants

Variants represent tagged polymorphic values.

A variant is a type expression, not a top-level declaration.

```wire
payload: variant type {
    user_created {
        user_id: string
        email: string
    }

    user_deleted {
        user_id: string
        reason: maybe string
    }
}
```

The word after `variant` is the discriminator field.

In this example, the discriminator field is `type`.

Runtime JSON:

```json
{
  "payload": {
    "type": "user_created",
    "user_id": "user_123",
    "email": "rafael@example.com"
  }
}
```

or:

```json
{
  "payload": {
    "type": "user_deleted",
    "user_id": "user_123",
    "reason": null
  }
}
```

## 13.1 Variant syntax

General form:

```wire
variant DiscriminatorField {
    case_name {
        field: Type
    }

    another_case {
        field: Type
    }
}
```

Example:

```wire
content: variant kind {
    text {
        value: string
    }

    image {
        url: string
        alt: maybe string
    }

    tool_call {
        name: string
        arguments: object
    }
}
```

Runtime JSON:

```json
{
  "content": {
    "kind": "image",
    "url": "https://example.com/image.png",
    "alt": null
  }
}
```

## 13.2 Variant case names

Variant case names may be identifiers:

```wire
user_created {
    user_id: string
}
```

For external API values that are not valid identifiers, string literals may be used:

```wire
event: variant type {
    "user.created" {
        user_id: string
        email: string
    }

    "user.deleted" {
        user_id: string
    }
}
```

Runtime JSON:

```json
{
  "event": {
    "type": "user.created",
    "user_id": "user_123",
    "email": "rafael@example.com"
  }
}
```

## 13.3 Variant discriminator injection

The discriminator field is implicit in each case.

This schema:

```wire
event: variant type {
    user_created {
        user_id: string
    }
}
```

conceptually produces a case shape like:

```wire
{
    type: "user_created"
    user_id: string
}
```

The user should not need to manually declare the discriminator field inside each case.

Invalid:

```wire
event: variant type {
    user_created {
        type: string
        user_id: string
    }
}
```

The compiler should reject manual declaration of a field with the same name as the discriminator.

## 13.4 Nullable variants

```wire
event: maybe variant type {
    user_created {
        user_id: string
    }

    user_deleted {
        user_id: string
    }
}
```

This allows the entire variant value to be null.

---

# 14. Object-level variants

Sometimes the whole schema is a variant object.

```wire
schema api_event {
    variant type {
        user_created {
            user_id: string
            email: string
        }

        user_deleted {
            user_id: string
            reason: maybe string
        }
    }
}
```

Runtime JSON:

```json
{
  "type": "user_created",
  "user_id": "user_123",
  "email": "rafael@example.com"
}
```

This is useful for APIs where the discriminator field and variant-specific fields exist at the root of the object.

Object-level variants are allowed only as the direct body of a schema.

---

# 15. Dynamic values

The DSL may allow dynamic values to be computed from agent outputs.

Example:

```wire
dynamic {
    value: match agent.example.some_variant {
        user_created.example.some_number
        user_deleted.example.some_number
    }
}
```

This expression reads as:

```text
Match over `agent.example.some_variant`.
If the value is the `user_created` case, return `.example.demo.some_number`.
If the value is the `user_deleted` case, return `.example.demo.some_number`.
```

The compiler should infer the matched value from the expression before the block.

The paths inside the `match` body are case-specific projections.

---

# 16. Match expressions

Because the DSL does not have `if` / `else`, variant narrowing should be handled by expression-level `match`.

## 16.1 Basic match expression

```wire
value: match agent.someagent.some_variant {
    user_created.example.demo.some_number
    user_deleted.example.demo.some_number
}
```

The expression being matched is:

```wire
agent.someagent.some_variant
```

Each entry starts with a variant case name.

```wire
user_created.example.demo.some_number
```

means:

```text
When the variant is `user_created`, access `example.demo.some_number` inside that case.
```

## 16.2 Match with explicit fallback

A fallback case may use `_`.

```wire
value: match agent.someagent.some_variant {
    user_created.example.demo.some_string
    user_deleted.example.demo.some_string
    _ "unknown"
}
```

The `_` case is used when no specific case matches.

For variants known statically, the compiler should usually require exhaustive matching unless `_` is present.

## 16.3 Match result type

All match branches must return the same type.

Valid:

```wire
value: match event.payload {
    user_created.user_id
    user_deleted.user_id
}
```

Both branches return `string`.

Invalid:

```wire
value: match event.payload {
    user_created.created_at
    user_deleted.retry_count
}
```

if `created_at` is `string` and `retry_count` is `number`.

Recommended compiler error:

```text
Match branches return incompatible types:
- user_created.created_at returns string
- user_deleted.retry_count returns number
```

## 16.4 Match over nullable variants

If the matched expression is nullable, the match must handle the nullable case using `_`.

The DSL does not allow `null` as a match branch label.

Given:

```wire
payload: maybe variant type {
    user_created {
        user_id: string
    }

    user_deleted {
        user_id: string
    }
}
```

This is invalid:

```wire
value: match payload {
    user_created.user_id
    user_deleted.user_id
}
```

because `payload` may be null.

This is also invalid:

```wire
value: match payload {
    user_created.user_id
    user_deleted.user_id
    _ "no payload"
}
```

Valid:

```wire
value: match payload {
    user_created.user_id
    user_deleted.user_id
    _ "no payload"
}
```

## 16.5 Match over nullable non-variant values

`match` may also be used to handle nullable values, but the nullable case must use `_`.

```wire
value: match user.profile {
    some.name
    _ "Unknown user"
}
```

However, this is optional. The preferred lightweight syntax for nullables is still `??`.

```wire
value: user.profile?.name ?? "Unknown user"
```

---

# 17. Path access

A path accesses nested values using dots.

```wire
agent.someagent.output.user.name
```

Plain dot access is only valid when every segment is statically known to exist and is not nullable.

Given:

```wire
schema output {
    user: {
        name: string
    }
}
```

Valid:

```wire
output.user.name
```

Given:

```wire
schema output {
    user: maybe {
        name: string
    }
}
```

Invalid:

```wire
output.user.name
```

because `user` may be null.

---

# 18. Safe nullable access

Safe nullable access uses `?.`.

```wire
output.user?.name
```

If `user` is null, the whole expression becomes null.

Given:

```wire
schema output {
    user: maybe {
        name: string
    }
}
```

Expression:

```wire
output.user?.name
```

has type:

```wire
maybe string
```

## 18.1 Chained safe access

```wire
output.a?.b?.c
```

If any nullable segment is null, the whole expression becomes null.

Given:

```wire
schema output {
    a: maybe {
        b: maybe {
            c: string
        }
    }
}
```

Expression type:

```wire
output.a?.b?.c
```

is:

```wire
maybe string
```

---

# 19. Null fallback

Null fallback uses `??`.

```wire
output.user?.name ?? "Unknown user"
```

Given:

```wire
output.user?.name
```

has type:

```wire
maybe string
```

and:

```wire
"Unknown user"
```

has type:

```wire
string
```

then:

```wire
output.user?.name ?? "Unknown user"
```

has type:

```wire
string
```

## 19.1 Fallback type rule

The left side must be `maybe T`.

The right side must be assignable to `T`.

Result type is `T`.

Valid:

```wire
name: output.user?.name ?? "Unknown"
```

Invalid:

```wire
name: output.user?.name ?? 123
```

if `name` expects `string`.

---

# 20. Variant projection

Variant projection allows access to a specific variant case without `if` / `else`.

Recommended syntax:

```wire
event.payload#user_created.user_id
```

This means:

```text
If `payload` is `user_created`, return `user_id`.
Otherwise, return null.
```

Given:

```wire
payload: variant type {
    user_created {
        user_id: string
    }

    user_deleted {
        user_id: string
    }
}
```

Expression:

```wire
payload#user_created.user_id
```

has type:

```wire
maybe string
```

because `payload` may not be the `user_created` case.

Use fallback when a plain value is required:

```wire
payload#user_created.user_id ?? "unknown"
```

## 20.1 Projection rule

```wire
value#case.field
```

is valid only if:

- `value` is a variant
- `case` is a known case of that variant
- `field` exists inside that case

The result is always nullable unless the compiler can prove the variant is already narrowed.

---

# 21. Template interpolation

Template interpolation uses expressions:

```wire
{{ agent.research.output.title }}
```

Template expressions are statically checked.

Unsafe expressions are rejected at compile time.

## 21.1 Unsafe nullable access

Given:

```wire
schema output {
    user: maybe {
        name: string
    }
}
```

Invalid:

```wire
{{ output.user.name }}
```

Compiler error:

```text
Cannot access `.name` through `output.user` because `output.user` is nullable.
Use `output.user?.name` and provide a fallback if a string is required.
```

Valid:

```wire
{{ output.user?.name ?? "Unknown user" }}
```

## 21.2 Unsafe variant access

Given:

```wire
schema message {
    content: variant type {
        text {
            text: string
        }

        image {
            url: string
        }
    }
}
```

Invalid:

```wire
{{ message.content.text }}
```

Compiler error:

```text
Cannot access `.text` directly on variant `message.content`.
Use variant projection or match.
```

Valid:

```wire
{{ message.content#text.text ?? "" }}
```

Valid:

```wire
{{ match message.content {
    text.text
    image.url
} }}
```

---

# 22. Dynamic block examples

## 22.1 Match a variant into a value

```wire
dynamic {
    value: match agent.someagent.some_variant {
        user_created.example.demo.some_number
        user_deleted.example.demo.some_number
    }
}
```

Both branches return `number`, so `value` is inferred as `number`.

## 22.2 Match with mixed branch types

```wire
dynamic {
    value: match agent.someagent.some_variant {
        user_created.example.demo.some_number
        user_deleted.example.demo.some_string
    }
}
```

If `some_number` is `number` and `some_string` is `string`, the compiler must reject the expression. Match branches may not mix result types.

## 22.3 Match with fallback

```wire
dynamic {
    value: match agent.someagent.some_variant {
        user_created.example.demo.some_number
        user_deleted.example.demo.some_number
        _ 0
    }
}
```

---

# 23. Recommended full schema example

```wire
schema participant {
    /// Participant name
    name: string

    /// Participant role
    role: string
}

schema research_summary {
    /// Research summary title
    title: string

    /// Primary participant for the summary
    lead: schema.participant

    /// All participants included in the summary
    participants: [schema.participant]

    /// Current publication status
    status: enum {
        draft
        ready
        published
    }

    /// Optional publication metadata. Field exists, value may be null.
    publication: maybe {
        published_at: string
        publisher: string
    }

    /// Main result returned by the research agent
    result: variant type {
        summary {
            text: string
            confidence: float
        }

        citations {
            items: [{
                title: string
                url: string
                quote: maybe string
            }]
        }

        error {
            code: string
            message: string
            retryable: boolean
        }
    }
}
```

---

# 24. Recommended dynamic examples

## 24.1 Extract a title from a variant

```wire
dynamic {
    title: match agent.research.result {
        summary.text
        citations.items[0]?.title
        error.message
    }
}
```

Notes:

- `summary.text` returns `string`
- `citations.items[0]?.title` returns `maybe string`
- `error.message` returns `string`

This may be invalid unless the nullable branch is handled.

Better:

```wire
dynamic {
    title: match agent.research.result {
        summary.text
        citations.items[0]?.title ?? "Untitled citation"
        error.message
    }
}
```

## 24.2 Extract an error message only when present

```wire
dynamic {
    error_message: agent.research.result#error.message
}
```

This produces:

```wire
maybe string
```

## 24.3 Extract an error message with fallback

```wire
dynamic {
    error_message: agent.research.result#error.message ?? "No error"
}
```

This produces:

```wire
string
```

---

# 25. JSON Schema mapping

## 25.1 Object schema

DSL:

```wire
schema user {
    id: string
    name: string
}
```

JSON Schema concept:

```json
{
  "type": "object",
  "required": ["id", "name"],
  "properties": {
    "id": { "type": "string" },
    "name": { "type": "string" }
  },
  "additionalProperties": false
}
```

## 25.2 Nullable field

DSL:

```wire
bio: maybe string
```

JSON Schema concept:

```json
{
  "type": ["string", "null"]
}
```

## 25.3 Enum

DSL:

```wire
status: enum {
    draft
    ready
    published
}
```

JSON Schema concept:

```json
{
  "type": "string",
  "enum": ["draft", "ready", "published"]
}
```

## 25.4 Variant

DSL:

```wire
event: variant type {
    user_created {
        user_id: string
    }

    user_deleted {
        user_id: string
    }
}
```

JSON Schema concept:

```json
{
  "oneOf": [
    {
      "type": "object",
      "required": ["type", "user_id"],
      "properties": {
        "type": { "const": "user_created" },
        "user_id": { "type": "string" }
      },
      "additionalProperties": false
    },
    {
      "type": "object",
      "required": ["type", "user_id"],
      "properties": {
        "type": { "const": "user_deleted" },
        "user_id": { "type": "string" }
      },
      "additionalProperties": false
    }
  ],
  "discriminator": {
    "propertyName": "type"
  }
}
```

---

# 26. Static type-checking rules

## 26.1 Field access

Plain field access:

```wire
a.b
```

is valid only when:

- `a` is a non-null object
- `b` exists on `a`
- `a` is not an unresolved variant

## 26.2 Nullable access

Safe access:

```wire
a?.b
```

is valid only when:

- `a` is `maybe object`
- `b` exists on the inner object

Result type:

```wire
maybe TypeOfB
```

## 26.3 Fallback

Fallback:

```wire
a ?? b
```

is valid only when:

- `a` is `maybe T`
- `b` is assignable to `T`

Result type:

```wire
T
```

## 26.4 Variant projection

Projection:

```wire
a#case.b
```

is valid only when:

- `a` is a variant
- `case` exists on the variant
- `b` exists inside that case

Result type:

```wire
maybe TypeOfB
```

## 26.5 Match

Match:

```wire
match value {
    case_a.path
    case_b.path
}
```

is valid only when:

- `value` is a variant or nullable value
- each case exists on the matched value
- the match is exhaustive, or `_` is provided
- all branches return the same type

---

# 27. Error message guidelines

The compiler should provide precise errors with actionable fixes.

## 27.1 Nullable access error

```text
Cannot access `.name` through `user` because `user` is nullable.
Use `user?.name` or provide a fallback with `??`.
```

## 27.2 Variant access error

```text
Cannot access `.email` directly on variant `event`.
Use `event#user_created.email` or `match event { ... }`.
```

## 27.3 Non-exhaustive match error

```text
Non-exhaustive match on `event`.
Missing cases: user_deleted, payment_failed.
Add the missing cases or use `_`.
```

## 27.4 Incompatible branch types

```text
Match branches return incompatible types.
- user_created returns number
- user_deleted returns string
Convert one branch explicitly so all branches return the same type.
```

---

# 28. Resolved design decisions

## 28.1 Match branches must not mix types

A `match` expression must produce one concrete result type.

Valid:

```wire
value: match event.payload {
    user_created.user_id
    user_deleted.user_id
}
```

Invalid:

```wire
value: match event.payload {
    user_created.user_id
    payment_failed.retry_count
}
```

If `user_id` is `string` and `retry_count` is `number`, the compiler must reject the expression.

## 28.2 No `null` type expression

`null` is not a valid standalone type expression.

Invalid:

```wire
value: null
```

Nullable values must use `maybe T`.

Valid:

```wire
value: maybe string
```

JSON `null` may still appear at runtime for `maybe` values, but users do not write `null` as a type.

## 28.3 `_` handles unmatched and nullable cases

The `_` branch is used for fallback handling. This includes unmatched variants and nullable values.

Valid:

```wire
value: match event.payload {
    user_created.user_id
    user_deleted.user_id
    _ "unknown"
}
```

Invalid:

```wire
value: match event.payload {
    user_created.user_id
    user_deleted.user_id
    null "unknown"
}
```

## 28.4 Schema names are lowercase snake_case

Valid:

```wire
schema research_summary {
    title: string
}
```

Invalid:

```wire
schema ResearchSummary {
    title: string
}
```

## 28.5 Shared field type references

Standalone schema assignment is invalid.

Invalid:

```wire
schema status = enum {
    draft
    ready
    published
}
```

Shared type-like fields should be declared inside a schema and referenced through `schema.<schema_name>.<field_name>`.

```wire
schema shared {
    status: enum {
        draft
        ready
        published
    }
}

schema some_other_schema {
    status: schema.shared.status
}
```

---

# 29. Summary of recommended syntax

## 29.1 Types

```wire
string
number
float
boolean
object
maybe T
[T]
[T; N]
(T1, T2, T3)
enum { draft, ready, published }
variant type {
    case_name {
        field: string
    }
}
schema.name
{
    field: Type
}
```

## 29.2 Expressions

```wire
value.a.b
value.a?.b
value.a?.b ?? "fallback"
value#case.field
match value {
    case_a.path
    case_b.path
    _ "fallback"
}
```

## 29.3 Descriptions

```wire
/// Description text
field: string
```

---

# 30. Final recommended direction

Use:

```wire
maybe T
```

for nullability.

Use:

```wire
enum { ... }
```

for fixed string values.

Use:

```wire
variant tag { ... }
```

for tagged polymorphic objects.

Use:

```wire
?. 
```

for safe nullable access.

Use:

```wire
??
```

for fallback values.

Use:

```wire
#case
```

for variant projection.

Use:

```wire
match value {
    case.path
    other_case.path
}
```

for expression-level variant handling without `if` / `else`.

The central safety rule is:

```text
Plain dot access is allowed only when the compiler can prove the path cannot encounter null and does not depend on a variant case.
```

This keeps the DSL concise while preserving strong compile-time safety.
