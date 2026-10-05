-- Keep the proposal reason independently of later approval/withdrawal reasons.
ALTER TABLE hr_evaluation_compensation_drafts ADD COLUMN review_reason TEXT;
