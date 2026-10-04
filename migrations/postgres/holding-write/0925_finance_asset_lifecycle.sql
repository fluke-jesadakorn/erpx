-- Additive owner evidence. Existing asset rows and journal history retain their meaning.
ALTER TABLE finance_assets ADD COLUMN depreciation_expense_account_id TEXT REFERENCES finance_accounts(id);
ALTER TABLE finance_assets ADD COLUMN activation_journal_id TEXT REFERENCES finance_journals(id);
ALTER TABLE finance_assets ADD COLUMN activated_by TEXT;
ALTER TABLE finance_assets ADD COLUMN created_by TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE finance_assets ADD COLUMN created_at BIGINT NOT NULL DEFAULT 0;
ALTER TABLE finance_assets ADD COLUMN updated_at BIGINT NOT NULL DEFAULT 0;
ALTER TABLE finance_assets ADD COLUMN depreciation_convention TEXT NOT NULL DEFAULT 'full_month' CHECK(depreciation_convention='full_month');
ALTER TABLE finance_assets ADD CONSTRAINT finance_asset_residual_limit CHECK(residual_minor<=cost_minor) NOT VALID;
ALTER TABLE finance_assets ADD CONSTRAINT finance_asset_depreciation_limit CHECK(accumulated_depreciation_minor>=0 AND accumulated_depreciation_minor<=cost_minor-residual_minor) NOT VALID;

CREATE TABLE finance_asset_events (
 id TEXT PRIMARY KEY,asset_id TEXT NOT NULL REFERENCES finance_assets(id),company_id TEXT NOT NULL,
 event_type TEXT NOT NULL CHECK(event_type IN ('created','edited','cancelled','activated','depreciated','disposal_prepared','disposal_cancelled','disposed')),
 asset_version BIGINT NOT NULL,command_id TEXT NOT NULL UNIQUE,payload_json TEXT NOT NULL,actor_id TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE TABLE finance_asset_depreciations (
 id TEXT PRIMARY KEY,asset_id TEXT NOT NULL REFERENCES finance_assets(id),company_id TEXT NOT NULL,
 depreciation_month TEXT NOT NULL CHECK(depreciation_month ~ '^\d{4}-\d{2}$'),ordinal BIGINT NOT NULL CHECK(ordinal>0),
 period_id TEXT NOT NULL REFERENCES finance_periods(id),entry_date TEXT NOT NULL,amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),asset_version BIGINT NOT NULL,
 command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL,UNIQUE(asset_id,depreciation_month),UNIQUE(asset_id,ordinal)
);
CREATE TABLE finance_asset_disposals (
 id TEXT PRIMARY KEY,asset_id TEXT NOT NULL REFERENCES finance_assets(id),company_id TEXT NOT NULL,
 asset_version BIGINT NOT NULL,period_id TEXT NOT NULL REFERENCES finance_periods(id),entry_date TEXT NOT NULL,
 proceeds_minor BIGINT NOT NULL CHECK(proceeds_minor>=0),proceeds_account_id TEXT REFERENCES finance_accounts(id),
 gain_account_id TEXT NOT NULL REFERENCES finance_accounts(id),loss_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 evidence_ref TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','cancelled')),
 journal_id TEXT UNIQUE REFERENCES finance_journals(id),created_by TEXT NOT NULL,decided_by TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(proceeds_minor=0 OR proceeds_account_id IS NOT NULL)
);
CREATE UNIQUE INDEX finance_asset_one_live_disposal ON finance_asset_disposals(asset_id) WHERE status IN ('draft','posted');
CREATE FUNCTION finance_asset_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 RAISE EXCEPTION 'ASSET_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER finance_asset_events_immutable BEFORE UPDATE OR DELETE ON finance_asset_events FOR EACH ROW EXECUTE FUNCTION finance_asset_evidence_immutable();
CREATE TRIGGER finance_asset_depreciations_immutable BEFORE UPDATE OR DELETE ON finance_asset_depreciations FOR EACH ROW EXECUTE FUNCTION finance_asset_evidence_immutable();
CREATE FUNCTION finance_asset_disposal_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.asset_id,NEW.company_id,NEW.asset_version,NEW.period_id,NEW.entry_date,NEW.proceeds_minor,
 NEW.proceeds_account_id,NEW.gain_account_id,NEW.loss_account_id,NEW.evidence_ref,NEW.created_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.asset_id,OLD.company_id,OLD.asset_version,OLD.period_id,OLD.entry_date,OLD.proceeds_minor,
 OLD.proceeds_account_id,OLD.gain_account_id,OLD.loss_account_id,OLD.evidence_ref,OLD.created_by,OLD.created_at)
 THEN RAISE EXCEPTION 'ASSET_DISPOSAL_REVISION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status<>'draft' THEN RAISE EXCEPTION 'ASSET_DISPOSAL_FINAL' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_asset_disposal_guard BEFORE UPDATE OR DELETE ON finance_asset_disposals FOR EACH ROW EXECUTE FUNCTION finance_asset_disposal_guard();
CREATE FUNCTION finance_asset_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (OLD.activation_journal_id IS NOT NULL AND
 (NEW.company_id,NEW.asset_number,NEW.account_id,NEW.cost_minor,NEW.residual_minor,NEW.useful_life_months,NEW.in_service_date,
 NEW.depreciation_account_id,NEW.depreciation_expense_account_id,NEW.currency,NEW.activation_journal_id,NEW.depreciation_convention)
 IS DISTINCT FROM (OLD.company_id,OLD.asset_number,OLD.account_id,OLD.cost_minor,OLD.residual_minor,OLD.useful_life_months,OLD.in_service_date,
 OLD.depreciation_account_id,OLD.depreciation_expense_account_id,OLD.currency,OLD.activation_journal_id,OLD.depreciation_convention))
 THEN RAISE EXCEPTION 'ACTIVATED_ASSET_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_asset_source_guard BEFORE UPDATE OR DELETE ON finance_assets FOR EACH ROW EXECUTE FUNCTION finance_asset_source_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['finance_asset_events','finance_asset_depreciations','finance_asset_disposals'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;
DO $$ DECLARE runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 FOREACH runtime_role IN ARRAY ARRAY[regexp_replace(current_user,'_migration$','_app'),regexp_replace(current_user,'_migration$','_jobs')] LOOP
  EXECUTE format('GRANT SELECT,INSERT ON finance_asset_events,finance_asset_depreciations TO %I',runtime_role);
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_asset_disposals TO %I',runtime_role);
 END LOOP;
END $$;
