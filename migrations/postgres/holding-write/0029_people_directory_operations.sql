CREATE TABLE people_directory_state (
 connection_id TEXT PRIMARY KEY REFERENCES iam_connections(id), company_id TEXT NOT NULL,
 version INTEGER NOT NULL DEFAULT 1, tested_version INTEGER, write_enabled BOOLEAN NOT NULL DEFAULT false,
 definition JSONB NOT NULL DEFAULT '{}'::jsonb, inventory JSONB, observed_at BIGINT
);
CREATE TABLE people_directory_operations (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,connection_id TEXT NOT NULL REFERENCES iam_connections(id),
 connection_version INTEGER NOT NULL,state_version INTEGER NOT NULL,version INTEGER NOT NULL DEFAULT 1,
 status TEXT NOT NULL CHECK(status IN ('pending_review','queued','running','retry','completed','blocked','cancelled','rejected')),
 definition JSONB NOT NULL,evidence JSONB NOT NULL,review_id TEXT REFERENCES iam_reviews(id),
 created_by TEXT NOT NULL,assignment_id TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 attempts INTEGER NOT NULL DEFAULT 0,next_attempt_at BIGINT NOT NULL DEFAULT 0,last_error TEXT,result JSONB
);
CREATE INDEX people_directory_operations_due ON people_directory_operations(next_attempt_at,created_at) WHERE status IN ('queued','running','retry');
CREATE TABLE people_holding_templates (
 id TEXT PRIMARY KEY,holding_id TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 1,name TEXT NOT NULL,
 definition JSONB NOT NULL,created_by TEXT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE people_template_adoptions (
 company_id TEXT NOT NULL,template_id TEXT NOT NULL REFERENCES people_holding_templates(id),
 template_version INTEGER NOT NULL,version INTEGER NOT NULL DEFAULT 1,definition JSONB NOT NULL,
 PRIMARY KEY(company_id,template_id)
);
