CREATE TABLE crm_leads (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL REFERENCES organization_nodes(id), department_id TEXT REFERENCES organization_nodes(id),
 title TEXT NOT NULL, contact_name TEXT NOT NULL, contact_detail TEXT NOT NULL, source TEXT NOT NULL, brief TEXT NOT NULL,
 owner_id TEXT NOT NULL, due_date TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','disqualified','qualified')),
 version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE crm_opportunities (
 id TEXT PRIMARY KEY, lead_id TEXT NOT NULL UNIQUE REFERENCES crm_leads(id), customer_id TEXT NOT NULL REFERENCES finance_parties(id),
 stage TEXT NOT NULL DEFAULT 'discovery' CHECK(stage IN ('discovery','proposal','negotiation','won','lost')),
 expected_minor BIGINT NOT NULL CHECK(expected_minor>=0), currency TEXT NOT NULL, close_date TEXT NOT NULL,
 quote_id TEXT UNIQUE REFERENCES commerce_orders(id), quote_version BIGINT, sales_id TEXT UNIQUE REFERENCES commerce_orders(id),
 version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE crm_case_events (
 id TEXT PRIMARY KEY, lead_id TEXT NOT NULL REFERENCES crm_leads(id), opportunity_id TEXT REFERENCES crm_opportunities(id),
 version BIGINT NOT NULL, action TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL, snapshot_json JSONB NOT NULL, created_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX crm_lead_event_version ON crm_case_events(lead_id,version) WHERE opportunity_id IS NULL;
CREATE UNIQUE INDEX crm_opportunity_event_version ON crm_case_events(opportunity_id,version) WHERE opportunity_id IS NOT NULL;
CREATE INDEX crm_lead_queue ON crm_leads(company_id,branch_id,department_id,status,due_date);
CREATE FUNCTION crm_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'CRM_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER crm_case_event_guard BEFORE UPDATE OR DELETE ON crm_case_events FOR EACH ROW EXECUTE FUNCTION crm_evidence_guard();
-- Preserve the accepted customer and authorization scope of a quotation linked to a deal.
CREATE FUNCTION crm_quote_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF (NEW.party_id,NEW.company_id,NEW.branch_id,NEW.department_id) IS DISTINCT FROM (OLD.party_id,OLD.company_id,OLD.branch_id,OLD.department_id)
 AND EXISTS(SELECT 1 FROM crm_opportunities WHERE quote_id=OLD.id OR sales_id=OLD.id) THEN RAISE EXCEPTION 'CRM_SOURCE_HAS_EVIDENCE' USING ERRCODE='23514'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER crm_quote_source BEFORE UPDATE ON commerce_orders FOR EACH ROW EXECUTE FUNCTION crm_quote_source_guard();
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['crm_leads','crm_opportunities','crm_case_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
