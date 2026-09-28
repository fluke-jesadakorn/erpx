-- Additive organization policy sources. Migration does not issue provider writes.
CREATE TABLE organization_gpo_rules (
 id text PRIMARY KEY, company_id text NOT NULL,
 target_kind text NOT NULL CHECK(target_kind IN ('level','department','position')), target_id text NOT NULL,
 version integer NOT NULL CHECK(version>0), revision integer NOT NULL CHECK(revision>0),
 connection_id text NOT NULL REFERENCES iam_connections(id), gpo_id text NOT NULL,
 user_filter_group_id text NOT NULL, computer_target_ou_id text NOT NULL,
 active boolean NOT NULL DEFAULT true,
 directory_config_status text NOT NULL DEFAULT 'pending' CHECK(directory_config_status IN ('pending','directory_applied','blocked')),
 directory_config_error text, legacy_position_id text,
 last_actor_id text NOT NULL, last_reason text NOT NULL, updated_at bigint NOT NULL,
 UNIQUE(company_id,legacy_position_id)
);
CREATE INDEX organization_gpo_rules_target ON organization_gpo_rules(company_id,target_kind,target_id) WHERE active;
CREATE TABLE organization_gpo_previews (
 token text PRIMARY KEY, company_id text NOT NULL, user_id text NOT NULL,
 assignment_id text NOT NULL, state_token text NOT NULL, expires_at bigint NOT NULL
);
CREATE TABLE organization_gpo_rule_revisions (
 rule_id text NOT NULL REFERENCES organization_gpo_rules(id), revision integer NOT NULL,
 snapshot jsonb NOT NULL, created_at bigint NOT NULL, PRIMARY KEY(rule_id,revision)
);
CREATE FUNCTION organization_gpo_rule_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='INSERT' OR NEW.revision IS DISTINCT FROM OLD.revision THEN
  INSERT INTO organization_gpo_rule_revisions(rule_id,revision,snapshot,created_at) VALUES(NEW.id,NEW.revision,to_jsonb(NEW),NEW.updated_at);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER organization_gpo_rule_history AFTER INSERT OR UPDATE ON organization_gpo_rules FOR EACH ROW EXECUTE FUNCTION organization_gpo_rule_history();
CREATE TRIGGER organization_gpo_rule_revisions_immutable BEFORE UPDATE OR DELETE ON organization_gpo_rule_revisions FOR EACH ROW EXECUTE FUNCTION department_authority_history_immutable();

INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,active,directory_config_status,directory_config_error,legacy_position_id,last_actor_id,last_reason,updated_at)
SELECT 'legacy-position:'||p.position_id,p.company_id,'position',p.position_id,GREATEST(p.version,1),GREATEST(p.revision,1),p.connection_id,p.gpo_id,p.user_filter_group_id,p.computer_target_ou_id,true,p.directory_config_status,p.directory_config_error,p.position_id,
 COALESCE(r.published_by,'migration:0320'),'Imported existing Position mapping without changing Directory state',p.updated_at
FROM position_gpo_policies p LEFT JOIN position_gpo_revisions r ON r.company_id=p.company_id AND r.position_id=p.position_id AND r.revision=p.revision
WHERE p.connection_id IS NOT NULL;

-- Compatibility writes still own one legacy direct rule; additional direct rules are independent.
CREATE FUNCTION sync_legacy_position_gpo_rule() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE actor_id text; reason_text text;
BEGIN
 SELECT published_by,reason INTO actor_id,reason_text FROM position_gpo_revisions WHERE company_id=NEW.company_id AND position_id=NEW.position_id AND revision=NEW.revision;
 IF NEW.connection_id IS NULL THEN
  UPDATE organization_gpo_rules SET active=false,version=GREATEST(version+1,NEW.version),revision=GREATEST(revision+1,NEW.revision),last_actor_id=COALESCE(actor_id,last_actor_id),last_reason=COALESCE(reason_text,'Legacy Position mapping removed'),updated_at=NEW.updated_at WHERE company_id=NEW.company_id AND legacy_position_id=NEW.position_id AND active;
 ELSE
  INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,active,directory_config_status,directory_config_error,legacy_position_id,last_actor_id,last_reason,updated_at)
  VALUES('legacy-position:'||NEW.position_id,NEW.company_id,'position',NEW.position_id,GREATEST(NEW.version,1),GREATEST(NEW.revision,1),NEW.connection_id,NEW.gpo_id,NEW.user_filter_group_id,NEW.computer_target_ou_id,true,NEW.directory_config_status,NEW.directory_config_error,NEW.position_id,COALESCE(actor_id,'legacy-position-api'),COALESCE(reason_text,'Legacy Position mapping updated'),NEW.updated_at)
  ON CONFLICT(id) DO UPDATE SET version=excluded.version,revision=excluded.revision,connection_id=excluded.connection_id,gpo_id=excluded.gpo_id,user_filter_group_id=excluded.user_filter_group_id,computer_target_ou_id=excluded.computer_target_ou_id,active=true,directory_config_status=excluded.directory_config_status,directory_config_error=excluded.directory_config_error,last_actor_id=excluded.last_actor_id,last_reason=excluded.last_reason,updated_at=excluded.updated_at;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER sync_legacy_position_gpo_rule AFTER INSERT OR UPDATE OF version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,directory_config_error ON position_gpo_policies FOR EACH ROW EXECUTE FUNCTION sync_legacy_position_gpo_rule();

