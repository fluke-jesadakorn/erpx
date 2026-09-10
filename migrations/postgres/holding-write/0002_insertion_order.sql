-- Explicit insertion order replaces the three historical SQLite rowid queries.
ALTER TABLE approval_instances ADD COLUMN insertion_order BIGINT GENERATED ALWAYS AS IDENTITY;
ALTER TABLE approval_decisions ADD COLUMN insertion_order BIGINT GENERATED ALWAYS AS IDENTITY;
ALTER TABLE staff_level_history ADD COLUMN insertion_order BIGINT GENERATED ALWAYS AS IDENTITY;
