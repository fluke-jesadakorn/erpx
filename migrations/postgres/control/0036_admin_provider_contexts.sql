-- Own Admin session selection only. Existing Company/Platform assignments,
-- account preferences, sessions and their authority remain unchanged.
CREATE TABLE admin_provider_contexts (
 session_id TEXT PRIMARY KEY REFERENCES session(id) ON DELETE CASCADE,
 user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
 assignment_id TEXT NOT NULL,
 revision BIGINT NOT NULL CHECK(revision > 0),
 updated_at BIGINT NOT NULL
);
CREATE INDEX admin_provider_contexts_account ON admin_provider_contexts(user_id,session_id);
