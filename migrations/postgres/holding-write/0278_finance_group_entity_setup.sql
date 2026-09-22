ALTER TABLE finance_group_entities ADD COLUMN version BIGINT NOT NULL DEFAULT 1;
ALTER TABLE finance_group_entities ADD COLUMN created_by TEXT NOT NULL DEFAULT 'migration';
ALTER TABLE finance_group_entities ADD COLUMN updated_at BIGINT NOT NULL DEFAULT 0;
