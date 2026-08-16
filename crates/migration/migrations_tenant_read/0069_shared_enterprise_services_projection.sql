-- Disposable shared-service projections. They are rebuilt only from typed
-- owner descriptors or shared-service coordination artifacts.

CREATE SCHEMA IF NOT EXISTS erpx_shared_read;

CREATE OR REPLACE FUNCTION erpx_shared_read.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_shared_read.work_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.work' CHECK (projection_name = 'shared.work'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    source_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    source_record_id UUID NOT NULL,
    source_record_type TEXT NOT NULL,
    source_record_version BIGINT NOT NULL CHECK (source_record_version > 0),
    source_position BIGINT NOT NULL CHECK (source_position > 0),
    freshness TEXT NOT NULL CHECK (freshness IN ('fresh','stale','unknown')),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, source_id, work_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.notification_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.notifications' CHECK (projection_name = 'shared.notifications'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    notification_id UUID NOT NULL,
    recipient_ref TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    inbox_state TEXT NOT NULL CHECK (inbox_state IN ('unread','read','acknowledged','action_required','withdrawn')),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, notification_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.reference_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.references' CHECK (projection_name = 'shared.references'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    owner_context TEXT NOT NULL,
    record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    stable_code TEXT NOT NULL,
    classification TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    descriptor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.search_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_read.current_tenant_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.search' CHECK (projection_name = 'shared.search'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    owner_context TEXT NOT NULL,
    record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    classification TEXT NOT NULL,
    authorized_document TSVECTOR NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    authorization_policy_version BIGINT NOT NULL CHECK (authorization_policy_version > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.bulk_job_projection (
    tenant_id UUID NOT NULL DEFAULT erpx_shared_read.current_tenant_id(),
    projection_name TEXT NOT NULL CHECK (projection_name IN ('shared.reports','shared.imports')),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    job_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    job_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, job_id),
    FOREIGN KEY (tenant_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);

CREATE INDEX shared_work_scope_due ON erpx_shared_read.work_projection(tenant_id, generation, scope_key, (state->>'due_at'), source_id, work_id);
CREATE INDEX shared_notification_recipient ON erpx_shared_read.notification_projection(tenant_id, generation, recipient_ref, inbox_state, projected_at DESC);
CREATE INDEX shared_reference_code ON erpx_shared_read.reference_projection(tenant_id, generation, scope_key, owner_context, stable_code);
CREATE INDEX shared_search_tenant_scope ON erpx_shared_read.search_projection(tenant_id, generation, scope_key, owner_context, record_type);
CREATE INDEX shared_search_document ON erpx_shared_read.search_projection USING GIN(authorized_document);
CREATE INDEX shared_bulk_job_scope ON erpx_shared_read.bulk_job_projection(tenant_id, generation, scope_key, job_status, projected_at DESC);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'work_projection','notification_projection','reference_projection',
        'search_projection','bulk_job_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_shared_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_shared_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY shared_read_tenant_isolation ON erpx_shared_read.%I '
            'USING (tenant_id = erpx_shared_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_shared_read.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- schema_generation=2, database_role=tenant_read, migration_set=tenant-read-v2.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';
