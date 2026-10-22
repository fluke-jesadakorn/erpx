import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  audit_events: "SELECT \"id\",\"command_id\",\"actor_id\",\"assignment_id\",\"action\",\"record_type\",\"record_id\",\"record_version\",\"purpose\",\"created_at\",\"authority_json\" FROM audit_events",
} as const;
