-- Durable worker liveness evidence used by the server readiness boundary.
-- A missing row is not healthy; the server applies a bounded freshness window.
CREATE TABLE IF NOT EXISTS erpx_control.worker_heartbeats (
    worker_id UUID PRIMARY KEY,
    last_seen_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ready', 'unavailable'))
);
