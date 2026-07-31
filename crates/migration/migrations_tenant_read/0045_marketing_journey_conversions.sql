-- Rebuildable Marketing journey conversion read model.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

ALTER TABLE IF EXISTS erpx_tenant.marketing_projection_checkpoints
    DROP CONSTRAINT IF EXISTS marketing_projection_checkpoints_aggregate_type_check;
ALTER TABLE IF EXISTS erpx_tenant.marketing_projection_checkpoints
    ADD CONSTRAINT marketing_projection_checkpoints_aggregate_type_check
    CHECK (aggregate_type IN (
        'marketing_profile', 'marketing_consent',
        'marketing_suppression', 'marketing_audience',
        'marketing_journey_conversion',
        'marketing_crm_accepted_lead_handoff', 'marketing_crm_opportunity',
        'marketing_crm_attribution'
    ));

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_journey_conversion_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    conversion_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    revenue_minor BIGINT CHECK (revenue_minor >= 0),
    currency TEXT CHECK (currency IS NULL OR (length(currency) = 3 AND currency = upper(currency))),
    occurred_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, conversion_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_conversion_read_scope_idx
    ON erpx_tenant.marketing_journey_conversion_read_rows
       (holding_ref, occurred_at DESC, conversion_ref);

ALTER TABLE erpx_tenant.marketing_journey_conversion_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_journey_conversion_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_journey_conversion_read_rows
    ON erpx_tenant.marketing_journey_conversion_read_rows;
CREATE POLICY tenant_scope_marketing_journey_conversion_read_rows
    ON erpx_tenant.marketing_journey_conversion_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
