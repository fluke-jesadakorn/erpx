-- Durable, profile-redacted handoffs from Marketing journeys to an
-- authoritative, holding-bound provider account. Provider endpoints and
-- credentials remain in the control-plane provider-account aggregate.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_integration_invocations (
    holding_ref TEXT NOT NULL
        CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    invocation_id UUID NOT NULL,
    integration_ref TEXT NOT NULL
        CHECK (btrim(integration_ref) <> '' AND octet_length(integration_ref) <= 160),
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    node_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    state TEXT NOT NULL
        CHECK (state IN ('queued', 'sending', 'succeeded', 'retry_scheduled', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    provider_reference TEXT
        CHECK (provider_reference IS NULL OR (btrim(provider_reference) <> '' AND octet_length(provider_reference) <= 255)),
    last_error TEXT
        CHECK (last_error IS NULL OR (btrim(last_error) <> '' AND octet_length(last_error) <= 128)),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    claim_id UUID,
    claimed_by UUID,
    claim_until TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, invocation_id),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_journey_integration_due_idx
    ON erpx_tenant.marketing_journey_integration_invocations
       (holding_ref, state, next_attempt_at, claim_until, invocation_id);

ALTER TABLE erpx_tenant.marketing_journey_integration_invocations ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_integration_invocations FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journey_integration_invocations
    ON erpx_tenant.marketing_journey_integration_invocations;
CREATE POLICY tenant_scope_marketing_journey_integration_invocations
    ON erpx_tenant.marketing_journey_integration_invocations
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
