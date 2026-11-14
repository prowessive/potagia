---
name: Rust Object-Oriented Design
description: Design and implement idiomatic Rust objects using encapsulation, traits, composition, dynamic dispatch, and the state pattern. Use for Rust architecture, structs, traits, polymorphism, and design refactors.
---

# Rust Object-Oriented Design

Apply the object-oriented features of Rust described in The Rust Book, chapter 18:
<https://doc.rust-lang.org/book/ch18-00-oop.html>

## Model objects explicitly

- Represent an object as a struct or enum holding its data, with behavior in
  `impl` blocks.
- Keep fields private. Expose constructors that validate invariants and methods
  that preserve them.
- Prefer a small, intention-revealing public API over broad getters and setters.
- Use newtypes to give meaning to primitive values and to prevent argument swaps.
- Return `Result` from constructors when validation can fail.

## Prefer composition and traits

- Rust has no inheritance. Share behavior with traits and compose small types
  instead of building class hierarchies.
- Define traits around capabilities, not around "is-a" relationships.
- Use default trait methods for shared behavior when the default is correct for
  most implementors.
- Prefer generics and `impl Trait` for static dispatch.
- Use `Box<dyn Trait>`, `&dyn Trait`, or `Arc<dyn Trait>` only when the concrete
  type is chosen at runtime.
- Accept borrowed trait objects (`&dyn Trait`) in parameters unless ownership is
  required.

## Model state with enums

- Use enums when a value has a fixed set of states and each state carries
  different data.
- Prefer exhaustive `match` over boolean flags and stringly-typed state.
- Apply the state pattern when behavior changes with state and the transitions
  are non-trivial.
- Keep transition methods on the type that owns the state.

## Respect ownership and abstraction boundaries

- Borrow instead of cloning. Clone only at ownership boundaries or when the API
  requires it.
- Keep lifetimes explicit only when they clarify the relationship between inputs
  and outputs.
- Use `Arc` and locks only for genuine shared ownership or cross-thread mutation.
- Hide implementation details behind modules and public APIs.
- Do not add abstraction without a current use case.

## Reuse standard traits

- Implement `Display` for user-facing formatting and `Debug` for diagnostics.
- Implement `From` and `TryFrom` for conversions; avoid ad-hoc `to_*` methods
  when a standard trait applies.
- Implement `Error` for error types and keep `source()` accurate.
- Derive `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, and `Default` only when the
  semantics are correct.
- Redact secrets in `Debug` output.

## Avoid

- Emulating class inheritance with nested structs and trait boilerplate.
- Public fields that let callers break invariants.
- `Box<dyn Trait>` by default when a generic would be simpler and faster.
- Trait objects for traits with generic methods, unless the API is redesigned.
- Getters and setters for every field.
- Sprawling `impl` blocks that mix unrelated responsibilities.

## Review checklist

- [ ] Does each type have one clear responsibility?
- [ ] Are invariants enforced by construction and private fields?
- [ ] Is composition used instead of inheritance emulation?
- [ ] Is dispatch static unless runtime polymorphism is required?
- [ ] Are state transitions explicit and exhaustive?
- [ ] Are clones and locks justified?
- [ ] Does the public API expose only what callers need?
