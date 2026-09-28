-- CRM identities are intentionally distinct from Finance parties and PMS guests.
CREATE TABLE crm_accounts (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('business','individual')),
 name TEXT NOT NULL, finance_party_id TEXT REFERENCES finance_parties(id), review_evidence TEXT,
 reviewed_by TEXT, created_by TEXT NOT NULL, version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE crm_contacts (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, account_id TEXT NOT NULL REFERENCES crm_accounts(id),
 name TEXT NOT NULL, detail TEXT NOT NULL, created_by TEXT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE TABLE crm_guest_links (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, contact_id TEXT NOT NULL REFERENCES crm_contacts(id),
 guest_id TEXT NOT NULL UNIQUE REFERENCES pms_records(id), property_id TEXT NOT NULL REFERENCES pms_properties(id),
 evidence TEXT NOT NULL, reviewed_by TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(contact_id,guest_id)
);
CREATE TABLE marketing_campaigns (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, code TEXT NOT NULL, name TEXT NOT NULL,
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, UNIQUE(company_id,code)
);
CREATE TABLE marketing_campaign_costs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, campaign_id TEXT NOT NULL REFERENCES marketing_campaigns(id),
 journal_line_id TEXT NOT NULL UNIQUE REFERENCES finance_journal_lines(id), evidence TEXT NOT NULL,
 reviewed_by TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE TABLE marketing_touches (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, lead_id TEXT NOT NULL REFERENCES crm_leads(id),
 campaign_id TEXT REFERENCES marketing_campaigns(id), source TEXT NOT NULL, evidence TEXT NOT NULL,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('staff','form','pms')), source_id TEXT NOT NULL,
 occurred_at BIGINT NOT NULL, created_by TEXT NOT NULL,
 UNIQUE(lead_id,source_kind,source_id)
);
CREATE TABLE marketing_public_limits (key TEXT PRIMARY KEY,bucket BIGINT NOT NULL,count BIGINT NOT NULL);
CREATE TABLE marketing_public_receipts (key TEXT PRIMARY KEY,request_hash TEXT NOT NULL,candidate_id TEXT NOT NULL UNIQUE REFERENCES marketing_lead_candidates(id),created_at BIGINT NOT NULL);
ALTER TABLE crm_leads ADD COLUMN account_id TEXT REFERENCES crm_accounts(id);
ALTER TABLE crm_leads ADD COLUMN contact_id TEXT REFERENCES crm_contacts(id);
ALTER TABLE crm_opportunities ADD COLUMN reservation_id TEXT UNIQUE REFERENCES pms_records(id);
ALTER TABLE crm_opportunities ADD COLUMN win_source TEXT CHECK(win_source IN ('sales','pms'));
ALTER TABLE crm_opportunities ADD COLUMN won_at BIGINT;
UPDATE crm_opportunities SET win_source='sales',won_at=updated_at WHERE stage='won' AND sales_id IS NOT NULL;
-- A legacy lead gets its own identity. Matching names or email addresses never merges people.
INSERT INTO crm_accounts(id,company_id,kind,name,finance_party_id,review_evidence,reviewed_by,created_by,created_at,updated_at)
 SELECT 'legacy-account-'||l.id,l.company_id,'individual',l.contact_name,v.customer_id,
 CASE WHEN v.customer_id IS NULL THEN NULL ELSE 'Existing qualified opportunity '||v.id END,
 CASE WHEN v.customer_id IS NULL THEN NULL ELSE 'migration' END,l.created_by,l.created_at,l.updated_at
 FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id;
INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at)
 SELECT 'legacy-contact-'||id,company_id,'legacy-account-'||id,contact_name,contact_detail,created_by,created_at,updated_at FROM crm_leads;
UPDATE crm_leads SET account_id='legacy-account-'||id,contact_id='legacy-contact-'||id;
CREATE INDEX crm_accounts_company ON crm_accounts(company_id,finance_party_id);
CREATE INDEX crm_contacts_account ON crm_contacts(company_id,account_id);
CREATE INDEX crm_touches_lead_time ON marketing_touches(company_id,lead_id,occurred_at,id);
CREATE INDEX marketing_cost_campaign ON marketing_campaign_costs(company_id,campaign_id);
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['crm_accounts','crm_contacts','crm_guest_links','marketing_campaigns','marketing_campaign_costs','marketing_touches'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
