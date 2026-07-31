CREATE TABLE IF NOT EXISTS erpx_control.customer_access_audit_summaries (
    customer_id UUID PRIMARY KEY REFERENCES erpx_control.customers(id) ON DELETE CASCADE,
    read_count BIGINT NOT NULL DEFAULT 0 CHECK (read_count >= 0),
    last_access_at TIMESTAMPTZ,
    customer_visible BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_control.customer_notes (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL REFERENCES erpx_control.customers(id) ON DELETE CASCADE,
    author_ref TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    body TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS customer_notes_customer_idx
    ON erpx_control.customer_notes (customer_id, created_at DESC, id);
