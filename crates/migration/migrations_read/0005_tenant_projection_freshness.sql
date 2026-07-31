ALTER TABLE erpx_control.tenant_projection_status
    ADD COLUMN IF NOT EXISTS published_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS tenant_projection_status_published_idx
    ON erpx_control.tenant_projection_status (published_at DESC, job_id DESC);
