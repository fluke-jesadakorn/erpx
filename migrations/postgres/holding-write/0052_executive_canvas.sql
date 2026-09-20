CREATE TABLE executive_reports (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, owner_id TEXT NOT NULL,
 definition_json TEXT NOT NULL, filters_json TEXT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE INDEX executive_reports_owner ON executive_reports(company_id,owner_id,updated_at DESC);
CREATE TABLE executive_report_revisions (
 report_id TEXT NOT NULL REFERENCES executive_reports(id), version BIGINT NOT NULL,
 definition_json TEXT NOT NULL, filters_json TEXT NOT NULL, author_id TEXT NOT NULL, created_at BIGINT NOT NULL,
 PRIMARY KEY(report_id,version)
);
CREATE TABLE executive_report_shares (
 id TEXT PRIMARY KEY, report_id TEXT NOT NULL REFERENCES executive_reports(id), company_id TEXT NOT NULL,
 recipient_id TEXT NOT NULL, recipient_name TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('viewer','editor')),
 mode TEXT NOT NULL CHECK(mode IN ('recipient','distribution')),
 widget_ids_json TEXT NOT NULL, definition_json TEXT NOT NULL, filters_json TEXT NOT NULL,
 publisher_json TEXT NOT NULL, expires_at BIGINT, revoked_at BIGINT, version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL, UNIQUE(report_id,recipient_id)
);
CREATE INDEX executive_report_recipient ON executive_report_shares(company_id,recipient_id,revoked_at,expires_at);
CREATE TABLE executive_generation_jobs (
 id TEXT PRIMARY KEY, report_id TEXT NOT NULL REFERENCES executive_reports(id), company_id TEXT NOT NULL,
 user_id TEXT NOT NULL, assignment_id TEXT NOT NULL, scope_key TEXT NOT NULL, question TEXT NOT NULL,
 input_json TEXT NOT NULL, actor_json TEXT NOT NULL, provider_json TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('queued','running','ready','failed','cancelled')),
 proposal_json TEXT, error TEXT, lease_token TEXT, lease_until BIGINT, attempts BIGINT NOT NULL DEFAULT 0,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE INDEX executive_jobs_queue ON executive_generation_jobs(state,created_at);
CREATE INDEX executive_jobs_history ON executive_generation_jobs(report_id,user_id,assignment_id,scope_key,created_at DESC);
CREATE INDEX executive_jobs_quota ON executive_generation_jobs(company_id,created_at);

-- Only the built-in owner is extended. Other roles use the existing reviewed IAM workflow.
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
SELECT r.company_id,r.id,r.published_revision+1,
 v.permissions_json || '["executive.access","executive.report.create","executive.report.edit","executive.report.share","executive.data.distribute"]'::jsonb,
 v.grantable_json || '["executive.access","executive.report.create","executive.report.edit","executive.report.share","executive.data.distribute"]'::jsonb,
 v.scopes_json,'system:executive-canvas',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
WHERE r.company_id='*' AND r.id='holding_owner';
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id)
WHERE r.company_id='*' AND r.id='holding_owner';
