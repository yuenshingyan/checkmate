# 005 — Collect references and required inputs

- **Status:** Planned
- **Dependencies:** 003
- **Goal:** Distinguish snippet-local names, declaration dependencies, and external values.

## Scope

Add a scope-aware syntax visitor in `crates/checkmate/src/resolve.rs`. Inspect the
selected body, supplied signature, and output expression. Classify references in
value/type positions without treating field labels or method names as free variables.
Recognize generated function parameters as explicit replacements for surrounding locals.

## Non-goals

Inferring parameter types, evaluating local initializers, or resolving arbitrary methods.

## Acceptance tests

- `User { name, active: true }` references the type and the shorthand value, not `active`.
- A snippet-defined variable is not requested as an input.
- Signature and output-only dependencies are included.
- Missing outer inputs name the reference and location; earlier statements never execute.
- Shadowing and nested patterns agree with task 003's scope rules.
