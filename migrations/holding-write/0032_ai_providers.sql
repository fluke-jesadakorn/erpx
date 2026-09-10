CREATE TABLE ai_connections (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL,
 version INTEGER NOT NULL DEFAULT 1, draft_revision INTEGER NOT NULL DEFAULT 1,
 active_revision INTEGER, disabled INTEGER NOT NULL DEFAULT 0 CHECK(disabled IN (0,1)),
 deleted_at INTEGER, created_by TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX ai_connection_company ON ai_connections(company_id,deleted_at);
CREATE TABLE ai_connection_revisions (
 connection_id TEXT NOT NULL REFERENCES ai_connections(id), revision INTEGER NOT NULL,
 base_url TEXT NOT NULL, auth_mode TEXT NOT NULL CHECK(auth_mode IN ('bearer','none')),
 credential_json TEXT CHECK(credential_json IS NULL OR json_valid(credential_json)),
 text_model TEXT NOT NULL, image_model TEXT,
 tested_at INTEGER, text_verified INTEGER NOT NULL DEFAULT 0, image_verified INTEGER NOT NULL DEFAULT 0,
 test_error TEXT, created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(connection_id,revision)
) STRICT;
CREATE TABLE ai_company_settings (
 company_id TEXT PRIMARY KEY, version INTEGER NOT NULL DEFAULT 1,
 mode TEXT NOT NULL DEFAULT 'deployment' CHECK(mode IN ('deployment','custom','disabled')),
  text_connection_id TEXT REFERENCES ai_connections(id), image_connection_id TEXT REFERENCES ai_connections(id),
  image_mode TEXT NOT NULL DEFAULT 'default' CHECK(image_mode IN ('default','custom','disabled')),
 updated_by TEXT NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE ai_provider_usage (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, connection_id TEXT, revision INTEGER,
 model TEXT NOT NULL, feature TEXT NOT NULL, input_tokens INTEGER NOT NULL DEFAULT 0,
 output_tokens INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL, created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE expense_ai_proposals (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, expense_id TEXT NOT NULL REFERENCES finance_expenses(id),
 payment_draft_id TEXT REFERENCES expense_payment_drafts(id), stage TEXT NOT NULL,
 source_version INTEGER NOT NULL, evidence_digest TEXT NOT NULL, provider_json TEXT NOT NULL CHECK(json_valid(provider_json)),
 state TEXT NOT NULL CHECK(state IN ('queued','running','ready','failed','stale','applied')),
 result_json TEXT CHECK(result_json IS NULL OR json_valid(result_json)), error TEXT,
 requested_by TEXT NOT NULL, claim_until INTEGER, attempts INTEGER NOT NULL DEFAULT 0,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX expense_ai_pending ON expense_ai_proposals(state,claim_until);
CREATE INDEX expense_ai_claim ON expense_ai_proposals(expense_id,created_at);
CREATE TABLE attachment_derivatives (
 id TEXT PRIMARY KEY, attachment_id TEXT NOT NULL REFERENCES attachments(id), company_id TEXT NOT NULL,
 page_number INTEGER NOT NULL CHECK(page_number>0), object_key TEXT NOT NULL UNIQUE,
 checksum_sha256 TEXT NOT NULL, original_checksum TEXT NOT NULL, bytes INTEGER NOT NULL CHECK(bytes>0),
 created_by TEXT NOT NULL, created_at INTEGER NOT NULL,
 UNIQUE(attachment_id,page_number,original_checksum)
) STRICT;
