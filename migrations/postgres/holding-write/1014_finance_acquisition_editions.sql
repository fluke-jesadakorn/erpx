-- Keep approved measurements and their owned calculations readable after correction.
ALTER TABLE finance_acquisitions ADD COLUMN parent_id TEXT REFERENCES finance_acquisitions(id);
ALTER TABLE finance_acquisitions ADD COLUMN parent_version INTEGER CHECK(parent_version>0);
ALTER TABLE finance_acquisitions ADD CONSTRAINT finance_acquisition_parent_pair CHECK((parent_id IS NULL)=(parent_version IS NULL));
ALTER TABLE finance_acquisitions DROP CONSTRAINT finance_acquisitions_status_check;
ALTER TABLE finance_acquisitions ADD CONSTRAINT finance_acquisitions_status_check CHECK(status IN ('draft','reviewed','approved','impaired','rejected','superseded'));
CREATE UNIQUE INDEX finance_acquisition_current_child ON finance_acquisitions(parent_id) WHERE parent_id IS NOT NULL AND status IN ('draft','approved','impaired');
CREATE FUNCTION protect_finance_acquisition_parent() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF (NEW.parent_id,NEW.parent_version) IS DISTINCT FROM (OLD.parent_id,OLD.parent_version) THEN
  RAISE EXCEPTION 'ACQUISITION_EDITION_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END;$$;
CREATE TRIGGER finance_acquisition_parent_immutable BEFORE UPDATE ON finance_acquisitions FOR EACH ROW EXECUTE FUNCTION protect_finance_acquisition_parent();
