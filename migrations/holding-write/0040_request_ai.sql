CREATE TABLE request_ai_proposals (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, request_id TEXT NOT NULL, source_version INTEGER NOT NULL,
 provider_json TEXT NOT NULL, actor_json TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('queued','running','ready','failed','stale')),
 result_json TEXT, error TEXT, attempts INTEGER NOT NULL DEFAULT 0, lease_token TEXT, claim_until INTEGER,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 FOREIGN KEY(company_id,request_id) REFERENCES employee_requests(company_id,id),
 UNIQUE(company_id,request_id,source_version,provider_json)
);
CREATE TABLE request_ai_usage (
 id TEXT PRIMARY KEY REFERENCES request_ai_proposals(id),company_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 month TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
 lease_until INTEGER NOT NULL,input_tokens INTEGER NOT NULL DEFAULT 0,output_tokens INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL
);
CREATE INDEX request_ai_queue ON request_ai_proposals(state,claim_until,created_at);
CREATE INDEX request_ai_quota ON request_ai_usage(company_id,month);
