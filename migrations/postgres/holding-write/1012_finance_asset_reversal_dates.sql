ALTER TABLE finance_asset_policy_runs ADD COLUMN reversal_entry_date TEXT CHECK(reversal_entry_date IS NULL OR reversal_entry_date ~ '^\d{4}-\d{2}-\d{2}$');
ALTER TABLE finance_asset_policy_runs DISABLE TRIGGER finance_asset_policy_run_immutable;
UPDATE finance_asset_policy_runs r SET reversal_entry_date=(SELECT MIN(j.entry_date) FROM jsonb_array_elements_text(r.reversal_ids_json::jsonb) item JOIN finance_journals j ON j.id=item.value) WHERE r.status='reversed';
ALTER TABLE finance_asset_policy_runs ENABLE TRIGGER finance_asset_policy_run_immutable;
ALTER TABLE finance_asset_policy_runs ADD CONSTRAINT finance_asset_reversal_date_shape CHECK(status='reversed' OR reversal_entry_date IS NULL);
-- A zero-amount historical inverse has no journal date to invent. Its dated
-- command receipt remains the original evidence until separately reconciled.
