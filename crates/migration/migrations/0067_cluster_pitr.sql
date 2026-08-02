-- Cluster-scoped physical PITR. Tenant backup jobs remain logical-only;
-- physical recovery coordinates and object keys belong to one PostgreSQL
-- cluster chain and never to a tenant namespace.

CREATE OR REPLACE FUNCTION erpx_control.reject_tenant_pitr_job()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.kind = 'pitr' THEN
        RAISE EXCEPTION 'tenant PITR is unsupported; use cluster-scoped PITR';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS tenant_backup_jobs_reject_pitr
    ON erpx_control.tenant_backup_jobs;
CREATE TRIGGER tenant_backup_jobs_reject_pitr
    BEFORE INSERT OR UPDATE OF kind ON erpx_control.tenant_backup_jobs
    FOR EACH ROW
    EXECUTE FUNCTION erpx_control.reject_tenant_pitr_job();

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_clusters (
    cluster_ref TEXT PRIMARY KEY CHECK (length(trim(cluster_ref)) BETWEEN 1 AND 120),
    storage_namespace TEXT NOT NULL CHECK (length(trim(storage_namespace)) BETWEEN 3 AND 63),
    state TEXT NOT NULL CHECK (state IN ('configured', 'active', 'suspended', 'failed')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_chains (
    id UUID PRIMARY KEY,
    cluster_ref TEXT NOT NULL REFERENCES erpx_control.platform_pitr_clusters(cluster_ref),
    chain_ref TEXT NOT NULL CHECK (length(trim(chain_ref)) BETWEEN 1 AND 120),
    state TEXT NOT NULL CHECK (
        state IN ('scheduled', 'capturing', 'continuous', 'degraded', 'expired', 'failed')
    ),
    base_backup_object_key TEXT,
    base_backup_sha256 TEXT CHECK (base_backup_sha256 IS NULL OR length(base_backup_sha256) = 64),
    start_lsn TEXT,
    stop_lsn TEXT,
    timeline_id BIGINT CHECK (timeline_id IS NULL OR timeline_id > 0),
    retention_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (cluster_ref, chain_ref),
    CHECK (retention_until > created_at),
    CHECK (
        base_backup_object_key IS NULL
        OR base_backup_object_key LIKE 'platform/%/chains/%/base/%'
    )
);

CREATE INDEX IF NOT EXISTS platform_pitr_chains_retention_idx
    ON erpx_control.platform_pitr_chains (state, retention_until, id);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_wal_segments (
    chain_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_chains(id),
    segment_name TEXT NOT NULL CHECK (segment_name ~ '^[0-9A-F]{24}([.][a-z0-9]+)?$'),
    object_key TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    archived_at TIMESTAMPTZ NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (chain_id, segment_name),
    UNIQUE (object_key),
    CHECK (verified_at >= archived_at),
    CHECK (object_key LIKE 'platform/%/chains/%/wal/%')
);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_recovery_points (
    id UUID PRIMARY KEY,
    chain_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_chains(id),
    label TEXT NOT NULL CHECK (length(trim(label)) BETWEEN 1 AND 120),
    target_time TIMESTAMPTZ,
    target_lsn TEXT,
    timeline_id BIGINT NOT NULL CHECK (timeline_id > 0),
    before_marker TEXT,
    after_marker TEXT,
    wal_continuity_verified BOOLEAN NOT NULL DEFAULT false,
    state TEXT NOT NULL CHECK (state IN ('available', 'blocked', 'expired')),
    created_at TIMESTAMPTZ NOT NULL,
    CHECK ((target_time IS NULL) <> (target_lsn IS NULL)),
    UNIQUE (chain_id, label)
);

CREATE INDEX IF NOT EXISTS platform_pitr_recovery_points_chain_idx
    ON erpx_control.platform_pitr_recovery_points (chain_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_restore_rehearsals (
    id UUID PRIMARY KEY,
    recovery_point_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_recovery_points(id),
    restore_target_ref TEXT NOT NULL CHECK (length(trim(restore_target_ref)) BETWEEN 1 AND 120),
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'claimed', 'restoring', 'verifying', 'completed', 'failed', 'cleaned')
    ),
    worker_id UUID,
    lease_until TIMESTAMPTZ,
    verification_summary JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (
        jsonb_typeof(verification_summary) = 'object'
    ),
    failure_category TEXT,
    requested_by TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (
        (state = 'claimed' AND worker_id IS NOT NULL AND lease_until IS NOT NULL)
        OR state <> 'claimed'
    )
);

CREATE INDEX IF NOT EXISTS platform_pitr_restore_rehearsals_due_idx
    ON erpx_control.platform_pitr_restore_rehearsals (state, created_at, id)
    WHERE state IN ('pending', 'claimed', 'restoring', 'verifying');

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_promotion_audit (
    id UUID PRIMARY KEY,
    rehearsal_id UUID NOT NULL REFERENCES erpx_control.platform_pitr_restore_rehearsals(id),
    actor_ref TEXT NOT NULL,
    runtime_stopped BOOLEAN NOT NULL,
    confirmation_digest TEXT NOT NULL CHECK (length(confirmation_digest) = 64),
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'rejected')),
    occurred_at TIMESTAMPTZ NOT NULL
);
