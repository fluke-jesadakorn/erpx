-- Signed originals are distinct immutable revisions of an approved document.
CREATE TABLE legal_executed_copies (
 id TEXT PRIMARY KEY,
 item_id TEXT NOT NULL REFERENCES legal_items(id),
 document_id TEXT NOT NULL UNIQUE REFERENCES legal_documents(id),
 approved_cycle INTEGER NOT NULL,
 approved_document_id TEXT NOT NULL REFERENCES legal_documents(id),
 approved_hash TEXT NOT NULL,
 executed_hash TEXT NOT NULL,
 metadata JSONB NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending_review' CHECK(state IN ('pending_review','changes_required','confirmed')),
 imported_by TEXT NOT NULL,
 imported_at BIGINT NOT NULL,
 reviewed_by TEXT,
 reviewed_at BIGINT,
 version INTEGER NOT NULL DEFAULT 1,
 FOREIGN KEY(item_id,approved_cycle) REFERENCES legal_approvals(item_id,cycle),
 CHECK(document_id<>approved_document_id),
 CHECK((state='confirmed')=(reviewed_by IS NOT NULL AND reviewed_at IS NOT NULL))
);
CREATE INDEX legal_executed_item ON legal_executed_copies(item_id,imported_at DESC,id);
CREATE TABLE legal_executed_reviews (
 id TEXT PRIMARY KEY,
 executed_copy_id TEXT NOT NULL REFERENCES legal_executed_copies(id),
 item_version INTEGER NOT NULL,
 decision TEXT NOT NULL CHECK(decision IN ('confirm','changes_required')),
 checks JSONB NOT NULL,
 reason TEXT NOT NULL,
 reviewer_id TEXT NOT NULL,
 created_at BIGINT NOT NULL
);
CREATE TRIGGER legal_executed_review_immutable BEFORE UPDATE OR DELETE ON legal_executed_reviews FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
CREATE FUNCTION legal_executed_copy_preserve() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-ARRAY['state','reviewed_by','reviewed_at','version']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['state','reviewed_by','reviewed_at','version']) OR OLD.state='confirmed' THEN
 RAISE EXCEPTION 'LEGAL_EXECUTED_COPY_IMMUTABLE'; END IF;
 IF NEW.state='confirmed' AND (iam_same_person(NEW.reviewed_by,NEW.imported_by,(SELECT company_id FROM legal_items WHERE id=NEW.item_id)) OR NOT EXISTS(SELECT 1 FROM legal_executed_reviews r WHERE r.executed_copy_id=NEW.id AND r.decision='confirm' AND r.reviewer_id=NEW.reviewed_by AND r.created_at=NEW.reviewed_at AND r.checks @> '{"contentMatches":true,"pagesComplete":true,"partiesVerified":true,"signaturesComplete":true,"authorityVerified":true}'::jsonb)) THEN
 RAISE EXCEPTION 'LEGAL_EXECUTED_REVIEW_REQUIRED'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER legal_executed_copy_preserve BEFORE UPDATE OR DELETE ON legal_executed_copies FOR EACH ROW EXECUTE FUNCTION legal_executed_copy_preserve();

-- Use live IAM and organization state; a branch/department assignment must cover
-- the record's exact scope, including the absence of a local scope.
CREATE FUNCTION legal_person_eligible(company_key TEXT,user_key TEXT,branch_key TEXT,department_key TEXT,permission_key TEXT DEFAULT NULL) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
 SELECT 1 FROM assignments a
 WHERE a.company_id=company_key AND a.user_id=user_key
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND EXISTS(SELECT 1 FROM iam_roles r WHERE r.id=a.role AND r.company_id IN ('*',company_key) AND r.status='active')
 AND (permission_key IS NULL OR iam_role_allows(a.role,company_key,permission_key))
 AND EXISTS(SELECT 1 FROM organization_nodes c WHERE c.id=company_key AND c.status='active')
 AND (a.scope_kind='holding' OR
  a.scope_kind='branch' AND a.scope_node_id=branch_key AND EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=company_key AND n.status='active') OR
  a.scope_kind='department' AND a.scope_node_id=department_key AND EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=company_key AND n.status='active' AND (n.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=company_key AND b.status='active'))))
 );
$$;
INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','legal_requester','Legal Requester',true,1,'active');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
VALUES('*','legal_requester',1,'["legal.read","legal.create","legal.edit","iam.self","records.read","self.tasks","self.requests"]'::jsonb,'[]'::jsonb,'["holding","branch","department"]'::jsonb,'system:legal-v1',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
UPDATE iam_permissions SET definition=jsonb_set(definition::jsonb,'{roles}',(definition::jsonb->'roles')||'["legal_requester"]'::jsonb) WHERE id IN ('legal.read','legal.create','legal.edit');
