-- Disposable Marketing Campaign Budget projection. Financial actuals are
-- composed from Procurement and Finance facts and are not authoritative here.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_read;

CREATE TABLE erpx_marketing_read.campaign_budget_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'marketing.campaign_budgets'
        CHECK (projection_name = 'marketing.campaign_budgets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    budget_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    budget_code TEXT NOT NULL,
    budget_name TEXT NOT NULL,
    period TEXT NOT NULL CHECK (period IN ('annual', 'quarterly', 'campaign')),
    budget_status TEXT NOT NULL CHECK (budget_status IN (
        'draft', 'review', 'approved', 'running', 'completed', 'cancelled'
    )),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requested_minor BIGINT NOT NULL CHECK (requested_minor > 0),
    approved_minor BIGINT CHECK (approved_minor > 0),
    reserved_minor BIGINT NOT NULL CHECK (reserved_minor >= 0),
    available_minor BIGINT NOT NULL,
    dimensions JSONB NOT NULL,
    allocations JSONB NOT NULL,
    reservations JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, budget_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (tenant_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (tenant_id, generation, scope_key, budget_code),
    CHECK (available_minor = COALESCE(approved_minor, 0) - reserved_minor)
);

CREATE INDEX marketing_budget_read_scope_campaign_status
    ON erpx_marketing_read.campaign_budget_projection
        (tenant_id, generation, scope_key, campaign_id, budget_status,
         aggregate_updated_at DESC, budget_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_marketing_read','campaign_budget_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.%I '
            'USING (tenant_id = erpx_cqrs_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_cqrs_read.current_tenant_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;
