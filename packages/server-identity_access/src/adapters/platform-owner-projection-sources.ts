import "server-only";
import { assignmentsProjectionCompact, iamMembersProjection } from './assignment-projection-source';
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  assignments: assignmentsProjectionCompact,
  iam_members: iamMembersProjection,
  organization_gpo_membership_claims: "SELECT \"connection_id\",\"group_id\",\"member_id\",\"source_id\",\"rule_id\",\"company_id\",\"position_id\",\"engagement_id\",\"person_id\",\"revision\",\"engagement_version\",\"gpo_id\",\"computer_target_ou_id\" FROM organization_gpo_membership_claims",
  organization_gpo_rules: "SELECT \"id\",\"company_id\",\"target_kind\",\"target_id\",\"version\",\"revision\",\"connection_id\",\"gpo_id\",\"user_filter_group_id\",\"computer_target_ou_id\",\"active\",\"directory_config_status\",\"directory_config_error\",\"legacy_position_id\",\"last_actor_id\",\"last_reason\",\"updated_at\" FROM organization_gpo_rules",
  people_directory_membership_sources: "SELECT \"connection_id\",\"group_id\",\"member_id\",\"source_id\",\"application_id\",\"created_at\" FROM people_directory_membership_sources",
  position_gpo_policies: "SELECT \"company_id\",\"position_id\",\"version\",\"revision\",\"connection_id\",\"gpo_id\",\"user_filter_group_id\",\"computer_target_ou_id\",\"directory_config_status\",\"directory_config_error\",\"updated_at\" FROM position_gpo_policies",
} as const);
