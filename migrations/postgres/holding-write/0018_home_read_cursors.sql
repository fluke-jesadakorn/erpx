CREATE TABLE home_read_cursors (
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  user_id TEXT NOT NULL,
  workspace_base TEXT NOT NULL,
  read_through BIGINT NOT NULL,
  PRIMARY KEY (company_id, user_id, workspace_base)
);
