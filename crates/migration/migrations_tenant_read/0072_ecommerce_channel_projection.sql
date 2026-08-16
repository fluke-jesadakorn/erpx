-- Disposable generation-managed channel governance projection.
CREATE TABLE erpx_ecommerce_read.channel_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'ecommerce.channels',
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    channel_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    tenant_scope JSONB NOT NULL,
    channel_code TEXT NOT NULL,
    channel_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, channel_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation)
      ON DELETE CASCADE
);

CREATE INDEX ecommerce_channel_scope
    ON erpx_ecommerce_read.channel_projection
       (tenant_id, generation, scope_key, channel_status, channel_id);

ALTER TABLE erpx_ecommerce_read.channel_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_ecommerce_read.channel_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON erpx_ecommerce_read.channel_projection
USING (tenant_id = erpx_ecommerce_read.current_tenant_id())
WITH CHECK (tenant_id = erpx_ecommerce_read.current_tenant_id());
