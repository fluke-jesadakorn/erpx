-- Retain newly issued CSV bytes independently of future renderer changes.
-- Older snapshots retain their original renderer path; no issued data is rewritten.
ALTER TABLE finance_report_snapshots ADD COLUMN export_content TEXT;
ALTER TABLE finance_report_snapshots ADD COLUMN export_sha256 TEXT;
ALTER TABLE finance_report_snapshots ADD CONSTRAINT finance_report_export_pair CHECK((export_content IS NULL)=(export_sha256 IS NULL));
