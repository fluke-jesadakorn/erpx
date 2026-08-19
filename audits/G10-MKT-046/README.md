# G10-MKT-046 — bounded Marketing journey domain

The Marketing domain now contains a holding-scoped, versioned Journey graph
with the required campaign lifecycle, validated trigger/wait/channel/branch/
segment-change/CRM-handoff/conversion/action definitions, deterministic node
ordering and reachability checks, and a resume-safe execution cursor. Cursor
advancement emits stable idempotency facts and enforces wait deadlines,
branch decisions, expected versions, scope containment, and paused/resumed
state. No side effects, provider calls, CRM mutations, or delivery authority
are present in this domain packet.

Independent evidence:

- `cargo test -p erpx-domain marketing_journey -- --nocapture` — PASS; 3
  tests.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS after
  main-agent review fixes.
- `cargo fmt --all --check` — PASS.

Worker persistence, journey scheduling, provider delivery, consent rechecks
at schedule/delivery, n8n approval, durable retries, and Marketing/CRM UI
remain open.

