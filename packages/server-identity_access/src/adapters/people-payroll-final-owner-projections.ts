import 'server-only';
import { assignmentsProjectionCompact, iamMembersProjection, iamIdentitiesProjection, peopleDirectoryOperationsProjection, organizationGpoRulesProjection } from './assignment-projection-source';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "calendar_lifecycle_facts_people_directory_operations_o_from": `FROM (${peopleDirectoryOperationsProjection}) o`,
  "directory_operation_facts_iam_members_m_join": `JOIN (${iamMembersProjection}) m`,
  "directory_operation_facts_iam_identities_i_from": `FROM (${iamIdentitiesProjection}) i`,
  "executive_report_sources_assignments_a_from": `FROM (${assignmentsProjectionCompact}) a`,
  "executive_report_sources_assignments_r_from": `FROM (${assignmentsProjectionCompact}) r`,
  "executive_report_sources_assignments_from": `FROM (${assignmentsProjectionCompact}) assignments`,
  "executive_report_sources_assignments_a_join": `JOIN (${assignmentsProjectionCompact}) a`,
  "executive_report_sources_iam_roles_r_from": "FROM (SELECT \"company_id\",\"id\",\"name\",\"built_in\",\"published_revision\",\"status\" FROM iam_roles) r",
  "executive_report_sources_iam_role_revisions_rv_join": "JOIN (SELECT \"company_id\",\"role_id\",\"revision\",\"permissions_json\",\"grantable_json\",\"scopes_json\",\"created_by\",\"created_at\" FROM iam_role_revisions) rv",
  "gpo_sources_iam_members_m_from": `FROM (${iamMembersProjection}) m`,
  "gpo_sources_iam_identities_i_join": `JOIN (${iamIdentitiesProjection}) i`,
  "organization_person_links_iam_members_m_from": `FROM (${iamMembersProjection}) m`,
  "people_lifecycle_position_gpo_jobs_j_from": "FROM (SELECT \"id\",\"company_id\",\"position_id\",\"person_id\",\"revision\",\"engagement_version\",\"connection_id\",\"gpo_id\",\"filter_group_id\",\"computer_target_ou_id\",\"desired_action\",\"status\",\"version\",\"attempts\",\"last_error\",\"directory_observed_at\",\"endpoint_observed_at\",\"created_at\",\"updated_at\",\"engagement_id\",\"account_id\",\"rule_id\",\"source_id\" FROM position_gpo_jobs) j",
  "people_lifecycle_organization_gpo_rules_r_join": `JOIN (${organizationGpoRulesProjection}) r`,
  "people_lifecycle_people_directory_operations_from": `FROM (${peopleDirectoryOperationsProjection}) people_directory_operations`,
  "profile_work_iam_members_m_from": `FROM (${iamMembersProjection}) m`,
  "people_directory_repository_iam_members_m_from": `FROM (${iamMembersProjection}) m`
} as const;}
