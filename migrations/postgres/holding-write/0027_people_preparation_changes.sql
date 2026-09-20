-- Changed work requirements reopen the same IT task; a title-only change does not.
CREATE OR REPLACE FUNCTION people_position_preparation_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE needs_review BOOLEAN;
BEGIN
 needs_review := TG_OP='INSERT';
 IF TG_OP='UPDATE' THEN
  needs_review := (NEW.requirements,NEW.branch_id,NEW.department_id,NEW.level_id,NEW.active)
    IS DISTINCT FROM (OLD.requirements,OLD.branch_id,OLD.department_id,OLD.level_id,OLD.active);
 END IF;
 INSERT INTO people_preparation_tasks(id,company_id,position_id,kind,title,assignee_id,updated_at,status)
 VALUES('position-prep-'||NEW.id,NEW.company_id,NEW.id,'position','Prepare access · '||NEW.title,
   (SELECT definition->>'itAssigneeId' FROM people_lifecycle_settings WHERE company_id=NEW.company_id),
   floor(extract(epoch FROM clock_timestamp())*1000)::bigint,CASE WHEN NEW.active=1 THEN 'pending' ELSE 'cancelled' END)
 ON CONFLICT(id) DO UPDATE SET title=excluded.title,updated_at=excluded.updated_at,version=people_preparation_tasks.version+1,
   status=CASE WHEN NEW.active=0 THEN 'cancelled' WHEN needs_review THEN 'pending' ELSE people_preparation_tasks.status END;
 RETURN NULL;
END; $$;
DROP TRIGGER people_position_preparation ON organization_positions;
CREATE TRIGGER people_position_preparation AFTER INSERT OR UPDATE OF title,requirements,branch_id,department_id,level_id,active ON organization_positions
 FOR EACH ROW EXECUTE FUNCTION people_position_preparation_changed();
