-- Retirement keeps the existing offboarding state machine and cutoff authority.
ALTER TABLE people_lifecycle_cases ADD COLUMN departure_reason TEXT NOT NULL DEFAULT 'other' CHECK(departure_reason IN ('retirement','other'));
ALTER TABLE people_preparation_tasks ADD COLUMN clearance_type TEXT CHECK(clearance_type IN ('hr','handover','equipment','directory','payroll','finance'));
ALTER TABLE people_preparation_tasks ADD COLUMN completion_reason TEXT NOT NULL DEFAULT '';
ALTER TABLE people_preparation_tasks ADD COLUMN document_references JSONB NOT NULL DEFAULT '[]';
ALTER TABLE people_preparation_tasks DROP CONSTRAINT people_preparation_tasks_status_check;
ALTER TABLE people_preparation_tasks ADD CONSTRAINT people_preparation_tasks_status_check CHECK(status IN ('pending','completed','cancelled','not_required'));
CREATE INDEX people_retirement_cases ON people_lifecycle_cases(company_id,effective_at) WHERE departure_reason='retirement';
