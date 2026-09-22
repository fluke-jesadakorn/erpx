-- Retain specification headers as well as their immutable active metric rows.
DROP TRIGGER quality_spec_guard ON inventory_quality_specs;
CREATE OR REPLACE FUNCTION quality_spec_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'QUALITY_SPEC_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status IN ('active','superseded','cancelled') AND NEW IS DISTINCT FROM OLD AND NOT (OLD.status='active' AND NEW.status='superseded' AND NEW.version=OLD.version+1) THEN
  RAISE EXCEPTION 'QUALITY_SPEC_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER quality_spec_guard BEFORE UPDATE OR DELETE ON inventory_quality_specs FOR EACH ROW EXECUTE FUNCTION quality_spec_guard();
