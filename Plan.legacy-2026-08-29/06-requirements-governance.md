# Requirements Governance

## Baseline control

This document controls the library rather than product runtime behavior. The
baseline owner is the Product Council. Domain Owners approve business
requirements, Architecture approves ownership and interfaces, Security and
Privacy approve protection controls, Operations approves continuity controls,
and UX approves channel and accessibility behavior.

## Atomicity and stable identity

- One normative leaf expresses one testable outcome.
- A capability family may group leaves but is explicitly non-normative.
- Existing atomic IDs remain stable; split children add alphabetic suffixes.
- IDs are never renumbered for presentation and are never reused after
  retirement.
- Requirement definitions, verification metadata, scenarios, and traceability
  form one controlled record even when presented in separate documents.

## Required fields

Every definition supplies title, `ERPX shall...`, rationale, success, denial,
failure/recovery, source, and scenario. The verification catalog deterministically
supplies method, `EVD-*` evidence identity, existing-evidence interpretation,
and sign-off roles. A missing field is a validation failure.

## Source states

Sources are marked current repository evidence, accepted ADR, accepted FSD,
accepted design, official regulatory authority, or required future evidence.
`Required evidence` states what acceptance must produce. `Existing evidence`
only identifies material already present in the source snapshot and does not
claim implementation completion.

## Decision lifecycle

FSDs use Draft, Accepted, Superseded, or Retired. Only Accepted decisions are
normative sources. Every supersession names the previous authority and affected
requirement families. Review is event-driven and the baseline version changes
only when the accepted target changes.

## Non-goals

This library does not hold work status, delivery gaps, priority, estimates,
release dates, named-person ownership, or compliance claims. Those concerns
belong to separately governed delivery and certification systems.

## Architecture governance

Baseline 2.0.0 applies [FSD-006](decisions/FSD-006-clean-rebuild-architecture-and-cutover.md)
to every bounded context. Architecture review must verify DDD ownership, Clean
Architecture dependency direction, Hexagonal consumer-owned ports/adapters,
and CQRS write/read separation as one coherent model. A transport or storage
technology cannot become domain authority, a read projection cannot enforce a
write invariant, and a shared integration type cannot import a context core.

The typed implementation catalog is generated or validated against this
normative library. Drift in context IDs, module/record slugs, service IDs,
event namespaces, channel policies, realms, or schema generation is a blocking
baseline failure rather than an undocumented compatibility exception.
