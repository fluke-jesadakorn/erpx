ALTER TABLE hr_engagements DROP CONSTRAINT hr_engagements_employment_type_check;
ALTER TABLE hr_engagements ADD CONSTRAINT hr_engagements_employment_type_check CHECK(employment_type IN ('salaried','daily','hourly','shift','contractor'));
ALTER TABLE hr_compensation_revisions ADD COLUMN daily_rate_minor BIGINT NOT NULL DEFAULT 0 CHECK(daily_rate_minor>=0);
ALTER TABLE hr_compensation_revisions ADD COLUMN standard_daily_hours DOUBLE PRECISION NOT NULL DEFAULT 8 CHECK(standard_daily_hours>0 AND standard_daily_hours<=24);
