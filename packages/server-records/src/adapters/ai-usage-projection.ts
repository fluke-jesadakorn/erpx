/** Canonical owner-local generator for the Records request-AI table column
 * projections. r30 admits the table-and-tail column head once; each member passes
 * its table and optional tail and renders byte-identical prior SQL, and the
 * native owner states the same fixed selection. */
export const requestAiProjectionHead = (table: string, columns: string, tail = '') =>
  `SELECT ${columns} FROM ${table}${tail}`;
export const requestAiUsageProjection = (columns: string) => requestAiProjectionHead('request_ai_usage', columns);
export const requestAiUsageProjectionSql = requestAiUsageProjection('"id","company_id","actor_id","month","state","lease_until","input_tokens","output_tokens","created_at"');
export const recordsRequestUsageSql = requestAiUsageProjection('company_id,month');
/** Canonical owner-local fixed read for the company+month request-AI usage
 * count, shared by the agent quota guard and the beta-management usage source.
 * Both consumers previously carried their own duplicate literal. */
export const requestAiUsageMonthlyCountSql = 'SELECT count(*)::int count FROM request_ai_usage WHERE company_id=? AND month=?';
/** Canonical owner-local generator for Records request-AI proposal column
 * projections. The full read and the bounded drain claim render their exact
 * prior SQL byte-for-byte; only the selected columns and optional tail differ. */
export const requestAiProposalProjection = (columns: string, tail = '') =>
  requestAiProjectionHead('request_ai_proposals', columns, tail);
/** Canonical owner-local fixed projection for the Records request-AI proposal
 * read. Admitted once and shared by every Records proposal consumer; the native
 * owner states the same fixed selection. No duplicate TS literal remains. */
export const requestAiProposalProjectionSql = requestAiProposalProjection('id,company_id,request_id,source_version,provider_json,actor_json,state,result_json,error,attempts,lease_token,claim_until,created_at,updated_at');
