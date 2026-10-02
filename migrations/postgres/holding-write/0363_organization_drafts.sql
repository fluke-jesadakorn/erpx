CREATE TABLE organization_drafts (
 id text PRIMARY KEY, company_id text NOT NULL, owner_id text NOT NULL,
 state text NOT NULL CHECK(state IN ('draft','submitted','discarded')) DEFAULT 'draft',
 version integer NOT NULL DEFAULT 1, definition jsonb NOT NULL, created_at bigint NOT NULL, updated_at bigint NOT NULL
);
CREATE UNIQUE INDEX organization_draft_owner ON organization_drafts(company_id,owner_id) WHERE state='draft';
CREATE TABLE organization_draft_requests (
 id text PRIMARY KEY, draft_id text NOT NULL REFERENCES organization_drafts(id), company_id text NOT NULL,
 item_keys jsonb NOT NULL, definition jsonb NOT NULL, baseline jsonb NOT NULL,
 dependencies jsonb NOT NULL, change_id text REFERENCES organization_node_changes(id),
 state text NOT NULL CHECK(state IN ('scheduled','waiting_approval','waiting_dependency','applied','needs_review')),
 actor_json jsonb NOT NULL, effective_at bigint NOT NULL, last_error text, applied_at bigint,
 created_at bigint NOT NULL, updated_at bigint NOT NULL
);
CREATE INDEX organization_draft_due ON organization_draft_requests(state,effective_at);
CREATE TABLE organization_draft_reservations (
 company_id text NOT NULL, kind text NOT NULL, entity_id text NOT NULL,
 request_id text NOT NULL REFERENCES organization_draft_requests(id),
 PRIMARY KEY(company_id,kind,entity_id)
);
CREATE FUNCTION guard_organization_draft_request() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='people_lifecycle_cases' AND NEW.status IN ('waiting_it','pending_review','scheduled') THEN
  IF EXISTS(SELECT 1 FROM organization_draft_reservations r WHERE r.company_id=NEW.company_id AND ((r.kind='person' AND r.entity_id=NEW.engagement_id) OR (r.kind='position' AND r.entity_id=NEW.definition->>'positionId'))) THEN
   RAISE EXCEPTION 'ORGANIZATION_DRAFT_RESERVED: this placement is reserved by a confirmed organization draft' USING ERRCODE='23514';
  END IF;
 ELSIF TG_TABLE_NAME='organization_position_moves' AND NEW.status='scheduled' THEN
  IF EXISTS(SELECT 1 FROM organization_draft_reservations r WHERE r.company_id=NEW.company_id AND ((r.kind='position' AND r.entity_id=NEW.position_id) OR (r.kind='person' AND EXISTS(SELECT 1 FROM hr_engagements e WHERE e.id=r.entity_id AND e.position_id=NEW.position_id)))) THEN
   RAISE EXCEPTION 'ORGANIZATION_DRAFT_RESERVED: this position is reserved by a confirmed organization draft' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER people_lifecycle_draft_reservation BEFORE INSERT OR UPDATE ON people_lifecycle_cases FOR EACH ROW EXECUTE FUNCTION guard_organization_draft_request();
CREATE TRIGGER position_move_draft_reservation BEFORE INSERT OR UPDATE ON organization_position_moves FOR EACH ROW EXECUTE FUNCTION guard_organization_draft_request();
DO $$ DECLARE app_role text; jobs_role text; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 app_role:=regexp_replace(current_user,'_migration$','_app'); jobs_role:=regexp_replace(current_user,'_migration$','_jobs');
 EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON organization_drafts,organization_draft_requests,organization_draft_reservations TO %I,%I',app_role,jobs_role);
END $$;
