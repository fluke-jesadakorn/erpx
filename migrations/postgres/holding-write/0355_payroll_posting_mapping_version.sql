ALTER TABLE hr_payroll_postings ADD COLUMN account_mapping_id TEXT REFERENCES hr_payroll_account_mappings(id);
ALTER TABLE hr_payroll_postings ADD COLUMN account_mapping_version BIGINT;
