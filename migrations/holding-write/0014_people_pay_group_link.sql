ALTER TABLE hr_engagements ADD COLUMN pay_group_id TEXT REFERENCES hr_pay_groups(id);
CREATE INDEX IF NOT EXISTS hr_engagements_pay_group_idx ON hr_engagements(pay_group_id,status);
