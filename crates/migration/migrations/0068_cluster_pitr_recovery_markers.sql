-- Durable, payload-free recovery markers used to prove that a physical
-- cluster restore stopped at the requested WAL/time boundary. These rows are
-- platform-scoped; no tenant may create or interpret a physical target.

CREATE TABLE IF NOT EXISTS erpx_control.platform_pitr_recovery_markers (
    id UUID PRIMARY KEY,
    rehearsal_ref TEXT NOT NULL CHECK (length(trim(rehearsal_ref)) BETWEEN 1 AND 120),
    marker_kind TEXT NOT NULL CHECK (marker_kind IN ('before', 'after')),
    committed_at TIMESTAMPTZ NOT NULL,
    UNIQUE (rehearsal_ref, marker_kind)
);