-- The reconciler resolves every source against current placement and dates; the
-- old Position-only trigger cannot represent inherited or multiple policies.
DROP TRIGGER queue_position_gpo_change ON hr_engagements;
ALTER TABLE position_gpo_jobs ADD COLUMN rule_id text;
ALTER TABLE position_gpo_jobs ADD COLUMN source_id text;
UPDATE position_gpo_jobs SET rule_id='legacy-position:'||position_id,source_id='position-gpo:'||engagement_id WHERE engagement_id IS NOT NULL;
DROP INDEX position_gpo_account_jobs_active;
CREATE UNIQUE INDEX position_gpo_rule_jobs_active ON position_gpo_jobs
 (company_id,position_id,source_id,account_id,connection_id,filter_group_id,rule_id,revision,desired_action)
 WHERE engagement_id IS NOT NULL AND account_id IS NOT NULL AND status IN ('queued','running','retry','blocked');

CREATE TABLE organization_gpo_membership_claims (
 connection_id text NOT NULL, group_id text NOT NULL, member_id text NOT NULL, source_id text NOT NULL,
 rule_id text NOT NULL, company_id text NOT NULL, position_id text NOT NULL, engagement_id text NOT NULL,
 person_id text NOT NULL, revision integer NOT NULL, engagement_version integer NOT NULL,
 gpo_id text NOT NULL, computer_target_ou_id text NOT NULL,
 PRIMARY KEY(connection_id,group_id,member_id,source_id)
);
INSERT INTO organization_gpo_membership_claims
SELECT s.connection_id,s.group_id,s.member_id,s.source_id,COALESCE(j.rule_id,'legacy-position:'||e.position_id,'legacy-orphan:'||e.id),e.company_id,COALESCE(j.position_id,e.position_id,'unassigned'),e.id,e.person_id,COALESCE(j.revision,p.revision,0),e.version,COALESCE(j.gpo_id,p.gpo_id,'unmapped'),COALESCE(j.computer_target_ou_id,p.computer_target_ou_id,'unmapped')
FROM people_directory_membership_sources s JOIN hr_engagements e ON s.source_id='position-gpo:'||e.id
LEFT JOIN position_gpo_policies p ON p.company_id=e.company_id AND p.position_id=e.position_id
LEFT JOIN LATERAL (SELECT * FROM position_gpo_jobs x WHERE x.connection_id=s.connection_id AND x.filter_group_id=s.group_id AND x.account_id=s.member_id AND x.engagement_id=e.id ORDER BY x.updated_at DESC LIMIT 1) j ON true;

DO $$ DECLARE migration_role text:=current_user; app_role text; jobs_role text; BEGIN
 IF migration_role !~ '_migration$' THEN RETURN; END IF;
 app_role:=regexp_replace(migration_role,'_migration$','_app'); jobs_role:=regexp_replace(migration_role,'_migration$','_jobs');
 EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON organization_gpo_rules,organization_gpo_membership_claims TO %I,%I',app_role,jobs_role);
 EXECUTE format('GRANT SELECT,INSERT,DELETE ON organization_gpo_previews TO %I,%I',app_role,jobs_role);
 EXECUTE format('GRANT SELECT,INSERT ON organization_gpo_rule_revisions TO %I,%I',app_role,jobs_role);
END $$;
