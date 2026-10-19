# 014 — Document the feature and add examples

- **Status:** Planned
- **Dependencies:** 013
- **Goal:** Explain exactly what is automatic and where explicit inputs remain required.

## Scope

Update the usage guide and architecture document. Add a runnable example package for
supported struct-returning extraction without manual context. Document the supported
matrix, opt-in syntax, copied type identity, errors, and context-baseline review workflow.
Retain the basic example as a demonstration of ordinary extraction.

## Non-goals

Advertising full semantic inference, adding IDE integrations, or generating user tests.

## Acceptance

- All documented examples run under workspace tests.
- Documentation clearly separates implemented behavior from deferred work.
- The new example leaves its original snippet untouched and includes user-authored tests.
- Source/config/baseline paths and commands are valid from the documented directory.
- Backlog statuses reflect actual completion, not merely documentation of a plan.
