CREATE TABLE active_work_contexts (
 session_id TEXT NOT NULL, user_id TEXT NOT NULL, company_id TEXT NOT NULL,
 assignment_id TEXT, assignment_version BIGINT, revision BIGINT NOT NULL DEFAULT 0,
 expires_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 PRIMARY KEY(session_id,company_id)
);
CREATE INDEX active_work_context_expiry ON active_work_contexts(expires_at);
ALTER TABLE audit_events ADD COLUMN authority_json TEXT;
ALTER TABLE command_receipts ADD COLUMN authority_json TEXT;
