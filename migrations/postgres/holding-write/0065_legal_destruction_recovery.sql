ALTER TABLE legal_destructions ADD COLUMN started_at bigint;
ALTER TABLE legal_destructions ADD COLUMN attempts integer NOT NULL DEFAULT 0;
