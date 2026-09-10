PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS assignments (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  role TEXT NOT NULL,
  modules_json TEXT NOT NULL CHECK(json_valid(modules_json)),
  company_id TEXT NOT NULL,
  branch_id TEXT,
  valid_from INTEGER NOT NULL,
  valid_until INTEGER NOT NULL,
  revoked_at INTEGER,
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  CHECK(valid_until > valid_from)
) STRICT;
CREATE INDEX IF NOT EXISTS assignment_active_idx ON assignments(user_id, valid_from, valid_until);

CREATE TABLE IF NOT EXISTS employees (
  id TEXT PRIMARY KEY,
  user_id TEXT UNIQUE,
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS leave_requests (
  id TEXT PRIMARY KEY,
  number TEXT NOT NULL UNIQUE,
  employee_id TEXT NOT NULL REFERENCES employees(id),
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  start_date TEXT NOT NULL,
  days INTEGER NOT NULL CHECK(days BETWEEN 1 AND 365),
  handover_note TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','pending_approval','approved','rejected','cancelled')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  prepared_by TEXT,
  decided_by TEXT,
  decision_note TEXT,
  last_command_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS leave_queue_idx ON leave_requests(state, company_id, branch_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS command_receipts (
  command_id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  record_id TEXT NOT NULL,
  result_json TEXT,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS command_assertions (
  command_id TEXT NOT NULL,
  step TEXT NOT NULL,
  ok INTEGER NOT NULL CHECK(ok = 1),
  PRIMARY KEY(command_id, step)
) STRICT;

CREATE TABLE IF NOT EXISTS decision_evidence (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
  record_id TEXT NOT NULL,
  action TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  reviewed_version INTEGER NOT NULL,
  input_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  action TEXT NOT NULL,
  record_type TEXT NOT NULL,
  record_id TEXT NOT NULL,
  record_version INTEGER NOT NULL,
  purpose TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS audit_record_idx ON audit_events(record_type, record_id, created_at);

CREATE TABLE IF NOT EXISTS outbox_events (
  id TEXT PRIMARY KEY,
  command_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  aggregate_type TEXT NOT NULL,
  aggregate_id TEXT NOT NULL,
  aggregate_version INTEGER NOT NULL,
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  created_at INTEGER NOT NULL,
  published_at INTEGER,
  claim_until INTEGER,
  attempts INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX IF NOT EXISTS outbox_pending_idx ON outbox_events(published_at, claim_until, created_at);

CREATE TABLE IF NOT EXISTS upload_intents (
  id TEXT PRIMARY KEY,
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  purpose TEXT NOT NULL,
  original_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  declared_bytes INTEGER NOT NULL CHECK(declared_bytes BETWEEN 1 AND 10485760),
  actual_bytes INTEGER,
  checksum_sha256 TEXT,
  staging_key TEXT NOT NULL UNIQUE,
  final_key TEXT UNIQUE,
  revision INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL CHECK(status IN ('pending','uploaded','ready','failed','expired')),
  created_by TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS upload_owner_idx ON upload_intents(owner_type, owner_id, status);

CREATE TABLE IF NOT EXISTS attachments (
  id TEXT PRIMARY KEY,
  upload_id TEXT NOT NULL UNIQUE REFERENCES upload_intents(id),
  owner_type TEXT NOT NULL,
  owner_id TEXT NOT NULL,
  purpose TEXT NOT NULL,
  object_key TEXT NOT NULL UNIQUE,
  revision INTEGER NOT NULL,
  original_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  checksum_sha256 TEXT NOT NULL,
  created_by TEXT NOT NULL,
  legal_hold INTEGER NOT NULL DEFAULT 0 CHECK(legal_hold IN (0,1)),
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS attachment_owner_idx ON attachments(owner_type, owner_id, created_at);
