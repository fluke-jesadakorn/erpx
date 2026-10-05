ALTER TABLE finance_ownership_interests ADD COLUMN company_id TEXT;
ALTER TABLE finance_ownership_interests ADD COLUMN parent_id TEXT REFERENCES finance_ownership_interests(id);
ALTER TABLE finance_ownership_interests ADD COLUMN source_hash TEXT;
ALTER TABLE finance_ownership_interests ADD COLUMN source_json TEXT CHECK(source_json IS NULL OR erpx_json_valid(source_json));
ALTER TABLE finance_ownership_interests ADD COLUMN reason TEXT;
ALTER TABLE finance_ownership_interests DROP CONSTRAINT finance_ownership_interests_status_check;
ALTER TABLE finance_ownership_interests ADD CONSTRAINT finance_ownership_interests_status_check CHECK(status IN ('draft','approved','retired','rejected'));
CREATE UNIQUE INDEX finance_ownership_revision ON finance_ownership_interests(parent_id) WHERE parent_id IS NOT NULL AND status NOT IN ('retired','rejected');
ALTER TABLE finance_acquisitions DROP CONSTRAINT finance_acquisitions_status_check;
ALTER TABLE finance_acquisitions ADD CONSTRAINT finance_acquisitions_status_check CHECK(status IN ('draft','reviewed','approved','impaired','rejected'));
ALTER TABLE finance_acquisitions ADD COLUMN company_id TEXT;
ALTER TABLE finance_acquisitions ADD COLUMN ownership_id TEXT REFERENCES finance_ownership_interests(id);
ALTER TABLE finance_acquisitions ADD COLUMN source_hash TEXT;
ALTER TABLE finance_acquisitions ADD COLUMN source_json TEXT CHECK(source_json IS NULL OR erpx_json_valid(source_json));
ALTER TABLE finance_acquisitions ADD COLUMN reason TEXT;
CREATE FUNCTION protect_finance_owned_ownership() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF OLD.source_hash IS NOT NULL AND (TG_OP='DELETE' OR
 (NEW.id,NEW.holding_id,NEW.company_id,NEW.parent_id,NEW.investor_entity_id,NEW.investee_entity_id,NEW.accounting_method,NEW.ownership_bps,NEW.voting_bps,NEW.control_assessment,NEW.effective_from,NEW.created_by,NEW.source_hash,NEW.source_json,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.holding_id,OLD.company_id,OLD.parent_id,OLD.investor_entity_id,OLD.investee_entity_id,OLD.accounting_method,OLD.ownership_bps,OLD.voting_bps,OLD.control_assessment,OLD.effective_from,OLD.created_by,OLD.source_hash,OLD.source_json,OLD.reason,OLD.created_at)) THEN RAISE EXCEPTION 'OWNERSHIP_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END; END;$$;
CREATE TRIGGER finance_owned_ownership_immutable BEFORE UPDATE OR DELETE ON finance_ownership_interests FOR EACH ROW EXECUTE FUNCTION protect_finance_owned_ownership();
CREATE FUNCTION protect_finance_owned_acquisition() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF OLD.source_hash IS NOT NULL AND (TG_OP='DELETE' OR
 (NEW.id,NEW.holding_id,NEW.company_id,NEW.ownership_id,NEW.investee_entity_id,NEW.acquisition_date,NEW.currency,NEW.consideration_minor,NEW.previous_interest_minor,NEW.nci_measurement,NEW.nci_minor,NEW.identifiable_net_assets_minor,NEW.deferred_tax_minor,NEW.goodwill_minor,NEW.bargain_purchase_minor,NEW.cgu_reference,NEW.calculation_run_id,NEW.created_by,NEW.source_hash,NEW.source_json,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.holding_id,OLD.company_id,OLD.ownership_id,OLD.investee_entity_id,OLD.acquisition_date,OLD.currency,OLD.consideration_minor,OLD.previous_interest_minor,OLD.nci_measurement,OLD.nci_minor,OLD.identifiable_net_assets_minor,OLD.deferred_tax_minor,OLD.goodwill_minor,OLD.bargain_purchase_minor,OLD.cgu_reference,OLD.calculation_run_id,OLD.created_by,OLD.source_hash,OLD.source_json,OLD.reason,OLD.created_at)) THEN RAISE EXCEPTION 'ACQUISITION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END; END;$$;
CREATE TRIGGER finance_owned_acquisition_immutable BEFORE UPDATE OR DELETE ON finance_acquisitions FOR EACH ROW EXECUTE FUNCTION protect_finance_owned_acquisition();
