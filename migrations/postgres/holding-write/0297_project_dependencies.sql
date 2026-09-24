CREATE TABLE staff_project_milestone_dependencies (
  company_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  predecessor_milestone_id TEXT NOT NULL REFERENCES staff_project_milestones(id) ON DELETE CASCADE,
  successor_milestone_id TEXT NOT NULL REFERENCES staff_project_milestones(id) ON DELETE CASCADE,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (project_id, successor_milestone_id, predecessor_milestone_id),
  CHECK (predecessor_milestone_id <> successor_milestone_id)
);
CREATE INDEX staff_project_milestone_dependencies_predecessor_idx
  ON staff_project_milestone_dependencies(company_id, project_id, predecessor_milestone_id, successor_milestone_id);

CREATE FUNCTION staff_project_milestone_dependency_validate() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  predecessor_company TEXT;
  predecessor_project TEXT;
  successor_company TEXT;
  successor_project TEXT;
  would_cycle BOOLEAN;
BEGIN
  SELECT company_id, project_id INTO predecessor_company, predecessor_project
    FROM staff_project_milestones WHERE id=NEW.predecessor_milestone_id;
  SELECT company_id, project_id INTO successor_company, successor_project
    FROM staff_project_milestones WHERE id=NEW.successor_milestone_id;
  IF predecessor_company IS NULL OR successor_company IS NULL
      OR predecessor_company<>NEW.company_id OR successor_company<>NEW.company_id
      OR predecessor_project<>NEW.project_id OR successor_project<>NEW.project_id THEN
    RAISE EXCEPTION 'PROJECT_DEPENDENCY_SCOPE_INVALID' USING ERRCODE='23514';
  END IF;
  WITH RECURSIVE reachable(milestone_id, path) AS (
    SELECT NEW.successor_milestone_id, ARRAY[NEW.successor_milestone_id]::text[]
    UNION ALL
    SELECT d.successor_milestone_id, reachable.path || d.successor_milestone_id
      FROM reachable
      JOIN staff_project_milestone_dependencies d
        ON d.predecessor_milestone_id=reachable.milestone_id
       AND d.company_id=NEW.company_id AND d.project_id=NEW.project_id
     WHERE NOT d.successor_milestone_id=ANY(reachable.path)
  )
  SELECT EXISTS(SELECT 1 FROM reachable WHERE milestone_id=NEW.predecessor_milestone_id)
    INTO would_cycle;
  IF would_cycle THEN
    RAISE EXCEPTION 'PROJECT_DEPENDENCY_CYCLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER staff_project_milestone_dependencies_validate
  BEFORE INSERT OR UPDATE ON staff_project_milestone_dependencies
  FOR EACH ROW EXECUTE FUNCTION staff_project_milestone_dependency_validate();

SELECT ensure_work_cache_coverage();
