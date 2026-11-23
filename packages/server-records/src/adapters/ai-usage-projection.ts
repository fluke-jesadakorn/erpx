/** Canonical owner-local fixed SQL for the Records request AI usage projection.
 * Admitted once and shared by every Records projection consumer; the native
 * owner states the same fixed selection. No duplicate TS literal remains. */
export const requestAiUsageProjectionSql = "SELECT \"id\",\"company_id\",\"actor_id\",\"month\",\"state\",\"lease_until\",\"input_tokens\",\"output_tokens\",\"created_at\" FROM request_ai_usage";
export const recordsRequestUsageSql = 'SELECT company_id,month FROM request_ai_usage';
/** Canonical owner-local fixed read for the company+month request-AI usage
 * count, shared by the agent quota guard and the beta-management usage source.
 * Both consumers previously carried their own duplicate literal. */
export const requestAiUsageMonthlyCountSql = 'SELECT count(*)::int count FROM request_ai_usage WHERE company_id=? AND month=?';
/** Canonical owner-local fixed projection for the Records request-AI proposal
 * read. Admitted once and shared by every Records proposal consumer; the native
 * owner states the same fixed selection. No duplicate TS literal remains. */
export const requestAiProposalProjectionSql = "SELECT id,company_id,request_id,source_version,provider_json,actor_json,state,result_json,error,attempts,lease_token,claim_until,created_at,updated_at FROM request_ai_proposals";
