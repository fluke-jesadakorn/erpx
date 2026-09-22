-- Customer contact and address identity fields use the same reviewed change workflow.
ALTER TABLE finance_parties ADD COLUMN phone TEXT;
ALTER TABLE finance_parties ADD COLUMN address_json JSONB NOT NULL DEFAULT '{}'::jsonb;
