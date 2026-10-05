-- Personal, read-only Executive MCP credentials. Plaintext secrets never persist.
CREATE TABLE executive_external_tokens (
  id TEXT PRIMARY KEY,
  secret_hash TEXT NOT NULL UNIQUE CHECK (secret_hash ~ '^[a-f0-9]{64}$'),
  name TEXT NOT NULL,
  user_id TEXT NOT NULL REFERENCES "user"(id),
  holding_id TEXT NOT NULL,
  company_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  assignment_version INTEGER NOT NULL,
  role TEXT NOT NULL,
  assignment_scope_kind TEXT NOT NULL CHECK (assignment_scope_kind IN ('holding','branch','department')),
  assignment_scope_node_id TEXT,
  branch_id TEXT,
  department_id TEXT,
  scope_label TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL CHECK (expires_at > created_at),
  revoked_at BIGINT,
  last_used_at BIGINT
);
CREATE INDEX executive_external_tokens_owner ON executive_external_tokens(user_id,holding_id,company_id,created_at DESC);
CREATE TABLE executive_mcp_audit (
  id TEXT PRIMARY KEY,
  token_id TEXT,
  user_id TEXT,
  company_id TEXT,
  assignment_id TEXT,
  branch_id TEXT,
  department_id TEXT,
  action TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  code TEXT NOT NULL,
  started_at BIGINT NOT NULL,
  duration_ms INTEGER NOT NULL
);
CREATE INDEX executive_mcp_audit_owner ON executive_mcp_audit(user_id,started_at DESC);
CREATE TABLE executive_mcp_rate_limits (
  user_id TEXT NOT NULL,
  company_id TEXT NOT NULL,
  window_start BIGINT NOT NULL,
  requests INTEGER NOT NULL,
  PRIMARY KEY (user_id,company_id)
);
DO $$
DECLARE app_role TEXT := left(current_user,length(current_user)-10) || '_app';
BEGIN
  IF right(current_user,10) = '_migration' AND EXISTS (SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT,INSERT,UPDATE ON executive_external_tokens,executive_mcp_rate_limits TO %I',app_role);
    EXECUTE format('GRANT INSERT ON executive_mcp_audit TO %I',app_role);
  END IF;
END;
$$;
