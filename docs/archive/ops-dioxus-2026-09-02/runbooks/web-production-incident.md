# ERPX Web Production Incident Runbook

> **Deprecated — Dioxus era (archived 2026-09-02).** This runbook describes the Dioxus Web / Tonic / worker stack (`:3004`/`:50051`/`:9000`) which was removed from `development` on `d15909bb`. It remains on `backup/rust-2026-09-02` for reference. Living runbooks for the Next.js era will live under `ops/runbooks/nextjs-*.md` (TBD).

This runbook applies to the Dioxus Web, typed API, worker, PostgreSQL, and
object-storage production stack. Preserve correlation IDs, audit events, and
outbox evidence during every response.

## Server unavailable

1. Confirm load-balancer health and `/health`, `/ready`, and `/metrics` status.
2. Compare deployment revision, environment references, database pool health,
   and worker heartbeats with the last known-good release.
3. Stop rollout expansion. Redeploy the retained compatible legacy bundle when
   recovery exceeds the incident objective; never run typed and legacy route
   authorities concurrently.

## HTTP errors

Group failures by route ID, status category, correlation ID, and revision.
Separate authorization denials from transport failures and projection errors.
Retry only commands whose idempotency key is known and retained.

## Authentication failures

Check identity-provider reachability, secure-cookie configuration, CSRF origin
validation, rate-limit telemetry, and account lockouts. Development personas
must remain disabled in the production profile.

## Projection lag

Inspect projection backlog, oldest event age, claims, retries, and worker
heartbeats. Do not bypass a stale projection with direct database writes.

## Dead letters

Quarantine the failing event, preserve its correlation and causation IDs, and
determine whether the failure is deterministic before replay. Never delete an
audit or domain outbox row to clear an alert.

## Worker heartbeat

Confirm the recorded worker process, lease, handler name, and database route.
Restart only the affected handler, then verify backlog and projection freshness.
