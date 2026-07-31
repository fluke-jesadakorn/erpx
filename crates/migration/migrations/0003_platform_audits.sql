CREATE TABLE IF NOT EXISTS erpx_control.global_read_audits (
    id UUID PRIMARY KEY,
    actor_ref TEXT NOT NULL,
    session_ref TEXT,
    customer_id UUID NOT NULL,
    module TEXT NOT NULL,
    scope JSONB NOT NULL,
    purpose TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    payload JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS global_read_audits_customer_idx
    ON erpx_control.global_read_audits (customer_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS global_read_audits_actor_idx
    ON erpx_control.global_read_audits (actor_ref, occurred_at DESC);
