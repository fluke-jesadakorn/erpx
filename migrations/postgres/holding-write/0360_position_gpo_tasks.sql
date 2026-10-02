-- Position GPO handoff. This migration only changes local rows; it never calls a Directory provider.
CREATE TABLE position_gpo_tasks (
 company_id text NOT NULL, position_id text NOT NULL,
 version integer NOT NULL DEFAULT 1 CHECK(version>0),
 disposition text CHECK(disposition IN ('not_required')),
 reason text, updated_at bigint NOT NULL,
 PRIMARY KEY(company_id,position_id)
);

INSERT INTO position_gpo_tasks(company_id,position_id,updated_at)
SELECT company_id,id,updated_at FROM organization_positions WHERE active=1;

CREATE FUNCTION maintain_position_gpo_task() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
  IF NEW.active=1 THEN INSERT INTO position_gpo_tasks(company_id,position_id,updated_at)
   VALUES(NEW.company_id,NEW.id,NEW.updated_at) ON CONFLICT DO NOTHING; END IF;
 ELSIF NEW.active=1 AND (OLD.active IS DISTINCT FROM NEW.active OR OLD.title IS DISTINCT FROM NEW.title
   OR OLD.branch_id IS DISTINCT FROM NEW.branch_id OR OLD.department_id IS DISTINCT FROM NEW.department_id
   OR OLD.level_id IS DISTINCT FROM NEW.level_id OR OLD.reports_to_position_id IS DISTINCT FROM NEW.reports_to_position_id) THEN
  INSERT INTO position_gpo_tasks(company_id,position_id,updated_at)
   VALUES(NEW.company_id,NEW.id,NEW.updated_at)
  ON CONFLICT(company_id,position_id) DO UPDATE SET version=position_gpo_tasks.version+1,
    disposition=NULL,reason=NULL,updated_at=excluded.updated_at;
  UPDATE organization_gpo_rules SET directory_config_status='pending',directory_config_error=NULL
   WHERE company_id=NEW.company_id AND target_kind='position' AND target_id=NEW.id AND active;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER maintain_position_gpo_task AFTER INSERT OR UPDATE ON organization_positions
 FOR EACH ROW EXECUTE FUNCTION maintain_position_gpo_task();

-- Expand every active Level/Department rule to the Positions it covered at cutover.
-- Old rule rows and revision snapshots remain as immutable provenance.
CREATE TEMP TABLE position_gpo_rule_conversion ON COMMIT DROP AS
 SELECT r.id old_rule_id,p.id position_id,
  'positionized:' || md5(r.id || ':' || p.id) new_rule_id
 FROM organization_gpo_rules r JOIN organization_positions p ON p.company_id=r.company_id AND p.active=1
 WHERE r.active AND ((r.target_kind='level' AND r.target_id=p.level_id)
   OR (r.target_kind='department' AND r.target_id=p.department_id));

INSERT INTO organization_gpo_rules(id,company_id,target_kind,target_id,version,revision,
 connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,active,directory_config_status,
 directory_config_error,legacy_position_id,last_actor_id,last_reason,updated_at)
SELECT m.new_rule_id,r.company_id,'position',m.position_id,r.version,r.revision,
 r.connection_id,r.gpo_id,r.user_filter_group_id,r.computer_target_ou_id,true,r.directory_config_status,
 r.directory_config_error,NULL,r.last_actor_id,'Converted from '||r.target_kind||' rule '||r.id||': '||r.last_reason,r.updated_at
FROM position_gpo_rule_conversion m JOIN organization_gpo_rules r ON r.id=m.old_rule_id;

-- Re-key current source claims and the local membership ledger together. This
-- preserves ownership of already applied memberships without reapplying them.
CREATE TEMP TABLE position_gpo_claim_conversion ON COMMIT DROP AS
 SELECT c.connection_id,c.group_id,c.member_id,c.source_id old_source_id,c.engagement_id,
  m.new_rule_id,'gpo-rule:['||to_json(m.new_rule_id)::text||','||to_json(c.engagement_id)::text||']' new_source_id
 FROM organization_gpo_membership_claims c
 JOIN position_gpo_rule_conversion m ON m.old_rule_id=c.rule_id AND m.position_id=c.position_id;

INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at)
SELECT s.connection_id,s.group_id,s.member_id,m.new_source_id,s.created_at
FROM position_gpo_claim_conversion m JOIN people_directory_membership_sources s
 ON s.connection_id=m.connection_id AND s.group_id=m.group_id AND s.member_id=m.member_id AND s.source_id=m.old_source_id
ON CONFLICT DO NOTHING;

INSERT INTO organization_gpo_membership_claims(connection_id,group_id,member_id,source_id,rule_id,
 company_id,position_id,engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id)
SELECT c.connection_id,c.group_id,c.member_id,m.new_source_id,m.new_rule_id,
 c.company_id,c.position_id,c.engagement_id,c.person_id,c.revision,c.engagement_version,c.gpo_id,c.computer_target_ou_id
FROM position_gpo_claim_conversion m JOIN organization_gpo_membership_claims c
 ON c.connection_id=m.connection_id AND c.group_id=m.group_id AND c.member_id=m.member_id AND c.source_id=m.old_source_id
ON CONFLICT DO NOTHING;

DELETE FROM organization_gpo_membership_claims c USING position_gpo_claim_conversion m
WHERE c.connection_id=m.connection_id AND c.group_id=m.group_id AND c.member_id=m.member_id AND c.source_id=m.old_source_id;
DELETE FROM people_directory_membership_sources s USING position_gpo_claim_conversion m
WHERE s.connection_id=m.connection_id AND s.group_id=m.group_id AND s.member_id=m.member_id AND s.source_id=m.old_source_id;

UPDATE organization_gpo_rules SET active=false,version=version+1,revision=revision+1,
 last_reason='Converted to direct Position rules',updated_at=(extract(epoch FROM clock_timestamp())*1000)::bigint
WHERE active AND target_kind IN ('level','department');

CREATE FUNCTION reopen_position_gpo_task_for_rule() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.target_kind='position' AND NEW.active AND (TG_OP='INSERT' OR OLD.version IS DISTINCT FROM NEW.version) THEN
  UPDATE position_gpo_tasks SET disposition=NULL,reason=NULL,version=version+1,updated_at=NEW.updated_at
   WHERE company_id=NEW.company_id AND position_id=NEW.target_id AND disposition IS NOT NULL;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER reopen_position_gpo_task_for_rule AFTER INSERT OR UPDATE ON organization_gpo_rules
 FOR EACH ROW EXECUTE FUNCTION reopen_position_gpo_task_for_rule();

DO $$ DECLARE migration_role text:=current_user; BEGIN
 IF migration_role ~ '_migration$' THEN
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON position_gpo_tasks TO %I,%I',
   regexp_replace(migration_role,'_migration$','_app'),regexp_replace(migration_role,'_migration$','_jobs'));
 END IF;
END $$;

INSERT INTO iam_permissions(id,definition) VALUES (
 'people.custom_attributes.write',
 '{"id":"people.custom_attributes.write","label":"Edit person custom attribute values","module":"people_payroll","roles":["holding_owner","hr_admin","people_manager","it_admin"],"conditions":["Person in selected Company; field access rules"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch","department"]}'::jsonb
) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

WITH grants(role_id,permission) AS (VALUES
 ('holding_owner','people.custom_attributes.write'),('hr_admin','people.custom_attributes.write'),
 ('people_manager','people.custom_attributes.write'),('it_admin','people.custom_attributes.write')
), selected AS (
 SELECT r.company_id,r.id role_id,r.published_revision,v.permissions_json,v.grantable_json,v.scopes_json,g.permission
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 JOIN grants g ON g.role_id=r.id WHERE r.company_id='*'
), revisions AS (
 INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
 SELECT company_id,role_id,published_revision+1,permissions_json || to_jsonb(permission),
  CASE WHEN role_id='holding_owner' THEN grantable_json || to_jsonb(permission) ELSE grantable_json END,
  scopes_json,'system:position-gpo-handoff',(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM selected WHERE NOT permissions_json ? permission
 RETURNING company_id,role_id,revision
)
UPDATE iam_roles r SET published_revision=v.revision FROM revisions v
WHERE r.company_id=v.company_id AND r.id=v.role_id;
