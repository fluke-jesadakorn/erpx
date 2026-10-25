-- Service-only sales and their reviewed financial credits can belong to the whole Company.
ALTER TABLE commerce_returns ALTER COLUMN branch_id DROP NOT NULL;
