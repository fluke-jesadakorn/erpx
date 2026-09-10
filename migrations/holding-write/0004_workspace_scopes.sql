CREATE TABLE IF NOT EXISTS organization_nodes (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK(kind IN ('company','headquarters','branch','department','team')),
  slug TEXT NOT NULL,
  display_name TEXT NOT NULL,
  parent_id TEXT REFERENCES organization_nodes(id),
  company_id TEXT NOT NULL,
  branch_id TEXT,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','suspended')),
  hierarchy_revision INTEGER NOT NULL DEFAULT 1 CHECK(hierarchy_revision > 0),
  UNIQUE(parent_id, slug)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS organization_branch_slug_idx
  ON organization_nodes(slug) WHERE kind = 'branch';
CREATE UNIQUE INDEX IF NOT EXISTS organization_holding_department_slug_idx
  ON organization_nodes(slug) WHERE kind = 'department' AND branch_id IS NULL;

ALTER TABLE assignments ADD COLUMN scope_kind TEXT NOT NULL DEFAULT 'holding'
  CHECK(scope_kind IN ('holding','branch','department'));
ALTER TABLE assignments ADD COLUMN scope_node_id TEXT;

UPDATE assignments
SET scope_kind = CASE WHEN branch_id IS NULL THEN 'holding' ELSE 'branch' END,
    scope_node_id = branch_id,
    role = CASE WHEN role = 'owner' THEN 'holding_owner' ELSE role END;

ALTER TABLE employees ADD COLUMN department_id TEXT;
ALTER TABLE leave_requests ADD COLUMN department_id TEXT;
ALTER TABLE social_posts ADD COLUMN department_id TEXT;
ALTER TABLE social_stories ADD COLUMN department_id TEXT;
