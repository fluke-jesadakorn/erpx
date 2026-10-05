-- A stale proposal can be withdrawn without changing approved pay or its evidence.
ALTER TABLE hr_evaluation_compensation_drafts DROP CONSTRAINT hr_evaluation_compensation_drafts_status_check;
ALTER TABLE hr_evaluation_compensation_drafts ADD CONSTRAINT hr_evaluation_compensation_drafts_status_check CHECK(status IN ('draft','approved','cancelled'));
