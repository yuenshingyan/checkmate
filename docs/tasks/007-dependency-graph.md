# 007 — Build the dependency graph

- **Status:** Planned
- **Dependencies:** 006
- **Goal:** Discover transitive declaration requirements once and deterministically.

## Scope

Represent referenced declarations and their dependencies using stable IDs. Traverse
field types, alias targets, generic bounds, and other supported syntax. Deduplicate
nodes and track visiting/visited states. Distinguish legal recursive type relationships
from unsupported cycles; use stable emission ordering without assuming every graph is a DAG.

## Non-goals

Arbitrary helper-function evaluation or unsupported declaration kinds.

## Acceptance tests

- A struct containing another named type discovers both declarations.
- Shared dependencies appear once and output is deterministic.
- Recursive supported types terminate analysis without duplicate emission.
- Unsupported dependencies include the chain that led to them.
- An unresolved graph never produces a partially guessed expansion.
