# 001 — Define the supported subset and safety contract

- **Status:** Planned
- **Dependencies:** None
- **Goal:** Define exactly what automatic discovery may transform before implementing it.

## Scope

Create a design document covering same-file named types, explicit imports, original
versus copied type identity, local inputs, visibility, and deterministic generation.
Specify a conservative first subset for structs/enums/type aliases and implementations.
Treat ambiguous traits, macro-produced items, conditional compilation, context-sensitive
macros, and unsupported attributes as explicit barriers unless safely resolved.
Decide the opt-in syntax; `context = "auto"` is a proposal, not an existing option.

## Non-goals

Type inference, executing surrounding statements, rewriting original sources, or
claiming that successful compilation proves semantic equivalence.

## Acceptance

- A supported/unsupported matrix includes a named reason for every excluded case.
- Examples distinguish reusable original types from copied function-local types.
- Existing extraction remains the default, including raw statement fixtures.
- The document specifies how unresolved input names and copied-type limitations surface.
