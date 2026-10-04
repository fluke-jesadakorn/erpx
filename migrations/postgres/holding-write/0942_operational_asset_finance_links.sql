ALTER TABLE operational_assets ADD COLUMN finance_asset_id TEXT REFERENCES finance_assets(id);
ALTER TABLE operational_assets ADD COLUMN finance_asset_version BIGINT;
ALTER TABLE operational_assets ADD CONSTRAINT operational_asset_finance_snapshot CHECK((finance_asset_id IS NULL AND finance_asset_version IS NULL) OR (finance_asset_id IS NOT NULL AND finance_asset_version>0));
CREATE UNIQUE INDEX operational_asset_finance_once ON operational_assets(finance_asset_id) WHERE finance_asset_id IS NOT NULL;
CREATE TABLE operational_asset_finance_links (
 id TEXT PRIMARY KEY,asset_id TEXT NOT NULL UNIQUE REFERENCES operational_assets(id),finance_asset_id TEXT NOT NULL UNIQUE REFERENCES finance_assets(id),
 asset_version BIGINT NOT NULL,finance_asset_version BIGINT NOT NULL,cost_minor BIGINT NOT NULL,currency TEXT NOT NULL,
 reason TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE FUNCTION operational_asset_finance_link_history() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'OPERATIONAL_ASSET_FINANCE_LINK_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER operational_asset_finance_link_history BEFORE UPDATE OR DELETE ON operational_asset_finance_links FOR EACH ROW EXECUTE FUNCTION operational_asset_finance_link_history();
CREATE FUNCTION operational_asset_finance_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.finance_asset_id IS NOT NULL AND (NEW.finance_asset_id,NEW.finance_asset_version) IS DISTINCT FROM (OLD.finance_asset_id,OLD.finance_asset_version)
 THEN RAISE EXCEPTION 'OPERATIONAL_ASSET_FINANCE_LINK_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF NEW.finance_asset_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_assets f WHERE f.id=NEW.finance_asset_id AND f.company_id=NEW.company_id AND f.currency=NEW.currency AND f.cost_minor=NEW.cost_minor)
 THEN RAISE EXCEPTION 'OPERATIONAL_ASSET_FINANCE_SOURCE_MISMATCH' USING ERRCODE='23514'; END IF;
 IF NEW.lifecycle_status='retired' AND OLD.lifecycle_status<>'retired' AND NEW.cost_minor>0 AND NOT EXISTS(
   SELECT 1 FROM finance_assets f JOIN finance_asset_disposals d ON d.asset_id=f.id AND d.company_id=f.company_id
   JOIN finance_journals j ON j.id=d.journal_id AND j.company_id=f.company_id
   WHERE f.id=NEW.finance_asset_id AND f.company_id=NEW.company_id AND f.status='retired' AND d.status='posted' AND j.status='posted')
 THEN RAISE EXCEPTION 'OPERATIONAL_ASSET_FINANCE_DISPOSAL_REQUIRED' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER operational_asset_finance_guard BEFORE UPDATE ON operational_assets FOR EACH ROW EXECUTE FUNCTION operational_asset_finance_guard();
CREATE FUNCTION finance_disposal_operational_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE physical operational_assets%ROWTYPE;
BEGIN
 IF NEW.status<>'posted' THEN RETURN NEW; END IF;
 SELECT * INTO physical FROM operational_assets WHERE finance_asset_id=NEW.asset_id FOR UPDATE;
 IF FOUND AND (physical.lifecycle_status<>'retirement_pending' OR EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.asset_id=physical.id AND w.status NOT IN ('completed','cancelled')))
 THEN RAISE EXCEPTION 'FINANCE_OPERATIONAL_RETIREMENT_REQUIRED' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_disposal_operational_guard BEFORE INSERT OR UPDATE ON finance_asset_disposals FOR EACH ROW EXECUTE FUNCTION finance_disposal_operational_guard();
CREATE UNIQUE INDEX maintenance_one_active_work_per_asset ON maintenance_work_orders(asset_id) WHERE status NOT IN ('completed','cancelled');
INSERT INTO work_cache_revisions(table_name) VALUES('operational_asset_finance_links') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON operational_asset_finance_links FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON operational_asset_finance_links FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
