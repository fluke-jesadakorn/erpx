ALTER TABLE finance_assets ADD COLUMN depreciation_account_id TEXT REFERENCES finance_accounts(id);
