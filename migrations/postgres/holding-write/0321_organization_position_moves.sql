-- One reviewed request owns an entire occupied position. No external writes occur here.
CREATE TABLE organization_position_moves (
  id text PRIMARY KEY,
  company_id text NOT NULL,
  position_id text NOT NULL,
  status text NOT NULL CHECK (status IN ('scheduled','applied','needs_review','cancelled')),
  version integer NOT NULL DEFAULT 1,
  effective_at bigint NOT NULL,
  definition text NOT NULL,
  preview text NOT NULL,
  snapshot_hash text NOT NULL,
  authorizing_assignment_id text NOT NULL,
  actor_json text NOT NULL,
  created_by text NOT NULL,
  created_at bigint NOT NULL,
  updated_at bigint NOT NULL,
  applied_at bigint,
  last_error text,
  FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id)
);
CREATE UNIQUE INDEX organization_position_move_pending ON organization_position_moves(company_id,position_id) WHERE status='scheduled';
CREATE INDEX organization_position_moves_due ON organization_position_moves(effective_at,id) WHERE status='scheduled';

-- Reserve every reviewed holder, including suspended and future engagements.
CREATE TABLE organization_position_move_members (
  move_id text NOT NULL REFERENCES organization_position_moves(id),
  engagement_id text NOT NULL REFERENCES hr_engagements(id),
  person_id text NOT NULL,
  PRIMARY KEY(move_id,engagement_id)
);

-- Concurrent placement submission cannot silently invalidate a scheduled whole-position move.
CREATE FUNCTION prevent_position_move_overlap() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status IN ('waiting_it','pending_review','scheduled') AND EXISTS (
    SELECT 1 FROM organization_position_moves m
    WHERE m.company_id=NEW.company_id AND m.status='scheduled' AND (
      m.position_id=NEW.definition->>'positionId'
      OR EXISTS(SELECT 1 FROM organization_position_move_members h WHERE h.move_id=m.id AND (h.engagement_id=NEW.engagement_id OR h.person_id=NEW.person_id))
    )
  ) THEN
    RAISE EXCEPTION 'POSITION_MOVE_OVERLAP: cancel or finish the scheduled position move first' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER people_lifecycle_position_move_overlap BEFORE INSERT OR UPDATE OF status,definition,engagement_id ON people_lifecycle_cases
FOR EACH ROW EXECUTE FUNCTION prevent_position_move_overlap();

DO $$ DECLARE migration_role text:=current_user; app_role text; jobs_role text; BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role:=regexp_replace(migration_role,'_migration$','_app'); jobs_role:=regexp_replace(migration_role,'_migration$','_jobs');
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON organization_position_moves,organization_position_move_members TO %I,%I',app_role,jobs_role);
END $$;
