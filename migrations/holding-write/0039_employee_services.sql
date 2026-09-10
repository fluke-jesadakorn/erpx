CREATE TABLE service_teams (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL,
 active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)), version INTEGER NOT NULL DEFAULT 1, configuration_json TEXT NOT NULL DEFAULT '{}',
 created_by TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 UNIQUE(company_id,id)
);
CREATE TABLE service_team_members (
 company_id TEXT NOT NULL, team_id TEXT NOT NULL, user_id TEXT NOT NULL,
 responsibility TEXT NOT NULL CHECK(responsibility IN ('lead','member')),
 PRIMARY KEY(team_id,user_id), FOREIGN KEY(company_id,team_id) REFERENCES service_teams(company_id,id)
);
CREATE TABLE service_team_coverage (
 company_id TEXT NOT NULL, team_id TEXT NOT NULL, category TEXT NOT NULL,
 scope_kind TEXT NOT NULL CHECK(scope_kind IN ('holding','branch','department')), scope_node_id TEXT NOT NULL,
 PRIMARY KEY(company_id,category,scope_kind,scope_node_id),
 FOREIGN KEY(company_id,team_id) REFERENCES service_teams(company_id,id)
);
CREATE TABLE employee_requests (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, department_id TEXT,
 kind TEXT NOT NULL CHECK(kind IN ('service','equipment','purchase')),
 category TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
 location TEXT NOT NULL DEFAULT '', impact TEXT NOT NULL DEFAULT 'normal', needed_date TEXT,
 items_json TEXT NOT NULL DEFAULT '[]', currency TEXT NOT NULL DEFAULT '',
 requester_user_id TEXT NOT NULL, created_by TEXT NOT NULL, parent_request_id TEXT,
 state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','returned','blocked','awaiting_approval','queued','in_progress','waiting_requester','resolved','fulfilled','closed','rejected','cancelled')),
 team_id TEXT, assignee_user_id TEXT, routing_json TEXT NOT NULL DEFAULT '{}',
 approval_json TEXT NOT NULL DEFAULT '{}', blocked_reason TEXT,
 resolution TEXT NOT NULL DEFAULT '', purchase_order_id TEXT, handover_json TEXT NOT NULL DEFAULT '{}',
 version INTEGER NOT NULL DEFAULT 1, evidence_version INTEGER NOT NULL DEFAULT 1,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 UNIQUE(company_id,id), FOREIGN KEY(company_id,team_id) REFERENCES service_teams(company_id,id),
 FOREIGN KEY(company_id,parent_request_id) REFERENCES employee_requests(company_id,id)
);
CREATE INDEX employee_requests_company_queue ON employee_requests(company_id,state,updated_at);
CREATE INDEX employee_requests_requester ON employee_requests(company_id,requester_user_id,updated_at);
CREATE INDEX employee_requests_team ON employee_requests(company_id,team_id,state);
CREATE UNIQUE INDEX employee_requests_order ON employee_requests(company_id,purchase_order_id) WHERE purchase_order_id IS NOT NULL;
CREATE TABLE employee_request_events (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, request_id TEXT NOT NULL, request_version INTEGER NOT NULL,
 action TEXT NOT NULL, actor_id TEXT NOT NULL, note TEXT NOT NULL DEFAULT '', evidence_json TEXT NOT NULL DEFAULT '{}',
 created_at INTEGER NOT NULL, FOREIGN KEY(company_id,request_id) REFERENCES employee_requests(company_id,id)
);
CREATE INDEX employee_request_history ON employee_request_events(request_id,created_at);
CREATE TABLE handbook_editors (
 company_id TEXT NOT NULL, user_id TEXT NOT NULL, granted_by TEXT NOT NULL, granted_at INTEGER NOT NULL,
 PRIMARY KEY(company_id,user_id)
);
CREATE TABLE handbook_settings (company_id TEXT PRIMARY KEY, version INTEGER NOT NULL);
CREATE TABLE handbook_articles (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
 draft_revision INTEGER NOT NULL DEFAULT 1, published_revision INTEGER,
 archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)), created_by TEXT NOT NULL,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(company_id,id)
);
CREATE TABLE handbook_revisions (
 company_id TEXT NOT NULL, article_id TEXT NOT NULL, revision INTEGER NOT NULL,
 title TEXT NOT NULL, category TEXT NOT NULL, body TEXT NOT NULL,
 created_by TEXT NOT NULL, created_at INTEGER NOT NULL, published_by TEXT, published_at INTEGER,
 PRIMARY KEY(article_id,revision), FOREIGN KEY(company_id,article_id) REFERENCES handbook_articles(company_id,id)
);
CREATE INDEX handbook_published ON handbook_articles(company_id,archived,published_revision);
