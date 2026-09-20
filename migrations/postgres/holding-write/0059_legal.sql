-- Legal-owned aggregates; legacy workspace records remain untouched.
CREATE TABLE legal_settings(company_id TEXT PRIMARY KEY,enabled BOOLEAN NOT NULL DEFAULT false,ai_enabled BOOLEAN NOT NULL DEFAULT false,version INTEGER NOT NULL DEFAULT 1);
CREATE TABLE legal_items(
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT,department_id TEXT,
 kind TEXT NOT NULL CHECK(kind IN ('request','matter','contract','obligation','compliance','license','corporate','ip','privacy','counsel','template','rule')),
 title TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',classification TEXT NOT NULL CHECK(classification IN ('internal','confidential','restricted')),
 country TEXT NOT NULL,jurisdiction TEXT NOT NULL,language TEXT NOT NULL,currency TEXT NOT NULL,timezone TEXT NOT NULL,
 priority TEXT NOT NULL,due_date TEXT,owner_id TEXT NOT NULL,created_by TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'draft',version INTEGER NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,UNIQUE(company_id,id));
CREATE INDEX legal_scope_idx ON legal_items(company_id,branch_id,department_id,kind,updated_at DESC,id);
CREATE INDEX legal_due_idx ON legal_items(company_id,due_date) WHERE due_date IS NOT NULL;
CREATE TABLE legal_members(item_id TEXT NOT NULL REFERENCES legal_items(id),user_id TEXT NOT NULL,access TEXT NOT NULL CHECK(access IN ('read','edit')),PRIMARY KEY(item_id,user_id));
CREATE TABLE legal_requests(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_matters(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_contracts(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_obligations(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_compliance(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_licenses(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_corporate(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_ip_assets(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_privacy(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_counsel(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_templates(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_rules(item_id TEXT PRIMARY KEY REFERENCES legal_items(id),data JSONB NOT NULL);
CREATE TABLE legal_documents(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),version INTEGER NOT NULL,name TEXT NOT NULL,media_type TEXT NOT NULL,checksum TEXT NOT NULL,bytes BIGINT NOT NULL,body TEXT NOT NULL DEFAULT '',object_key TEXT UNIQUE,locked BOOLEAN NOT NULL DEFAULT false,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(item_id,version));
CREATE TABLE legal_events(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),action TEXT NOT NULL,actor_id TEXT NOT NULL,note TEXT NOT NULL,version INTEGER NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX legal_events_item_idx ON legal_events(item_id,created_at,id);
CREATE TABLE legal_comments(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),document_id TEXT REFERENCES legal_documents(id),section TEXT NOT NULL DEFAULT '',body TEXT NOT NULL,visibility TEXT NOT NULL CHECK(visibility IN ('internal','external')),created_by TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE legal_links(source_id TEXT NOT NULL REFERENCES legal_items(id),target_id TEXT NOT NULL REFERENCES legal_items(id),relation TEXT NOT NULL CHECK(relation IN ('related','evidence','amends','obligation','derived')),PRIMARY KEY(source_id,target_id,relation),CHECK(source_id<>target_id));
CREATE TABLE legal_holds(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),reason TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('active','release_requested','released')),created_by TEXT NOT NULL,release_requested_by TEXT,released_by TEXT,created_at BIGINT NOT NULL,released_at BIGINT);
CREATE TABLE legal_hold_targets(hold_id TEXT NOT NULL REFERENCES legal_holds(id),item_id TEXT NOT NULL REFERENCES legal_items(id),PRIMARY KEY(hold_id,item_id));
CREATE TABLE legal_envelopes(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),document_id TEXT NOT NULL REFERENCES legal_documents(id),document_hash TEXT NOT NULL,approved_version INTEGER NOT NULL,state TEXT NOT NULL CHECK(state IN ('sent','completed','declined','cancelled','expired')),consent TEXT NOT NULL,expires_at BIGINT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE UNIQUE INDEX legal_envelope_active ON legal_envelopes(item_id) WHERE state='sent';
CREATE TABLE legal_signers(envelope_id TEXT NOT NULL REFERENCES legal_envelopes(id),user_id TEXT NOT NULL,ordinal INTEGER NOT NULL,state TEXT NOT NULL CHECK(state IN ('pending','signed','declined')),viewed_at BIGINT,signed_at BIGINT,evidence JSONB,PRIMARY KEY(envelope_id,user_id),UNIQUE(envelope_id,ordinal));
CREATE TABLE legal_approvals(item_id TEXT NOT NULL REFERENCES legal_items(id),cycle INTEGER NOT NULL,document_id TEXT NOT NULL REFERENCES legal_documents(id),document_hash TEXT NOT NULL,signers JSONB NOT NULL,reviewer_id TEXT NOT NULL,decision TEXT NOT NULL,created_at BIGINT NOT NULL,PRIMARY KEY(item_id,cycle));
CREATE TABLE legal_shares(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),document_id TEXT NOT NULL REFERENCES legal_documents(id),email TEXT NOT NULL,user_id TEXT,expires_at BIGINT NOT NULL,revoked_at BIGINT,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,version INTEGER NOT NULL DEFAULT 1);
CREATE INDEX legal_shares_email ON legal_shares(lower(email),expires_at) WHERE revoked_at IS NULL;
CREATE TABLE legal_access_events(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),document_id TEXT,actor_id TEXT NOT NULL,action TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE legal_saved_views(company_id TEXT NOT NULL,user_id TEXT NOT NULL,name TEXT NOT NULL,query JSONB NOT NULL,PRIMARY KEY(company_id,user_id,name));
CREATE TABLE legal_reminders(item_id TEXT NOT NULL REFERENCES legal_items(id),due_date TEXT NOT NULL,days INTEGER NOT NULL,user_id TEXT NOT NULL,created_at BIGINT NOT NULL,PRIMARY KEY(item_id,due_date,days,user_id));
CREATE TABLE legal_ai_proposals(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),source_version INTEGER NOT NULL,source_hash TEXT NOT NULL,question TEXT NOT NULL,proposal JSONB,state TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,error TEXT);
-- Prevent application-level document history rewrites, including accidental maintenance.
CREATE FUNCTION legal_document_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_PRESERVATION_REQUIRED'; END IF;
 IF (to_jsonb(NEW)-'locked') IS DISTINCT FROM (to_jsonb(OLD)-'locked') OR (OLD.locked AND NOT NEW.locked) THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_IMMUTABLE'; END IF;
 RETURN NEW; END $$;
CREATE TRIGGER legal_document_immutable BEFORE UPDATE OR DELETE ON legal_documents FOR EACH ROW EXECUTE FUNCTION legal_document_immutable();
CREATE FUNCTION legal_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LEGAL_EVIDENCE_IMMUTABLE'; END $$;
CREATE TRIGGER legal_events_immutable BEFORE UPDATE OR DELETE ON legal_events FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
CREATE TRIGGER legal_access_immutable BEFORE UPDATE OR DELETE ON legal_access_events FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
INSERT INTO iam_permissions VALUES('legal.read','{"id": "legal.read", "label": "Legal: read", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.create','{"id": "legal.create", "label": "Legal: create", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.edit','{"id": "legal.edit", "label": "Legal: edit", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.review','{"id": "legal.review", "label": "Legal: review", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.approve','{"id": "legal.approve", "label": "Legal: approve", "module": "legal", "namespace": "customer", "availability": "live", "risk": "privileged", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.sign','{"id": "legal.sign", "label": "Legal: sign", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.share','{"id": "legal.share", "label": "Legal: share", "module": "legal", "namespace": "customer", "availability": "live", "risk": "privileged", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.export','{"id": "legal.export", "label": "Legal: export", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.hold','{"id": "legal.hold", "label": "Legal: hold", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.hold.release','{"id": "legal.hold.release", "label": "Legal: hold.release", "module": "legal", "namespace": "customer", "availability": "live", "risk": "privileged", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.manage','{"id": "legal.manage", "label": "Legal: manage", "module": "legal", "namespace": "customer", "availability": "live", "risk": "privileged", "roles": ["holding_owner", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('legal.ai','{"id": "legal.ai", "label": "Legal: ai", "module": "legal", "namespace": "customer", "availability": "live", "risk": "normal", "roles": ["holding_owner", "legal_counsel", "legal_manager"], "conditions": ["Explicit membership for confidential and restricted records"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_roles VALUES('*','legal_counsel','Legal Counsel',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','legal_counsel',1,'["legal.read", "legal.create", "legal.edit", "legal.review", "legal.approve", "legal.sign", "legal.share", "legal.export", "legal.hold", "legal.hold.release", "legal.ai", "iam.self", "records.read", "self.tasks", "self.requests"]'::jsonb,'[]'::jsonb,'["holding","branch","department"]'::jsonb,'system:legal',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
INSERT INTO iam_roles VALUES('*','legal_manager','Legal Manager',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','legal_manager',1,'["legal.read", "legal.create", "legal.edit", "legal.review", "legal.approve", "legal.sign", "legal.share", "legal.export", "legal.hold", "legal.hold.release", "legal.manage", "legal.ai", "iam.self", "records.read", "self.tasks", "self.requests"]'::jsonb,'[]'::jsonb,'["holding","branch","department"]'::jsonb,'system:legal',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json || '["legal.read","legal.create","legal.edit","legal.review","legal.approve","legal.sign","legal.share","legal.export","legal.hold","legal.hold.release","legal.manage","legal.ai"]'::jsonb,
v.grantable_json || '["legal.read","legal.create","legal.edit","legal.review","legal.approve","legal.sign","legal.share","legal.export","legal.hold","legal.hold.release","legal.manage","legal.ai"]'::jsonb,
v.scopes_json,'system:legal',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='holding_owner';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='holding_owner';
CREATE TABLE legal_portal_receipts(id TEXT PRIMARY KEY,user_id TEXT NOT NULL,request_hash TEXT NOT NULL,result_json TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE legal_document_inspections(id TEXT PRIMARY KEY,document_id TEXT NOT NULL REFERENCES legal_documents(id),kind TEXT NOT NULL,result_json TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX legal_inspections_document ON legal_document_inspections(document_id,created_at);
ALTER TABLE legal_ai_proposals ADD COLUMN actor_json TEXT;
ALTER TABLE legal_ai_proposals ADD COLUMN lease_until BIGINT;
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['legal_items','legal_members','legal_settings','legal_envelopes','legal_signers','legal_shares'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
