-- Revise unsent customer text without replacing its reviewed history.
ALTER TABLE finance_collection_cases ADD COLUMN root_id TEXT REFERENCES finance_collection_cases(id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE finance_collection_cases ADD COLUMN revision_number BIGINT NOT NULL DEFAULT 1 CHECK(revision_number>0);
ALTER TABLE finance_collection_cases ADD COLUMN parent_case_id TEXT UNIQUE REFERENCES finance_collection_cases(id);
ALTER TABLE finance_collection_cases ADD COLUMN parent_version BIGINT;
ALTER TABLE finance_collection_cases ADD COLUMN revision_reason TEXT;
UPDATE finance_collection_cases SET root_id=id;
ALTER TABLE finance_collection_cases ALTER COLUMN root_id SET NOT NULL;
ALTER TABLE finance_collection_cases DROP CONSTRAINT finance_collection_cases_status_check;
ALTER TABLE finance_collection_cases ADD CONSTRAINT finance_collection_cases_status_check CHECK(status IN ('draft','approved','test_delivered','cancelled','superseded'));
ALTER TABLE finance_collection_cases DROP CONSTRAINT finance_collection_cases_invoice_id_invoice_version_stage_key;
CREATE UNIQUE INDEX finance_collection_root_scope ON finance_collection_cases(invoice_id,invoice_version,stage) WHERE parent_case_id IS NULL;
CREATE UNIQUE INDEX finance_collection_live_scope ON finance_collection_cases(invoice_id,invoice_version,stage) WHERE status IN ('draft','approved','test_delivered');
ALTER TABLE finance_collection_cases ADD CONSTRAINT finance_collection_edition UNIQUE(root_id,revision_number);
ALTER TABLE finance_collection_cases ADD CONSTRAINT finance_collection_parent_shape CHECK(
 (parent_case_id IS NULL AND root_id=id AND revision_number=1 AND parent_version IS NULL AND revision_reason IS NULL)
 OR (parent_case_id IS NOT NULL AND root_id<>id AND revision_number>1 AND parent_version IS NOT NULL AND parent_version>0 AND revision_reason IS NOT NULL AND length(revision_reason)>=3));
CREATE OR REPLACE FUNCTION protect_collection_case() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.company_id,NEW.invoice_id,NEW.invoice_version,NEW.as_of_date,NEW.stage,NEW.amount_minor,NEW.currency,NEW.message,NEW.source_hash,NEW.prepared_by,NEW.root_id,NEW.revision_number,NEW.parent_case_id,NEW.parent_version,NEW.revision_reason)
 IS DISTINCT FROM (OLD.company_id,OLD.invoice_id,OLD.invoice_version,OLD.as_of_date,OLD.stage,OLD.amount_minor,OLD.currency,OLD.message,OLD.source_hash,OLD.prepared_by,OLD.root_id,OLD.revision_number,OLD.parent_case_id,OLD.parent_version,OLD.revision_reason)
 OR OLD.status IN ('test_delivered','superseded') THEN RAISE EXCEPTION 'COLLECTION_SOURCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE FUNCTION validate_collection_edition() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE parent finance_collection_cases; BEGIN
 IF NEW.parent_case_id IS NULL THEN RETURN NEW;END IF;
 SELECT * INTO parent FROM finance_collection_cases WHERE id=NEW.parent_case_id;
 IF (parent.company_id,parent.root_id,parent.invoice_id,parent.currency,parent.stage,parent.version,parent.status,parent.revision_number+1)
 IS DISTINCT FROM (NEW.company_id,NEW.root_id,NEW.invoice_id,NEW.currency,NEW.stage,NEW.parent_version,'superseded'::text,NEW.revision_number)
 OR NEW.as_of_date<parent.as_of_date OR EXISTS(SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=NEW.root_id)
 THEN RAISE EXCEPTION 'COLLECTION_EDITION_SOURCE_CHANGED' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_collection_edition_source BEFORE INSERT ON finance_collection_cases FOR EACH ROW EXECUTE FUNCTION validate_collection_edition();
CREATE FUNCTION validate_collection_delivery_edition() RETURNS trigger LANGUAGE plpgsql AS $$ DECLARE source finance_collection_cases; BEGIN
 SELECT * INTO source FROM finance_collection_cases WHERE id=NEW.case_id;
 IF source.company_id IS DISTINCT FROM NEW.company_id OR source.status IS DISTINCT FROM 'approved'
 OR EXISTS(SELECT 1 FROM finance_collection_cases WHERE parent_case_id=source.id)
 OR EXISTS(SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=source.root_id)
 THEN RAISE EXCEPTION 'COLLECTION_DELIVERY_EDITION_INVALID' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_collection_delivery_edition BEFORE INSERT ON finance_collection_deliveries FOR EACH ROW EXECUTE FUNCTION validate_collection_delivery_edition();
