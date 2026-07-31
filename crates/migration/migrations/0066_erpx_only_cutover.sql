-- ERPX development cutover authority.
--
-- The singleton is intentionally immutable: this repository no longer
-- contains a legacy command bridge, so command ownership cannot be changed by
-- configuration or an environment fallback. Any future ownership model needs
-- an explicit forward migration with its own audit evidence.
CREATE TABLE IF NOT EXISTS erpx_control.command_ownership (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    mode TEXT NOT NULL CHECK (mode = 'erpx_only'),
    evidence_id UUID NOT NULL UNIQUE,
    established_at TIMESTAMPTZ NOT NULL,
    established_by TEXT NOT NULL CHECK (btrim(established_by) <> '')
);

CREATE TABLE IF NOT EXISTS erpx_control.command_ownership_events (
    event_id UUID PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode = 'erpx_only'),
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> ''),
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    evidence JSONB NOT NULL
);

CREATE OR REPLACE FUNCTION erpx_control.reject_ownership_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'ERPX command ownership evidence is append-only'
        USING ERRCODE = '55000';
END;
$$;

DROP TRIGGER IF EXISTS command_ownership_immutable
    ON erpx_control.command_ownership;
CREATE TRIGGER command_ownership_immutable
BEFORE UPDATE OR DELETE ON erpx_control.command_ownership
FOR EACH ROW EXECUTE FUNCTION erpx_control.reject_ownership_mutation();

DROP TRIGGER IF EXISTS command_ownership_events_append_only
    ON erpx_control.command_ownership_events;
CREATE TRIGGER command_ownership_events_append_only
BEFORE UPDATE OR DELETE ON erpx_control.command_ownership_events
FOR EACH ROW EXECUTE FUNCTION erpx_control.reject_ownership_mutation();

CREATE OR REPLACE FUNCTION erpx_control.require_erpx_command_owner()
RETURNS void
LANGUAGE plpgsql
STABLE
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM erpx_control.command_ownership
        WHERE singleton AND mode = 'erpx_only'
    ) THEN
        RAISE EXCEPTION 'authoritative ERPX command ownership is unavailable'
            USING ERRCODE = '55000';
    END IF;
END;
$$;

INSERT INTO erpx_control.command_ownership
    (singleton, mode, evidence_id, established_at, established_by)
VALUES (
    TRUE,
    'erpx_only',
    '4e1a4e86-f821-5f73-84e7-04f30bcc055c',
    now(),
    'cutover:migration-0066'
)
ON CONFLICT (singleton) DO NOTHING;

INSERT INTO erpx_control.command_ownership_events
    (event_id, mode, actor_ref, correlation_id, occurred_at, evidence)
VALUES (
    '4e1a4e86-f821-5f73-84e7-04f30bcc055c',
    'erpx_only',
    'cutover:migration-0066',
    '86da8dba-644d-5f9a-90b7-8d02ab39e6db',
    now(),
    '{"event":"erpx.command_ownership.established.v1","rollback":"intentionally_absent"}'::jsonb
)
ON CONFLICT (event_id) DO NOTHING;

INSERT INTO erpx_control.audit_events
    (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
VALUES (
    '7d1c7d6f-1786-5a52-aa06-4bbce4261f61',
    'cutover:migration-0066',
    NULL,
    'platform.command_ownership.establish',
    'Persist ERPX-only development command authority',
    '86da8dba-644d-5f9a-90b7-8d02ab39e6db',
    '{"mode":"erpx_only","append_only":true}'::jsonb
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO erpx_control.outbox_events (
    id, event_type, event_version, aggregate_type, aggregate_id,
    aggregate_version, tenant_scope, actor, correlation_id, causation_id,
    idempotency_key, payload, occurred_at
)
VALUES (
    '8a84e11c-0593-5a2c-b86a-9e07a6b1ca56',
    'erpx.command_ownership.established.v1',
    1,
    'command_ownership',
    '4e1a4e86-f821-5f73-84e7-04f30bcc055c',
    1,
    NULL,
    '{"kind":"system","subject":"cutover:migration-0066"}'::jsonb,
    '86da8dba-644d-5f9a-90b7-8d02ab39e6db',
    NULL,
    'cutover:erpx-only:v1',
    '{"mode":"erpx_only","rollback":"intentionally_absent"}'::jsonb,
    now()
)
ON CONFLICT (id) DO NOTHING;

-- Idempotent development fixture receipts. The fixture command writes one
-- bounded row per context and one outbox event per successful command.
CREATE TABLE IF NOT EXISTS erpx_control.demo_seed_commands (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    fixture_version TEXT NOT NULL CHECK (btrim(fixture_version) <> ''),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    completed_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_control.demo_contexts (
    context_key TEXT PRIMARY KEY CHECK (btrim(context_key) <> ''),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('ready', 'unconfigured', 'disabled')),
    sample_count BIGINT NOT NULL CHECK (sample_count >= 0),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);
