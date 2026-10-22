import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  handbook_editors: "SELECT \"company_id\",\"user_id\",\"granted_by\",\"granted_at\" FROM handbook_editors",
  social_post_sources: "SELECT \"post_id\",\"ordinal\",\"source_kind\",\"source_id\" FROM social_post_sources",
  social_posts: "SELECT \"id\",\"kind\",\"author_id\",\"author_name\",\"company_id\",\"branch_id\",\"body\",\"linked_module\",\"linked_record_id\",\"linked_record_label\",\"state\",\"version\",\"last_command_id\",\"created_at\",\"updated_at\",\"department_id\",\"mentions_json\",\"content_json\",\"audience_kind\",\"purpose\",\"edited_at\",\"retracted_at\",\"audience_id\",\"rich_content_json\" FROM social_posts",
  social_task_origins: "SELECT \"post_id\",\"task_id\",\"source_version\",\"source_hash\",\"ai_draft_id\",\"created_by\",\"created_at\" FROM social_task_origins",
  staff_notifications: "SELECT \"id\",\"company_id\",\"user_id\",\"record_type\",\"record_id\",\"record_version\",\"message\",\"created_at\",\"read_at\" FROM staff_notifications",
} as const;
