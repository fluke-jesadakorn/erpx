-- Domain event log backing /api/notifications polling endpoint
-- Populated automatically by src/lib/events.ts publish()

CREATE TABLE IF NOT EXISTS domain_events (
    id BIGSERIAL PRIMARY KEY,
    type VARCHAR(80) NOT NULL,
    actor_id INT REFERENCES users(id) ON DELETE SET NULL,
    ref_type VARCHAR(40),
    ref_id BIGINT,
    payload JSONB,
    severity VARCHAR(20) DEFAULT 'info',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_domain_events_created_at ON domain_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_domain_events_type ON domain_events(type);
CREATE INDEX IF NOT EXISTS idx_domain_events_actor ON domain_events(actor_id);
