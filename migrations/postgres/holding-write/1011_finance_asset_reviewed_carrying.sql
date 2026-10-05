-- Adopt retained asset history without restating or replacing its posted journals.
ALTER TABLE finance_asset_policy_editions ADD COLUMN basis_json TEXT CHECK(basis_json IS NULL OR erpx_json_valid(basis_json));
CREATE OR REPLACE FUNCTION protect_finance_asset_policy() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.asset_id,NEW.parent_id,NEW.content_json,NEW.source_hash,NEW.prepared_by,NEW.reason,NEW.created_at,NEW.basis_json)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.asset_id,OLD.parent_id,OLD.content_json,OLD.source_hash,OLD.prepared_by,OLD.reason,OLD.created_at,OLD.basis_json) THEN RAISE EXCEPTION 'ASSET_POLICY_EDITION_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
