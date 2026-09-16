-- Company-owned user-management metadata. External directory records remain untouched.
CREATE TABLE um_attributes(company_id TEXT NOT NULL,id TEXT NOT NULL,version INTEGER NOT NULL,definition JSONB NOT NULL,PRIMARY KEY(company_id,id));
CREATE TABLE um_values(company_id TEXT NOT NULL,target TEXT NOT NULL CHECK(target IN ('person','group','unit')),object_id TEXT NOT NULL,version INTEGER NOT NULL,definition JSONB NOT NULL,PRIMARY KEY(company_id,target,object_id));
CREATE TABLE um_groups(company_id TEXT NOT NULL,id TEXT NOT NULL,version INTEGER NOT NULL,definition JSONB NOT NULL,PRIMARY KEY(company_id,id));
CREATE TABLE um_memberships(company_id TEXT NOT NULL,group_id TEXT NOT NULL,person_id TEXT NOT NULL,PRIMARY KEY(company_id,group_id,person_id),FOREIGN KEY(company_id,group_id) REFERENCES um_groups(company_id,id));
CREATE TABLE um_policies(company_id TEXT NOT NULL,id TEXT NOT NULL,version INTEGER NOT NULL,definition JSONB NOT NULL,PRIMARY KEY(company_id,id));
CREATE TABLE um_attachments(company_id TEXT NOT NULL,id TEXT NOT NULL,version INTEGER NOT NULL,definition JSONB NOT NULL,author_id TEXT NOT NULL,parent_id TEXT NOT NULL REFERENCES assignments(id),PRIMARY KEY(company_id,id));
CREATE TABLE um_history(id BIGSERIAL PRIMARY KEY,company_id TEXT NOT NULL,entity TEXT NOT NULL,object_id TEXT NOT NULL,version INTEGER NOT NULL,definition JSONB NOT NULL,created_at BIGINT NOT NULL);
CREATE TRIGGER um_history_immutable BEFORE UPDATE OR DELETE ON um_history FOR EACH ROW EXECUTE FUNCTION iam_immutable();
CREATE TABLE um_candidates(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,person_id TEXT NOT NULL,user_id TEXT NOT NULL,attachment_id TEXT NOT NULL,attachment_version INTEGER NOT NULL,policy_version INTEGER NOT NULL,grant_index INTEGER NOT NULL,definition JSONB NOT NULL,status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','revoked','rejected','blocked')),review_id TEXT REFERENCES iam_reviews(id),assignment_id TEXT REFERENCES assignments(id),created_at BIGINT NOT NULL,FOREIGN KEY(company_id,attachment_id) REFERENCES um_attachments(company_id,id));
CREATE UNIQUE INDEX um_candidate_current ON um_candidates(company_id,person_id,attachment_id,grant_index) WHERE status IN ('pending','active');
CREATE INDEX um_candidates_company ON um_candidates(company_id,status);
CREATE INDEX um_memberships_person ON um_memberships(company_id,person_id);

CREATE FUNCTION um_descendant(company_key TEXT,node_key TEXT,root_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE path AS (
 SELECT id,parent_id,ARRAY[id] visited FROM organization_nodes WHERE company_id=company_key AND id=node_key AND status='active'
 UNION ALL SELECT n.id,n.parent_id,p.visited||n.id FROM organization_nodes n JOIN path p ON n.id=p.parent_id WHERE n.company_id=company_key AND n.status='active' AND NOT n.id=ANY(p.visited) AND cardinality(p.visited)<64)
 SELECT root_key IS NULL OR EXISTS(SELECT 1 FROM path WHERE id=root_key); $$;
CREATE FUNCTION um_person_in_unit(company_key TEXT,person_key TEXT,node_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM hr_people p WHERE p.company_id=company_key AND p.id=person_key AND p.status='active' AND p.merged_into_id IS NULL AND
 (node_key IS NULL OR EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=node_key AND n.company_id=company_key AND n.kind='company' AND n.status='active') OR EXISTS(SELECT 1 FROM hr_engagements e WHERE e.company_id=company_key AND e.person_id=person_key AND e.status='active' AND um_descendant(company_key,COALESCE(e.department_id,e.branch_id),node_key)))); $$;
CREATE FUNCTION um_person_facts(company_key TEXT,person_key TEXT) RETURNS JSONB LANGUAGE sql STABLE AS $$
 SELECT COALESCE((SELECT jsonb_build_object('worker_type',e.worker_type,'employment_type',e.employment_type,'job_title',e.job_title,'branch_id',e.branch_id,'department_id',e.department_id,'start_date',e.start_date) FROM hr_engagements e WHERE e.company_id=company_key AND e.person_id=person_key AND e.status='active' ORDER BY e.id LIMIT 1),'{}'::jsonb)
 || COALESCE((SELECT jsonb_object_agg('custom:'||x.key,x.value) FROM um_values v CROSS JOIN LATERAL jsonb_each(v.definition) x JOIN um_attributes a ON a.company_id=v.company_id AND a.id=x.key AND NOT (a.definition->>'archived')::boolean WHERE v.company_id=company_key AND v.target='person' AND v.object_id=person_key),'{}'::jsonb); $$;
CREATE FUNCTION um_rule_matches(rule JSONB,facts JSONB) RETURNS BOOLEAN LANGUAGE plpgsql IMMUTABLE AS $$
 DECLARE condition JSONB; actual JSONB; wanted JSONB; empty_value BOOLEAN; matches BOOLEAN; result BOOLEAN;
 BEGIN
 IF rule IS NULL OR jsonb_array_length(rule->'conditions')=0 THEN RETURN false; END IF;
 result := rule->>'mode'='all';
 FOR condition IN SELECT * FROM jsonb_array_elements(rule->'conditions') LOOP
 actual := facts->(condition->>'field'); wanted := condition->'value';
 empty_value := actual IS NULL OR actual IN ('null'::jsonb,'""'::jsonb,'[]'::jsonb);
 matches := CASE WHEN condition->>'op'='empty' THEN empty_value WHEN empty_value THEN false
 WHEN condition->>'op'='not-empty' THEN true
 WHEN condition->>'op'='eq' THEN actual=wanted WHEN condition->>'op'='neq' THEN actual<>wanted
 WHEN condition->>'op'='contains' THEN jsonb_typeof(actual)='array' AND actual @> jsonb_build_array(wanted)
 WHEN jsonb_typeof(actual)<>jsonb_typeof(wanted) THEN false
 WHEN condition->>'op'='gt' THEN actual>wanted WHEN condition->>'op'='gte' THEN actual>=wanted
 WHEN condition->>'op'='lt' THEN actual<wanted WHEN condition->>'op'='lte' THEN actual<=wanted ELSE false END;
 IF rule->>'mode'='all' THEN result:=result AND COALESCE(matches,false); ELSE result:=result OR COALESCE(matches,false); END IF;
 END LOOP;
 RETURN result;
 END; $$;
CREATE FUNCTION um_group_member(company_key TEXT,person_key TEXT,group_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE children AS (
 SELECT g.id,g.definition,ARRAY[g.id] visited FROM um_groups g WHERE g.company_id=company_key AND g.id=group_key AND NOT (g.definition->>'archived')::boolean AND um_person_in_unit(company_key,person_key,g.definition->>'scopeNodeId')
 UNION ALL SELECT g.id,g.definition,c.visited||g.id FROM um_groups g JOIN children c ON g.definition->>'parentId'=c.id WHERE g.company_id=company_key AND NOT (g.definition->>'archived')::boolean AND NOT g.id=ANY(c.visited) AND cardinality(c.visited)<32 AND um_person_in_unit(company_key,person_key,g.definition->>'scopeNodeId'))
 SELECT EXISTS(SELECT 1 FROM children c WHERE CASE WHEN c.definition->>'kind'='automatic' THEN um_rule_matches(c.definition->'rule',um_person_facts(company_key,person_key)) ELSE EXISTS(SELECT 1 FROM um_memberships m WHERE m.company_id=company_key AND m.group_id=c.id AND m.person_id=person_key) END); $$;
CREATE FUNCTION um_attachment_matches(company_key TEXT,person_key TEXT,attachment_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM um_attachments a JOIN um_policies p ON p.company_id=a.company_id AND p.id=a.definition->>'policyId' WHERE a.company_id=company_key AND a.id=attachment_key AND NOT (a.definition->>'archived')::boolean AND NOT (p.definition->>'archived')::boolean
 AND CASE WHEN a.definition->>'target'='group' THEN um_group_member(company_key,person_key,a.definition->>'targetId') ELSE um_person_in_unit(company_key,person_key,a.definition->>'targetId') END
 AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements_text(a.definition->'exclusions') x WHERE CASE WHEN a.definition->>'target'='group' THEN um_group_member(company_key,person_key,x) ELSE um_person_in_unit(company_key,person_key,x) END)); $$;
CREATE FUNCTION um_candidate_eligible(candidate_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM um_candidates c JOIN um_attachments a ON a.company_id=c.company_id AND a.id=c.attachment_id JOIN um_policies p ON p.company_id=a.company_id AND p.id=a.definition->>'policyId'
 JOIN hr_people h ON h.company_id=c.company_id AND h.id=c.person_id AND h.user_id=c.user_id AND h.status='active' AND h.merged_into_id IS NULL
 JOIN iam_roles r ON r.company_id IN ('*',c.company_id) AND r.id=c.definition->>'roleId' AND r.status='active'
 WHERE c.id=candidate_key AND c.status IN ('pending','active') AND a.version=c.attachment_version AND p.version=c.policy_version
 AND r.published_revision=(c.definition->>'expectedRoleRevision')::integer AND (c.definition->>'validUntil')::bigint>at_time AND um_attachment_matches(c.company_id,c.person_id,c.attachment_id)); $$;
ALTER TABLE assignments DROP CONSTRAINT assignments_source_check;
ALTER TABLE assignments ADD CONSTRAINT assignments_source_check CHECK(source IN ('legacy','manual','directory','emergency','policy'));
CREATE OR REPLACE FUNCTION iam_assignment_active(grant_id TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE chain AS (
 SELECT a.id,a.user_id,a.role,a.company_id,a.parent_id,a.revoked_at,a.valid_from,a.valid_until,a.source,a.source_id,ARRAY[a.id] AS path,false AS cycle FROM assignments a WHERE a.id=grant_id
 UNION ALL SELECT p.id,p.user_id,p.role,p.company_id,p.parent_id,p.revoked_at,p.valid_from,p.valid_until,p.source,p.source_id,c.path||p.id,p.id=ANY(c.path) FROM assignments p JOIN chain c ON p.id=c.parent_id WHERE NOT c.cycle AND cardinality(c.path)<32)
 SELECT EXISTS(SELECT 1 FROM chain) AND NOT EXISTS(SELECT 1 FROM chain WHERE revoked_at IS NOT NULL OR valid_from>at_time OR valid_until<=at_time OR cycle OR cardinality(path)>=32 OR NOT iam_principal_active(user_id,company_id,at_time) OR source='policy' AND NOT um_candidate_eligible(source_id,at_time))
 AND EXISTS(SELECT 1 FROM chain WHERE parent_id IS NULL)
 AND NOT EXISTS(SELECT 1 FROM chain ch JOIN chain p ON p.id=ch.parent_id
 LEFT JOIN iam_roles cr ON cr.id=ch.role AND cr.company_id IN ('*',ch.company_id) AND cr.status='active'
 LEFT JOIN iam_role_revisions cv ON cv.company_id=cr.company_id AND cv.role_id=cr.id AND cv.revision=cr.published_revision
 LEFT JOIN iam_roles pr ON pr.id=p.role AND pr.company_id IN ('*',p.company_id) AND pr.status='active'
 LEFT JOIN iam_role_revisions pv ON pv.company_id=pr.company_id AND pv.role_id=pr.id AND pv.revision=pr.published_revision
 WHERE ch.company_id!=p.company_id OR ch.valid_until>p.valid_until OR cv.revision IS NULL OR pv.revision IS NULL OR NOT ((cv.permissions_json||cv.grantable_json)<@pv.grantable_json)); $$;

-- Persist loss of eligibility before a later rejoin can restore the same grant.
CREATE FUNCTION um_invalidate(company_key TEXT) RETURNS void LANGUAGE plpgsql AS $$
 DECLARE item RECORD; stamp BIGINT:=floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
 BEGIN
 FOR item IN SELECT * FROM um_candidates c WHERE c.company_id=company_key AND c.status IN ('pending','active') AND NOT um_candidate_eligible(c.id,stamp) FOR UPDATE LOOP
 UPDATE um_candidates SET status='revoked' WHERE id=item.id;
 UPDATE iam_reviews SET status='failed',version=version+1 WHERE id=item.review_id AND status='pending';
 UPDATE assignments SET revoked_at=stamp,version=version+1 WHERE id=item.assignment_id AND revoked_at IS NULL;
 UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=company_key AND assignment_id=item.assignment_id;
 INSERT INTO iam_audit VALUES('um-'||gen_random_uuid()::text,company_key,'system:user-management',NULL,'policy.eligibility_lost',item.id,jsonb_build_object('assignmentId',item.assignment_id,'personId',item.person_id),stamp);
 END LOOP;
 END; $$;
CREATE FUNCTION um_changed() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN
 PERFORM um_invalidate(CASE WHEN TG_OP='DELETE' THEN OLD.company_id ELSE NEW.company_id END);
 RETURN NULL;
 END; $$;
CREATE TRIGGER um_people_changed AFTER UPDATE OR DELETE ON hr_people FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_engagement_changed AFTER INSERT OR UPDATE OR DELETE ON hr_engagements FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_membership_changed AFTER INSERT OR UPDATE OR DELETE ON um_memberships FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_group_changed AFTER UPDATE ON um_groups FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_attribute_changed AFTER UPDATE ON um_attributes FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_value_changed AFTER INSERT OR UPDATE ON um_values FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_policy_changed AFTER UPDATE ON um_policies FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_attachment_changed AFTER UPDATE ON um_attachments FOR EACH ROW EXECUTE FUNCTION um_changed();
CREATE TRIGGER um_unit_changed AFTER UPDATE OR DELETE ON organization_nodes FOR EACH ROW EXECUTE FUNCTION um_changed();
