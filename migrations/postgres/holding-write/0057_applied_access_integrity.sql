CREATE FUNCTION people_application_snapshot_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN
 IF (to_jsonb(NEW)-'ended_at') IS DISTINCT FROM (to_jsonb(OLD)-'ended_at') OR OLD.ended_at IS NOT NULL AND NEW.ended_at IS DISTINCT FROM OLD.ended_at THEN
 RAISE EXCEPTION 'Applied access snapshots are immutable; create a new application';
 END IF;
 RETURN NEW;
 END; $$;
CREATE TRIGGER people_application_snapshot BEFORE UPDATE ON people_applied_packages FOR EACH ROW EXECUTE FUNCTION people_application_snapshot_immutable();
-- A pinned package membership survives HR movement. Direct/rule membership retains its own scope rules.
DO $$ DECLARE body TEXT; BEGIN
 body:=pg_get_functiondef('um_group_member(text,text,text)'::regprocedure);
 body:=replace(body,'AND um_person_in_unit(company_key,person_key,g.definition->>''scopeNodeId'')',
 'AND (um_person_in_unit(company_key,person_key,g.definition->>''scopeNodeId'') OR EXISTS(SELECT 1 FROM people_applied_packages pinned WHERE pinned.company_id=company_key AND pinned.person_id=person_key AND pinned.definition->''groupIds'' @> jsonb_build_array(g.id) AND people_application_active(pinned.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)))');
 EXECUTE body;
END $$;
