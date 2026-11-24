import "server-only";
import { ATTACHMENTS_OWNER_PROJECTION_SQL } from "./owner-projection-dedupe-sql";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  attachment_links: "SELECT \"id\",\"set_id\",\"attachment_id\",\"upload_intent_id\",\"caption\",\"state\",\"supersedes_link_id\",\"created_by\",\"created_at\",\"expires_at\",\"bound_at\",\"binding_type\",\"binding_id\",\"withdrawn_at\",\"withdrawn_by\",\"withdrawal_reason\" FROM attachment_links",
  attachment_sets: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"target_type\",\"target_id\",\"component_kind\",\"component_ref\",\"purpose_key\",\"expected_target_version\",\"policy_version\",\"classification\",\"audience_json\",\"retention_policy_key\",\"retention_policy_version\",\"retain_until\",\"destruction_mode\",\"required_count\",\"max_files\",\"state\",\"created_by\",\"created_at\",\"updated_at\",\"bound_at\",\"bound_command_id\" FROM attachment_sets",
  attachments: ATTACHMENTS_OWNER_PROJECTION_SQL,
} as const;
