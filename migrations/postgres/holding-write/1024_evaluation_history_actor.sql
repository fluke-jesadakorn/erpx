-- Workflow actors can differ from the original author or latest score editor.
ALTER TABLE hr_evaluation_assessments ADD COLUMN updated_by TEXT;
CREATE OR REPLACE FUNCTION hr_evaluation_history() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='hr_evaluation_definitions' THEN
  INSERT INTO hr_evaluation_definition_history(id,company_id,version,kind,payload_json,actor_id,reason,created_at)
  VALUES(NEW.id,NEW.company_id,NEW.version,NEW.kind,NEW.payload_json,NEW.updated_by,NEW.reason,NEW.updated_at);
 ELSE
  INSERT INTO hr_evaluation_assessment_history(id,company_id,version,status,content_json,actor_id,reason,created_at,period_start,period_end,role_id,role_version)
  VALUES(NEW.id,NEW.company_id,NEW.version,NEW.status,NEW.content_json,COALESCE(NEW.updated_by,NEW.confirmed_by,NEW.reviewed_by,NEW.created_by),NEW.reason,NEW.updated_at,NEW.period_start,NEW.period_end,NEW.role_id,NEW.role_version);
 END IF;
 RETURN NEW;
END $$;
