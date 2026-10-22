import "server-only";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  assignments: "SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments",
  iam_members: "SELECT \"company_id\",\"user_id\",\"principal_id\",\"person_id\",\"status\",\"login_policy\",\"connection_id\",\"identity_id\",\"version\",\"emergency_until\" FROM iam_members",
  organization_gpo_membership_claims: "SELECT \"connection_id\",\"group_id\",\"member_id\",\"source_id\",\"rule_id\",\"company_id\",\"position_id\",\"engagement_id\",\"person_id\",\"revision\",\"engagement_version\",\"gpo_id\",\"computer_target_ou_id\" FROM organization_gpo_membership_claims",
  organization_gpo_rules: "SELECT \"id\",\"company_id\",\"target_kind\",\"target_id\",\"version\",\"revision\",\"connection_id\",\"gpo_id\",\"user_filter_group_id\",\"computer_target_ou_id\",\"active\",\"directory_config_status\",\"directory_config_error\",\"legacy_position_id\",\"last_actor_id\",\"last_reason\",\"updated_at\" FROM organization_gpo_rules",
  people_directory_membership_sources: "SELECT \"connection_id\",\"group_id\",\"member_id\",\"source_id\",\"application_id\",\"created_at\" FROM people_directory_membership_sources",
  position_gpo_policies: "SELECT \"company_id\",\"position_id\",\"version\",\"revision\",\"connection_id\",\"gpo_id\",\"user_filter_group_id\",\"computer_target_ou_id\",\"directory_config_status\",\"directory_config_error\",\"updated_at\" FROM position_gpo_policies",
} as const);
