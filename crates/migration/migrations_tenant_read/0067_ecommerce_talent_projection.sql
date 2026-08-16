-- Disposable owner-scoped E-commerce and Talent projections.
CREATE SCHEMA IF NOT EXISTS erpx_ecommerce_read;
CREATE SCHEMA IF NOT EXISTS erpx_talent_read;

CREATE OR REPLACE FUNCTION erpx_ecommerce_read.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_talent_read.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;

CREATE TABLE erpx_ecommerce_read.listing_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'ecommerce.listings', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, listing_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    channel_id UUID NOT NULL, listing_status TEXT NOT NULL, price_minor BIGINT NOT NULL,
    currency TEXT NOT NULL, available BOOLEAN NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, listing_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_ecommerce_read.journey_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'ecommerce.journeys', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL, owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')),
    owner_principal_id UUID NOT NULL, lifecycle_status TEXT NOT NULL, order_ref JSONB,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_talent_read.candidate_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'talent.candidate', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL, authority_realm TEXT NOT NULL CHECK (authority_realm = 'candidate'),
    candidate_principal_id UUID NOT NULL, lifecycle_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_talent_read.workforce_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'talent.workforce', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL, authority_realm TEXT NOT NULL CHECK (authority_realm = 'workforce'),
    lifecycle_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE INDEX ecommerce_listing_scope ON erpx_ecommerce_read.listing_projection(tenant_id, generation, scope_key, listing_status, listing_id);
CREATE INDEX ecommerce_journey_owner ON erpx_ecommerce_read.journey_projection(tenant_id, generation, owner_realm, owner_principal_id, projected_at DESC);
CREATE INDEX talent_candidate_owner ON erpx_talent_read.candidate_projection(tenant_id, generation, candidate_principal_id, projected_at DESC);
CREATE INDEX talent_workforce_scope ON erpx_talent_read.workforce_projection(tenant_id, generation, scope_key, record_type, projected_at DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_ecommerce_read','listing_projection'),('erpx_ecommerce_read','journey_projection'),
        ('erpx_talent_read','candidate_projection'),('erpx_talent_read','workforce_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.%I '
            'USING (tenant_id = %I.current_tenant_id()) '
            'WITH CHECK (tenant_id = %I.current_tenant_id())',
            schema_name, table_name, schema_name, schema_name
        );
    END LOOP;
END;
$$;
