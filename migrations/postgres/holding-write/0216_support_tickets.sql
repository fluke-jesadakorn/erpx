-- Staff-assisted source-linked support. No customer identity or acknowledgement is inferred.
CREATE TABLE support_tickets (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,
 order_id TEXT NOT NULL REFERENCES commerce_orders(id),customer_id TEXT NOT NULL REFERENCES finance_parties(id),
 title TEXT NOT NULL,issue TEXT NOT NULL,owner_id TEXT NOT NULL,due_date TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','in_progress','waiting_customer','resolved','closed','cancelled')),
 blocked_reason TEXT,resolution TEXT,resolved_by TEXT,resolved_at BIGINT,confirmation_note_id TEXT REFERENCES customer_contact_notes(id),confirmation_note_version BIGINT,
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status<>'resolved' OR resolution IS NOT NULL),CHECK(status<>'closed' OR (resolution IS NOT NULL AND confirmation_note_id IS NOT NULL AND confirmation_note_version IS NOT NULL))
);
CREATE TABLE support_ticket_events (
 id TEXT PRIMARY KEY,ticket_id TEXT NOT NULL REFERENCES support_tickets(id),version BIGINT NOT NULL,
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,owner_id TEXT NOT NULL,due_date TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,evidence_json JSONB NOT NULL,UNIQUE(ticket_id,version)
);
CREATE INDEX support_ticket_queue ON support_tickets(company_id,branch_id,department_id,status,due_date);
CREATE FUNCTION support_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SUPPORT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER support_event_immutable BEFORE UPDATE OR DELETE ON support_ticket_events FOR EACH ROW EXECUTE FUNCTION support_evidence_guard();
CREATE FUNCTION support_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF (NEW.party_id,NEW.company_id,NEW.branch_id,NEW.department_id) IS DISTINCT FROM (OLD.party_id,OLD.company_id,OLD.branch_id,OLD.department_id) AND EXISTS(SELECT 1 FROM support_tickets WHERE order_id=OLD.id) THEN RAISE EXCEPTION 'SUPPORT_SOURCE_HAS_EVIDENCE' USING ERRCODE='23514'; END IF;RETURN NEW;
END $$;
CREATE TRIGGER support_source_scope BEFORE UPDATE ON commerce_orders FOR EACH ROW EXECUTE FUNCTION support_source_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['support_tickets','support_ticket_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;
INSERT INTO iam_roles VALUES('*','support_agent','Support agent',true,1,'active'),('*','support_manager','Support manager',true,1,'active');
INSERT INTO iam_role_revisions VALUES
('*','support_agent',1,'["iam.self","self.requests","self.tasks","self.roster","leave.self","support.read","support.create","support.work"]','[]','["holding","branch","department"]','system',0),
('*','support_manager',1,'["iam.self","self.requests","self.tasks","self.roster","leave.self","support.read","support.create","support.work","support.manage"]','[]','["holding","branch","department"]','system',0);
INSERT INTO iam_permissions SELECT id,jsonb_build_object('id',id,'label',id,'module','support','roles',roles,'conditions',jsonb_build_array('Source scope; current eligible owner; independent recorded customer confirmation'),'risk',risk,'availability','live','namespace','customer') FROM(VALUES
 ('support.read','["holding_owner","support_agent","support_manager","sales_operator","sales_manager"]'::jsonb,'normal'),
 ('support.create','["holding_owner","support_agent","support_manager","sales_operator","sales_manager"]'::jsonb,'normal'),
 ('support.work','["holding_owner","support_agent","support_manager"]'::jsonb,'normal'),
 ('support.manage','["holding_owner","support_manager"]'::jsonb,'privileged')) p(id,roles,risk);
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||CASE WHEN r.id='holding_owner' THEN '["support.read","support.create","support.work","support.manage"]'::jsonb ELSE '["support.read","support.create"]'::jsonb END,v.grantable_json||CASE WHEN r.id='holding_owner' THEN '["support.read","support.create","support.work","support.manage"]'::jsonb ELSE '[]'::jsonb END,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id IN('holding_owner','sales_operator','sales_manager');
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id IN('holding_owner','sales_operator','sales_manager');
