-- A selectable, reviewable author role; this migration grants no personal assignments.
INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)
VALUES('*','executive_author','Executive Author',true,1,'active');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
VALUES('*','executive_author',1,'["executive.access","executive.report.create","executive.report.edit","executive.report.share","ai.context.cross","iam.self","records.read","self.tasks","self.requests","self.roster","leave.self"]'::jsonb,'[]'::jsonb,'["holding","branch","department"]'::jsonb,'system:executive-canvas',floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
-- Draft recipe only. Existing lifecycle publication and independent grant review remain required.
INSERT INTO people_access_packages(company_id,id,version,published_revision,definition)
SELECT id,'executive-author',1,0,jsonb_build_object('name','Executive Author','account','local','connectionId',NULL,
 'grants',jsonb_build_array(jsonb_build_object('roleId','executive_author','expectedRoleRevision',1,'scopeKind','holding','scopeNodeId',NULL,'validUntil',floor(extract(epoch FROM clock_timestamp())*1000)::bigint+31536000000)),
 'groupIds','[]'::jsonb,'directoryGroupIds','[]'::jsonb,'windowsPolicies','[]'::jsonb,'tasks','[]'::jsonb)
FROM organization_nodes WHERE kind='company' AND status='active'
ON CONFLICT(company_id,id) DO NOTHING;
