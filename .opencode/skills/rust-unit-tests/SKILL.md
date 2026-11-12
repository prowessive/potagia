---
name: Rust Unit Tests
description: Add and maintain deterministic Rust unit tests for every feature, behavior change, and bug fix, covering happy paths, edge cases, errors, and state transitions. Use whenever implementing or reviewing Rust code.
---

# Rust Unit Tests

## Coverage rule

Every new feature, behavior change, or bug fix includes unit tests. No
implementation is complete without tests.

## Placement

- Put unit tests in a `#[cfg(test)] mod tests` in the same module as the code
  under test.
- Use `use super::*;` to access the module API.
- Test through public behavior; do not assert private implementation details.
- If logic is hard to test, extract it into a focused function or type with a
  clear API.
- Use integration tests under `tests/` for cross-module behavior when unit tests
  would be artificial.

## Structure

Follow Arrange-Act-Assert:

```rust
#[test]
fn rejects_port_above_u16_max() {
    let mut config = valid_config();

    let error = config.set_port(70_000);

    assert!(matches!(error, Err(ConfigError::InvalidPort { .. })));
}
```

- Test one behavior per test.
- Use descriptive names that state the expected behavior, for example
  `returns_default_when_section_is_missing`.
- Keep setup helpers small and named after the scenario they build; avoid a
  shared mutable fixture.

## Cases to cover

- happy path
- empty, zero, minimum, maximum, and boundary values
- invalid input and malformed data
- every error variant that can be triggered
- state transitions, especially invalid transitions
- optional and missing configuration
- Unicode and multi-byte input when text is processed
- IPv4 and IPv6 when addresses are parsed or formatted

## Determinism and isolation

- Tests must be deterministic, order-independent, and safe to run in parallel.
- Do not depend on the real clock, current directory, environment variables,
  network, or external services.
- Inject dependencies behind traits and use fakes or stubs.
- Use temporary directories when the filesystem is unavoidable and clean up
  after the test.
- Do not use `std::env::set_var` in tests; pass values through a function
  parameter or an injected lookup.
- Avoid sleeps and timing-based assertions. Use deterministic synchronization
  primitives when concurrency must be tested.

## Async tests

- Use `#[tokio::test]` for async code.
- Use `#[tokio::test(flavor = "multi_thread")]` only when the behavior requires
  multiple threads.
- Assert on returned values and errors, not on scheduler timing.

## Errors

- Prefer matching structured error variants with `matches!` over asserting on
  message substrings.
- Assert that secrets do not appear in `Debug` output or error messages.
- For serialization boundaries, test invalid input and round trips.

## Bug fixes

Add a regression test that fails before the fix and passes after it. Keep the
test focused on the reported behavior.

## Running tests

Run the full suite before reporting completion:

```sh
cargo test
```

For a focused loop, run the affected module or test by name:

```sh
cargo test config::tests
cargo test rejects_port_above_u16_max
```

Do not claim tests pass without running them. If a test cannot run, explain why.

## Review checklist

- [ ] Does every new behavior have a test?
- [ ] Are boundary and error cases covered?
- [ ] Is each test focused on one behavior?
- [ ] Are tests deterministic and isolated?
- [ ] Do async tests use the right runtime?
- [ ] Does a bug fix include a regression test?
- [ ] Does the full suite pass?
