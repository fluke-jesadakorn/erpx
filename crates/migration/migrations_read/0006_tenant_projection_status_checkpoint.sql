CREATE SCHEMA IF NOT EXISTS erpx_control;

-- A single durable source cursor for the metadata-only scheduler status
-- projection. The source tuple is strictly ordered by mutable source update
-- time and job identity, so a retry can replay a page without skipping an
-- update to an older job.
CREATE TABLE IF NOT EXISTS erpx_control.tenant_projection_status_checkpoints (
    projection_name TEXT PRIMARY KEY CHECK (
        length(trim(projection_name)) BETWEEN 1 AND 120
        AND projection_name ~ '^[A-Za-z0-9_.-]+$'
    ),
    source_updated_at TIMESTAMPTZ,
    source_job_id UUID,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tenant_projection_status_checkpoint_pair CHECK (
        (source_updated_at IS NULL) = (source_job_id IS NULL)
    )
);
