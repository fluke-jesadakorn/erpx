CREATE TABLE work_role_preferences (
 user_id TEXT NOT NULL,
 company_id TEXT NOT NULL,
 assignment_id TEXT NOT NULL,
 assignment_version BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 PRIMARY KEY(user_id,company_id)
);
-- Retain the most recently selected role from existing sessions. Restoration
-- still checks the assignment's current validity and version before use.
INSERT INTO work_role_preferences(user_id,company_id,assignment_id,assignment_version,updated_at)
SELECT DISTINCT ON(user_id,company_id) user_id,company_id,assignment_id,assignment_version,updated_at
FROM active_work_contexts WHERE assignment_id IS NOT NULL AND assignment_version IS NOT NULL
ORDER BY user_id,company_id,updated_at DESC,session_id;
