-- Durable, credential-free health observations for the Platform Integrations
-- operator surface.  The table stores categorical state only; endpoints,
-- database keys, provider payloads, and credentials are deliberately excluded.

CREATE TABLE IF NOT EXISTS erpx_control.platform_integration_observations (
    integration_key TEXT PRIMARY KEY,
    state TEXT NOT NULL CHECK (state IN (
        'ready', 'not_ready', 'configured', 'not_configured',
        'healthy', 'degraded', 'unavailable', 'not_probed'
    )),
    capability TEXT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    observed_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (integration_key ~ '^[a-z][a-z0-9_]{1,63}$'),
    CHECK (length(btrim(capability)) BETWEEN 1 AND 256),
    CHECK (length(btrim(observed_by)) BETWEEN 1 AND 255)
);

CREATE INDEX IF NOT EXISTS platform_integration_observations_checked_idx
    ON erpx_control.platform_integration_observations (checked_at DESC, integration_key);

INSERT INTO erpx_control.platform_integration_observations
    (integration_key, state, capability, checked_at, observed_by)
VALUES
    ('control_plane', 'not_probed', 'SQLx control-plane write/read pools', now(), 'migration'),
    ('identity', 'not_probed', 'SQLx identity, session, and recovery boundary', now(), 'migration'),
    ('tenant_routing', 'not_probed', 'Physical tenant write/read database routing', now(), 'migration'),
    ('object_storage', 'not_probed', 'MinIO-compatible tenant object storage', now(), 'migration'),
    ('billing', 'not_probed', 'Stripe catalog, webhook, and entitlement boundary', now(), 'migration')
ON CONFLICT (integration_key) DO NOTHING;
