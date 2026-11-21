SELECT EXISTS(SELECT 1 FROM company_release_acceptances release
    JOIN company_legal_release_revisions revision ON revision.company_id=release.company_id
    JOIN (SELECT "company_id","status","version" FROM company_business_acceptance) setup ON setup.company_id=release.company_id
    WHERE release.company_id=$1 AND release.revoked_at IS NULL
    AND release.scope_version='legal-th-operations-v1' AND release.software_version='synthetic-native-release'
    AND release.configuration_revision=revision.revision AND setup.status='accepted' AND setup.version=release.setup_version
    AND release.id=(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=release.company_id AND latest.scope_version=release.scope_version ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1)
    
    AND NOT EXISTS(SELECT 1 FROM (VALUES ('legal_workflows','legal.manage'),('legal_content','legal.manage'),('legal_access','organization.manage'),('legal_documents','legal.manage'),('legal_ai','legal.manage'),('legal_release','organization.manage'),('legal_operations','organization.manage')) AS required(id,permission) WHERE NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN (SELECT "id","user_id","role","modules_json","company_id","branch_id","valid_from","valid_until","revoked_at","version","scope_kind","scope_node_id","parent_id","source","source_id" FROM assignments) reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id)))) AS legacy_allowed;
