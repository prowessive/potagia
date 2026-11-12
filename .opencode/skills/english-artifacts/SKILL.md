---
name: English Repository Artifacts
description: Write code identifiers, comments, rustdoc, error messages, logs, tests, commit messages, and project documentation in English. Use whenever creating or modifying repository artifacts.
---

# English Repository Artifacts

Write every artifact committed to the repository in English:

- identifiers: modules, types, traits, functions, variables, and constants
- comments and `// SAFETY:` notes
- rustdoc, README, and design documents
- error messages, logs, and CLI help
- test names and test data descriptions
- commit messages and pull request descriptions

Communicate with the user in their language. The language of the conversation
does not change the language of the repository.

## Naming

- Use English, intention-revealing names.
- Follow Rust conventions: `snake_case` for functions and modules,
  `UpperCamelCase` for types and traits, and `SCREAMING_SNAKE_CASE` for
  constants.
- Avoid abbreviations unless they are domain-standard.
- Do not mix languages inside one identifier.
- When modifying existing non-English code, keep the change scoped. Propose a
  separate rename instead of renaming unrelated identifiers.

## Documentation

- Give every public item rustdoc that states its purpose and contract.
- Document `# Errors` when a function returns `Result`.
- Document `# Panics` when a function can panic.
- Add `# Examples` when an example clarifies usage; keep examples compilable or
  mark them `no_run`/`ignore` deliberately.
- State units, ranges, and defaults for numeric parameters.
- Keep documentation synchronized with behavior.
- Document module boundaries and invariants that are not obvious from types.
- Use American English spelling consistently.

## Comments

- Prefer expressive code over comments.
- Comment only why: non-obvious decisions, trade-offs, invariants, external
  constraints, or workarounds.
- Do not restate the code or narrate the implementation.
- Delete stale comments and commented-out code.
- Use `// SAFETY:` to justify every `unsafe` block.

## Messages

- Write error messages that state what failed and what the caller can do next,
  without exposing secrets.
- Use lowercase, direct sentences for `Display` messages and logs.
- Keep commit messages in the imperative mood, for example
  `Add YAML configuration loader`.
- Put the reason for a change in the commit body when it is not obvious.

## Review checklist

- [ ] Are all new identifiers, comments, docs, messages, and tests in English?
- [ ] Is public behavior documented with rustdoc?
- [ ] Are error and panic conditions documented?
- [ ] Do comments explain why rather than what?
- [ ] Are examples and units accurate?
- [ ] Is the commit message in English and imperative?
