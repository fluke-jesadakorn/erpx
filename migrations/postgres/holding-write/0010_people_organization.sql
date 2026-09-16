-- Legacy rank values retain their meaning in historical approval evidence.
-- Numbered levels are an explicitly assigned catalog, never inferred from roles.
ALTER TABLE staff_levels ADD COLUMN level_number INTEGER CHECK (level_number BETWEEN 1 AND 6);
CREATE UNIQUE INDEX staff_levels_number ON staff_levels(company_id,level_number) WHERE level_number IS NOT NULL;

CREATE TABLE organization_positions (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  title TEXT NOT NULL,
  level_id TEXT NOT NULL REFERENCES staff_levels(id),
  reports_to_position_id TEXT,
  is_ceo INTEGER NOT NULL DEFAULT 0 CHECK (is_ceo IN (0,1)),
  active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,id),
  CHECK (reports_to_position_id IS DISTINCT FROM id),
  FOREIGN KEY(company_id,reports_to_position_id) REFERENCES organization_positions(company_id,id) DEFERRABLE INITIALLY DEFERRED
);
CREATE UNIQUE INDEX organization_ceo_position ON organization_positions(company_id) WHERE is_ceo=1 AND active=1;
ALTER TABLE hr_engagements ADD COLUMN position_id TEXT;
ALTER TABLE hr_engagements ADD CONSTRAINT engagement_position_company FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id);

CREATE TABLE organization_templates (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  positions_json TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE TABLE staff_organization_history (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  before_json TEXT NOT NULL,
  after_json TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  command_id TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX staff_organization_history_engagement ON staff_organization_history(engagement_id,created_at);

INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at)
SELECT 'org-level-' || c.company_id || '-' || l.n,c.company_id,'ORG_L' || l.n,l.name,-l.n,l.n,1,
  floor(extract(epoch FROM clock_timestamp())*1000)::bigint,floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM (SELECT DISTINCT company_id FROM organization_nodes WHERE company_id IS NOT NULL) c
CROSS JOIN (VALUES (1,'CEO'),(2,'Executive / Director'),(3,'Branch / Department Manager'),(4,'Supervisor'),(5,'Senior Staff'),(6,'Staff')) AS l(n,name);
