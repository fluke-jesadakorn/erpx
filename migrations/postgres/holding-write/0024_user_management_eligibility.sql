CREATE OR REPLACE FUNCTION um_candidate_eligible(candidate_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM um_candidates c JOIN um_attachments a ON a.company_id=c.company_id AND a.id=c.attachment_id JOIN um_policies p ON p.company_id=a.company_id AND p.id=a.definition->>'policyId'
 JOIN hr_people h ON h.company_id=c.company_id AND h.id=c.person_id AND h.user_id=c.user_id AND h.status='active' AND h.merged_into_id IS NULL
 JOIN iam_roles r ON r.company_id IN ('*',c.company_id) AND r.id=c.definition->>'roleId' AND r.status='active'
 WHERE c.id=candidate_key AND c.status IN ('pending','active','rejected','blocked') AND a.version=c.attachment_version AND p.version=c.policy_version
 AND (c.status<>'active' OR EXISTS(SELECT 1 FROM assignments g WHERE g.id=c.assignment_id AND g.revoked_at IS NULL AND g.valid_until>at_time))
 AND r.published_revision=(c.definition->>'expectedRoleRevision')::integer AND (c.definition->>'validUntil')::bigint>at_time
 AND NOT EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=c.company_id AND m.user_id=c.user_id AND m.status<>'active')
 AND um_attachment_matches(c.company_id,c.person_id,c.attachment_id)); $$;
CREATE OR REPLACE FUNCTION um_invalidate(company_key TEXT) RETURNS void LANGUAGE plpgsql AS $$
 DECLARE item RECORD; stamp BIGINT:=floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
 BEGIN
 FOR item IN SELECT * FROM um_candidates c WHERE c.company_id=company_key AND c.status IN ('pending','active','rejected','blocked') AND NOT um_candidate_eligible(c.id,stamp) FOR UPDATE LOOP
 UPDATE um_candidates SET status='revoked' WHERE id=item.id;
 UPDATE iam_reviews SET status='failed',version=version+1 WHERE id=item.review_id AND status='pending';
 UPDATE assignments SET revoked_at=stamp,version=version+1 WHERE id=item.assignment_id AND revoked_at IS NULL;
 UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=company_key AND assignment_id=item.assignment_id;
 INSERT INTO iam_audit VALUES('um-'||gen_random_uuid()::text,company_key,'system:user-management',COALESCE(item.assignment_id,(SELECT parent_id FROM um_attachments WHERE company_id=company_key AND id=item.attachment_id)),'policy.eligibility_lost',item.id,jsonb_build_object('assignmentId',item.assignment_id,'personId',item.person_id),stamp);
 END LOOP;
 END; $$;
CREATE TRIGGER um_principal_changed AFTER UPDATE ON iam_members FOR EACH ROW EXECUTE FUNCTION um_changed();
