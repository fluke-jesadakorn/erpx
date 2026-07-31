-- Immutable conversion facts produced by the Marketing journey worker.
-- The write row is authoritative; the matching read row is rebuilt from its
-- outbox event and never becomes a command source.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_conversions (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    conversion_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    revenue_minor BIGINT CHECK (revenue_minor >= 0),
    currency TEXT CHECK (currency IS NULL OR (length(currency) = 3 AND currency = upper(currency))),
    occurred_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version = 1),
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, conversion_ref),
    UNIQUE (holding_ref, run_id, journey_id, profile_id, conversion_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_conversions_scope_idx
    ON erpx_tenant.marketing_journey_conversions
       (holding_ref, occurred_at DESC, conversion_ref);

ALTER TABLE erpx_tenant.marketing_journey_conversions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_conversions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journey_conversions
    ON erpx_tenant.marketing_journey_conversions;
CREATE POLICY tenant_scope_marketing_journey_conversions
    ON erpx_tenant.marketing_journey_conversions
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
