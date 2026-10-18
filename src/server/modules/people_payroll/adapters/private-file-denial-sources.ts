import 'server-only';
/** IAM's existing Company/worker foundation remains independent of a People
 * purchase. This private hash input cannot authorize or expose HR records. */
export const privateFileDenialPeopleWitness=`jsonb_build_object(
 'people',(SELECT jsonb_agg(to_jsonb(person) ORDER BY person.id) FROM hr_people person WHERE person.company_id=denial_scope.company_id),
 'engagements',(SELECT jsonb_agg(to_jsonb(engagement) ORDER BY engagement.id) FROM hr_engagements engagement WHERE engagement.company_id=denial_scope.company_id),
 'cutoffs',(SELECT jsonb_agg(to_jsonb(cutoff) ORDER BY to_jsonb(cutoff)::text) FROM people_access_cutoffs cutoff WHERE cutoff.company_id=denial_scope.company_id),
 'cases',(SELECT jsonb_agg(to_jsonb(lifecycle) ORDER BY lifecycle.id) FROM people_lifecycle_cases lifecycle WHERE lifecycle.company_id=denial_scope.company_id),
 'grants',(SELECT jsonb_agg(to_jsonb(grant_row) ORDER BY to_jsonb(grant_row)::text) FROM people_lifecycle_grants grant_row WHERE grant_row.company_id=denial_scope.company_id),
 'packageRevisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY to_jsonb(revision)::text) FROM people_package_revisions revision WHERE revision.company_id=denial_scope.company_id),
 'appliedPackages',(SELECT jsonb_agg(to_jsonb(package_row) ORDER BY to_jsonb(package_row)::text) FROM people_applied_packages package_row WHERE package_row.company_id=denial_scope.company_id),
 'provisionOperations',(SELECT jsonb_agg(to_jsonb(operation) ORDER BY to_jsonb(operation)::text) FROM people_provision_operations operation WHERE operation.company_id=denial_scope.company_id),
 'provisionGrants',(SELECT jsonb_agg(to_jsonb(provision) ORDER BY to_jsonb(provision)::text) FROM people_provision_grants provision WHERE EXISTS(SELECT 1 FROM people_provision_operations operation WHERE operation.id=provision.operation_id AND operation.company_id=denial_scope.company_id)))`;
