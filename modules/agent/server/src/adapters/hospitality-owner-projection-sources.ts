import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  ai_workspace_turns: "SELECT \"id\",\"company_id\",\"user_id\",\"assignment_id\",\"scope_key\",\"conversation_key\",\"question\",\"paths_json\",\"sources_json\",\"answer_json\",\"state\",\"error\",\"created_at\",\"updated_at\",\"english_answer_json\",\"private_session_id\" FROM ai_workspace_turns",
} as const;
