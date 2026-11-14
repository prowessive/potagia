---
name: Clean Code
description: Apply clean-code principles to Rust changes, including intention-revealing names, small focused functions, flat control flow, explicit errors, and no duplication or dead code. Use during implementation and review.
---

# Clean Code

## Names

- Name types, functions, variables, and modules for intent, not implementation.
- Use domain terms consistently.
- Avoid abbreviations unless they are standard in the domain.
- Prefer `is_`, `has_`, and `can_` for booleans; `try_` for fallible operations.
- Do not encode types in names.

## Functions and types

- Keep functions small and focused on one level of abstraction.
- Keep types cohesive; split types that change for unrelated reasons.
- Prefer early returns and guard clauses to nested conditionals.
- Avoid boolean parameters that select behavior; use enums or separate functions.
- Avoid long parameter lists; group related values in a struct when it improves
  the API.
- Make invalid states unrepresentable with types.

## Duplication and abstraction

- Remove duplicated logic when the duplication is real, not incidental.
- Do not introduce a generic abstraction for two similar-looking cases without a
  third use case.
- Delete dead code, unused parameters, commented-out code, and speculative
  extension points.
- Keep changes scoped; do not refactor unrelated code in the same change unless
  required.

## Errors

- Use `Result` for recoverable failures and custom error types for domain errors.
- Propagate with `?`; add context at boundaries.
- Avoid `unwrap` and `expect` on runtime input. Use `expect` only for proven
  invariants with a clear message.
- Do not panic in library code for invalid user input.
- Preserve the error source when wrapping errors.
- Never expose secrets in error messages or logs.

## Comments

- Prefer expressive code over comments.
- Comment only why: non-obvious trade-offs, invariants, external constraints, or
  workarounds.
- Do not narrate what the code does.
- Delete stale comments and commented-out code.
- Use `// SAFETY:` for every `unsafe` block.

## Dependencies and configuration

- Add a dependency only when the standard library or an existing crate cannot
  solve the problem.
- Justify new dependencies and keep them minimal.
- Keep configuration explicit; avoid hidden global state.
- Inject external dependencies so behavior is testable.

## Quality gates

Run before reporting completion:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

- Do not disable lints or tests to make a check pass.
- If a check cannot run, report it explicitly.

## Review checklist

- [ ] Do names reveal intent?
- [ ] Is each function and type focused?
- [ ] Is control flow flat and easy to follow?
- [ ] Are errors explicit and safe?
- [ ] Is there duplication or dead code?
- [ ] Are comments limited to non-obvious reasoning?
- [ ] Are new dependencies justified?
- [ ] Do fmt, clippy, and tests pass?
