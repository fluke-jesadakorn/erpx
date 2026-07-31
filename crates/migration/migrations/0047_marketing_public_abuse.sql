-- Durable anonymous Marketing abuse windows. The key is intentionally scoped
-- to the public tenant route and published resource; no IP, contact, token,
-- or request-body value is stored.
CREATE TABLE IF NOT EXISTS erpx_control.marketing_public_abuse_windows (
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE CASCADE,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    hostname TEXT NOT NULL CHECK (length(btrim(hostname)) BETWEEN 1 AND 253),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('form', 'preview')),
    resource_id UUID NOT NULL,
    window_started_at TIMESTAMPTZ NOT NULL,
    request_count BIGINT NOT NULL CHECK (request_count > 0),
    last_seen_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, holding_ref, hostname, resource_kind, resource_id, window_started_at)
);

CREATE INDEX IF NOT EXISTS marketing_public_abuse_cleanup_idx
    ON erpx_control.marketing_public_abuse_windows (last_seen_at);
