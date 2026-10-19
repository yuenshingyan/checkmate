# 005 — Collect references and required inputs

- **Status:** Complete
- **Dependencies:** 003, 004
- **Goal:** Distinguish snippet-local names, declaration dependencies, and external values.

## Result

Implemented the test-gated collector in `resolve.rs` and `resolve/collect.rs`, with
26 focused tests. It compares original declaration identities with an analysis
function containing only the supplied signature, selected lines, and optional output.
Original constant-pattern identities are preserved when indexing that incomplete
analysis function, including or-patterns; earlier initializers are never copied or run.

The result distinguishes snippet locals, explicit inputs, original declarations/imports,
and generated-only locals/imports. Missing inputs and capture/shadowing changes fail
with source-aware diagnostics. Unknown paths, type-versus-const generic arguments,
qualified final targets, and method/trait operations remain explicit deferred work
for subsequent tasks, not inferred semantics. The public macro remains unchanged.

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
