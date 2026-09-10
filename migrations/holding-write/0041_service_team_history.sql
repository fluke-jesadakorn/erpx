CREATE TABLE service_team_revisions (
  company_id TEXT NOT NULL,
  team_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  configuration_json TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(team_id,version),
  FOREIGN KEY(company_id,team_id) REFERENCES service_teams(company_id,id)
);
-- Preserve the current configuration of teams created before this expansion.
INSERT INTO service_team_revisions(company_id,team_id,version,configuration_json,actor_id,created_at)
SELECT company_id,id,version,configuration_json,created_by,updated_at FROM service_teams;
